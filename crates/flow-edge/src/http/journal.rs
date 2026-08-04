//! Reading the appliance journal, on screen or as a file.

use axum::Json;
use axum::extract::{Query, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;

use crate::edge_state::EdgeState;
use crate::journal::{EVENT_CATEGORIES, EventCategory, EventPage, RecordedEvent};

/// How many journal entries `/api/events` returns unless asked otherwise.
pub(super) const DEFAULT_EVENT_LIMIT: usize = 100;

/// Ceiling on that, so one request cannot ask the appliance to serialize
/// its whole journal.
pub(super) const MAX_EVENT_LIMIT: usize = 1_000;

#[derive(Deserialize)]
pub(super) struct EventsQuery {
    limit: Option<usize>,
    /// Read further back: only rows older than this one.
    before: Option<i64>,
    /// Read forward: only rows newer than this one.
    ///
    /// This is what tells a reader how much has arrived while they were
    /// reading, without the list moving under them.
    after: Option<i64>,
    /// Restrict to one family.
    category: Option<String>,
}

pub(super) async fn events(
    State(state): State<EdgeState>,
    Query(query): Query<EventsQuery>,
) -> Json<Vec<RecordedEvent>> {
    // An unknown family reads as no filter rather than as an error: it can
    // only come from a hand-written URL, and an empty journal would look like
    // an appliance that has never done anything.
    let category = query
        .category
        .as_deref()
        .and_then(|name| EVENT_CATEGORIES.into_iter().find(|c| c.as_str() == name));

    Json(
        state.recent_events(&EventPage {
            limit: query
                .limit
                .unwrap_or(DEFAULT_EVENT_LIMIT)
                .clamp(1, MAX_EVENT_LIMIT),
            before: query.before,
            after: query.after,
            category,
        }),
    )
}

/// How many rows are held at once while an export is written.
///
/// The whole journal is 20 000 rows; reading it in slices keeps the row buffer
/// small whatever the retention grows to, and the text it produces is under
/// two megabytes.
pub(super) const CSV_SLICE: usize = 2_000;

#[derive(Deserialize)]
pub(super) struct CsvQuery {
    category: Option<String>,
}

/// Sends the journal as CSV, honouring the family filter.
///
/// The client address is included: an access incident forwarded to whoever
/// handles it is not usable without saying where it came from. It is personal
/// data, which is why the journal bounds its retention (ADR 0012) — an export
/// takes it off the appliance, and that is the operator's decision to make.
pub(super) async fn events_csv(
    State(state): State<EdgeState>,
    Query(query): Query<CsvQuery>,
) -> Response {
    let category = query
        .category
        .as_deref()
        .and_then(|name| EVENT_CATEGORIES.into_iter().find(|c| c.as_str() == name));

    let mut out = String::from("time,category,kind,client,detail\n");
    let mut before = None;
    loop {
        let slice = state.recent_events(&EventPage {
            limit: CSV_SLICE,
            before,
            after: None,
            category,
        });
        let Some(last) = slice.last() else { break };
        before = Some(last.id);
        for event in &slice {
            out.push_str(&csv_row(event));
        }
        if slice.len() < CSV_SLICE {
            break;
        }
    }

    (
        [
            (header::CONTENT_TYPE, "text/csv; charset=utf-8".to_owned()),
            (
                header::CONTENT_DISPOSITION,
                format!(
                    "attachment; filename=\"journal-{}.csv\"",
                    category.map_or("all", EventCategory::as_str)
                ),
            ),
        ],
        out,
    )
        .into_response()
}

pub(super) fn csv_row(event: &RecordedEvent) -> String {
    format!(
        "{},{},{},{},{}\n",
        csv_field(&utc_instant(event.ts_us)),
        event.category.as_str(),
        event.kind.as_str(),
        csv_field(event.client.as_deref().unwrap_or_default()),
        csv_field(event.detail.as_deref().unwrap_or_default()),
    )
}

/// Quotes a field so that a comma, a quote or a newline in an operator's own
/// words cannot end the field early — RFC 4180, doubling the quote.
/// The instant an event carries, in UTC and in a form a spreadsheet sorts.
///
/// UTC rather than the site's zone: an export is read elsewhere, and a naive
/// local time with no offset is the classic way two records of the same
/// incident stop lining up.
pub(super) fn utc_instant(ts_us: u64) -> String {
    let micros = i64::try_from(ts_us).unwrap_or(0);
    jiff::Timestamp::from_microsecond(micros).map_or_else(
        |_| ts_us.to_string(),
        |ts| ts.strftime("%Y-%m-%dT%H:%M:%SZ").to_string(),
    )
}

pub(super) fn csv_field(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}
