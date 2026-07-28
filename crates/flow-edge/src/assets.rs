//! Serving the dashboard the daemon carries.
//!
//! The appliance ships as one artifact. The built dashboard is compiled into
//! the binary rather than laid beside it, so there is no second thing to
//! copy onto a card, no way for the two to drift apart, and nothing to serve
//! from a directory that might be missing.
//!
//! # The `dashboard` feature
//!
//! Embedding is behind a Cargo feature, off by default, because the frontend
//! is built by a separate toolchain: the crate has to compile, test and be
//! developed on a machine with no Node installed, and on a clean clone where
//! nothing has been built yet. A release turns the feature on after
//! `pnpm build`; without it the daemon serves a notice saying so, which is a
//! development state rather than a deployment one.
//!
//! With the feature on, debug builds read the files from disk instead of
//! embedding them, so rebuilding the dashboard does not mean recompiling
//! Rust.
//!
//! # Routing
//!
//! The dashboard is a single-page application: the browser may ask for
//! `/settings` directly, and only the application itself knows that this is
//! a route rather than a file. Anything that does not match an embedded file
//! therefore falls back to `index.html`, and the application resolves the
//! path once it is running. Requests under `/api` never reach here — they
//! are matched by the API router first — so a mistyped endpoint still fails
//! as an endpoint rather than silently returning a web page.

use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};

/// The built dashboard, as produced by `pnpm build`.
#[cfg(feature = "dashboard")]
mod bundled {
    use rust_embed::Embed;

    #[derive(Embed)]
    #[folder = "$CARGO_MANIFEST_DIR/../../dashboard/build"]
    struct Dashboard;

    pub(super) fn read(path: &str) -> Option<Vec<u8>> {
        Dashboard::get(path).map(|file| file.data.into_owned())
    }
}

/// Stand-in when the crate is built without the dashboard.
#[cfg(not(feature = "dashboard"))]
mod bundled {
    pub(super) fn read(_path: &str) -> Option<Vec<u8>> {
        None
    }
}

/// Entry point of the single-page application.
const INDEX: &str = "index.html";

/// Serves an embedded asset, falling back to the application shell.
pub(crate) async fn serve(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { INDEX } else { path };

    if let Some(response) = embedded(path) {
        return response;
    }
    // A path that is not a file is a client-side route; the shell resolves
    // it. Only a genuinely missing shell is an error.
    match embedded(INDEX) {
        Some(response) => response,
        None => not_built(),
    }
}

fn embedded(path: &str) -> Option<Response> {
    let file = bundled::read(path)?;
    let mime = mime_of(path);
    Some(
        (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, mime),
                // Hashed asset names make the built files safe to cache hard;
                // the shell itself must not be, or a dashboard update would
                // never reach a browser that already has one.
                (
                    header::CACHE_CONTROL,
                    if path == INDEX {
                        "no-cache"
                    } else {
                        "public, max-age=31536000, immutable"
                    },
                ),
            ],
            file,
        )
            .into_response(),
    )
}

/// Answers when this build carries no dashboard.
///
/// A development state, not a deployment one, so it says exactly what to do
/// rather than returning a bare 404. Both causes lead here: the feature was
/// off, or it was on but nothing had been built.
fn not_built() -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        "This build carries no dashboard.\n\n\
         Build it:      cd dashboard && pnpm install && pnpm build\n\
         Embed it:      cargo build --release -p flow-edge --features dashboard\n",
    )
        .into_response()
}

/// Content type from the file extension.
///
/// Only the handful of types a built SvelteKit application emits; anything
/// else is served as bytes rather than guessed at.
fn mime_of(path: &str) -> &'static str {
    match path.rsplit_once('.').map(|(_, extension)| extension) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        Some("ico") => "image/x-icon",
        Some("woff2") => "font/woff2",
        Some("woff") => "font/woff",
        Some("txt") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_extensions_are_typed_and_unknown_ones_are_bytes() {
        assert_eq!(mime_of("index.html"), "text/html; charset=utf-8");
        assert_eq!(
            mime_of("_app/immutable/entry/app.CxK1.js"),
            "text/javascript; charset=utf-8"
        );
        assert_eq!(
            mime_of("_app/immutable/assets/0.DqZ2.css"),
            "text/css; charset=utf-8"
        );
        assert_eq!(mime_of("inter-latin-wght-normal.woff2"), "font/woff2");
        assert_eq!(mime_of("favicon.ico"), "image/x-icon");
        assert_eq!(mime_of("noextension"), "application/octet-stream");
        assert_eq!(mime_of("archive.tar.gz"), "application/octet-stream");
    }

    #[tokio::test]
    async fn an_unbuilt_dashboard_says_what_to_do() {
        // Whatever this build embeds, the message must never be a bare 404
        // that leaves a developer guessing.
        let response = not_built();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn any_unknown_path_resolves_to_the_shell_or_the_notice() {
        // Client-side routes must not 404: the shell owns the path.
        let response = serve("/settings/network".parse::<Uri>().unwrap()).await;
        assert_ne!(response.status(), StatusCode::NOT_FOUND);
    }
}
