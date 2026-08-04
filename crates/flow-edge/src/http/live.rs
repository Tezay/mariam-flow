//! What the appliance is seeing now, and what it tells the world.

use axum::Json;
use axum::extract::{Query, State};
use axum::http::header;
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use tokio_stream::StreamExt;

use crate::edge_state::EdgeState;
use crate::history::{MINUTE_US, MinuteSummary};
use crate::now_us;

/// Streams the live state as server-sent events.
///
/// One-way and over plain HTTP, which is all this needs: the browser only
/// listens, and the built-in reconnection of `EventSource` covers a dropped
/// connection without a line of code. Keep-alives stop an idle appliance —
/// one whose queue has not changed — from looking dead to a proxy.
pub(super) async fn live(
    State(state): State<EdgeState>,
) -> Sse<impl tokio_stream::Stream<Item = Result<SseEvent, std::convert::Infallible>>> {
    // Driven by a tick as well as by new estimates. On estimates alone, the
    // stream falls silent in exactly the situations a watcher needs to hear
    // about: every sensor gone quiet produces no estimate, so the screen
    // would freeze on its last good state instead of reporting the silence —
    // and a capture, which suspends estimation entirely, would show a clock
    // that never advances.
    let estimates =
        tokio_stream::wrappers::WatchStream::new(state.watch_estimates()).map(|_| false);
    let ticks = tokio_stream::wrappers::IntervalStream::new(tokio::time::interval(
        std::time::Duration::from_secs(1),
    ))
    .map(|_| false);

    // Merged in as a third source rather than raced against the whole stream:
    // this response has to *end* when the daemon is asked to stop, or a
    // graceful shutdown waits for as long as one dashboard is left open.
    let stopping = tokio_stream::wrappers::WatchStream::new(state.watch_stopping());

    let stream = estimates
        .merge(ticks)
        .merge(stopping)
        .map_while(move |stopping| {
            if stopping {
                return None;
            }
            let event = SseEvent::default()
                .json_data(state.live_snapshot())
                .unwrap_or_else(|_| SseEvent::default().comment("snapshot unavailable"));
            Some(Ok(event))
        });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// How old an estimate may be before it stops being published.
///
/// An estimate outlives the window it was computed from: if the sensors have
/// gone quiet, the last good number stays true-looking for as long as nobody
/// replaces it. A queue changes on the scale of a minute, so anything older
/// than this describes a hall that has since emptied or filled.
pub(super) const PUBLIC_MAX_AGE_US: u64 = 90 * 1_000_000;

/// The waiting time as anyone may read it.
///
/// Three states rather than two. A site outside its service hours is not a
/// fault, and reporting it as one would leave every display in every hall
/// announcing a breakdown all night — after which nobody notices a real one.
#[derive(Serialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub(super) enum PublicEstimate {
    /// A fresh, reliable estimate during service.
    Ok {
        wait_min: f32,
        class: String,
        confidence: f32,
        ts_us: u64,
    },
    /// The site is not serving.
    Closed {
        /// When it next opens, if the schedule says.
        #[serde(skip_serializing_if = "Option::is_none")]
        opens_at_us: Option<u64>,
    },
    /// Serving, but with nothing worth showing.
    ///
    /// Carries no reason: this is the surface a hall display reads, and an
    /// explanation of *why* the appliance cannot estimate is an operator's
    /// business — the dashboard and the journal both say it.
    Unavailable {},
}

/// Publishes the waiting time, and nothing else.
///
/// The one route that answers without a session: it is what the product
/// exists to say. Raw measurement never appears here, only the aggregate the
/// privacy invariant allows to leave the appliance at all.
pub(super) async fn public_estimate(State(state): State<EdgeState>) -> Response {
    let service = state.service_state();
    let body = if service.open {
        match state.latest_estimate() {
            Some(estimate)
                if estimate.reliable
                    && now_us().saturating_sub(estimate.ts_us) <= PUBLIC_MAX_AGE_US =>
            {
                PublicEstimate::Ok {
                    wait_min: estimate.wait_minutes,
                    class: estimate.display_class.to_string(),
                    confidence: estimate.confidence,
                    ts_us: estimate.ts_us,
                }
            }
            _ => PublicEstimate::Unavailable {},
        }
    } else {
        PublicEstimate::Closed {
            opens_at_us: service.changes_at_us,
        }
    };

    // Readable from a page served anywhere: a hall display is not hosted by
    // the appliance. Only this route says so, it takes no credential, and it
    // publishes what is meant to be on a screen in the first place.
    ([(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")], Json(body)).into_response()
}

/// How far back `/api/estimates` reaches unless asked otherwise.
pub(super) const DEFAULT_HISTORY_MINUTES: u64 = 60;

/// Ceiling on the minutes one request may ask for, so a single call cannot
/// make the appliance serialize two years of history.
pub(super) const MAX_HISTORY_MINUTES: u64 = 7 * 24 * 60;

#[derive(Deserialize)]
pub(super) struct EstimatesQuery {
    minutes: Option<u64>,
}

pub(super) async fn estimates(
    State(state): State<EdgeState>,
    Query(query): Query<EstimatesQuery>,
) -> Json<Vec<MinuteSummary>> {
    let minutes = query
        .minutes
        .unwrap_or(DEFAULT_HISTORY_MINUTES)
        .clamp(1, MAX_HISTORY_MINUTES);
    let since = now_us().saturating_sub(minutes * MINUTE_US);
    Json(state.minutes(since, usize::try_from(minutes).unwrap_or(usize::MAX)))
}
