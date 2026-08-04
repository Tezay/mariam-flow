use super::*;

#[tokio::test]
async fn health_needs_no_session() {
    let state = state_for(factory(), false);
    let (status, _, body) = send(&state, "GET", "/health", None, None, 10).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
}

#[tokio::test]
async fn the_status_surface_is_closed_to_anonymous_callers() {
    let state = state_for(installed(), true);
    let (status, _, body) = send(&state, "GET", "/api/status", None, None, 10).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"], "authentication required");
    assert!(body.get("kit_id").is_none(), "nothing leaks before login");
}

#[tokio::test]
async fn a_forged_or_stale_cookie_is_refused() {
    let state = state_for(installed(), true);
    for cookie in [
        "mf_session=deadbeef",
        "mf_session=",
        "other=value",
        "mf_session",
    ] {
        let (status, _, _) = send(&state, "GET", "/api/status", Some(cookie), None, 10).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "cookie {cookie:?}");
    }
}

#[tokio::test]
async fn the_right_secret_opens_a_session_that_unlocks_the_surface() {
    let state = state_for(installed(), true);
    let cookie = session_of(&state).await;

    let (status, _, body) = send(&state, "GET", "/api/status", Some(&cookie), None, 10).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["kit_id"], "KIT-0001");
    assert_eq!(body["phase"]["phase"], "operational");
}

#[tokio::test]
async fn the_session_cookie_is_defended_against_scripts_and_other_sites() {
    let state = state_for(installed(), true);
    let (_, headers) = login_with(&state, SECRET, 10).await;
    let cookie = headers.get(header::SET_COOKIE).unwrap().to_str().unwrap();

    assert!(cookie.contains("HttpOnly"), "unreadable by scripts");
    assert!(cookie.contains("SameSite=Strict"), "no cross-site riding");
    assert!(cookie.contains("Path=/"));
    assert!(
        !cookie.contains(SECRET),
        "the cookie must carry a token, never the secret"
    );
}

#[tokio::test]
async fn the_secret_is_accepted_however_it_was_typed() {
    let state = state_for(installed(), true);
    let (status, _) = login_with(&state, "k7m49pqr2wxy6btn3hfd", 10).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn a_wrong_secret_is_refused_without_a_session() {
    let state = state_for(installed(), true);
    let (status, headers) = login_with(&state, WRONG_SECRET, 10).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(
        headers.get(header::SET_COOKIE).is_none(),
        "no session handed out"
    );
}

#[tokio::test]
async fn repeated_failures_start_costing_time() {
    let state = state_for(installed(), true);

    // The first few failures are free — mistyping happens.
    for _ in 0..3 {
        let (status, _) = login_with(&state, WRONG_SECRET, 20).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
    // Then the client is told to wait.
    let (status, headers) = login_with(&state, WRONG_SECRET, 20).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(headers.get(header::RETRY_AFTER).unwrap(), "1");

    // And while blocked, even the correct secret has to wait its turn —
    // otherwise the block would be trivially bypassed.
    let (status, headers) = login_with(&state, SECRET, 20).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert!(headers.get(header::SET_COOKIE).is_none());
}

#[tokio::test]
async fn one_client_being_throttled_never_locks_out_another() {
    let state = state_for(installed(), true);
    for _ in 0..8 {
        login_with(&state, WRONG_SECRET, 66).await;
    }
    let (blocked, _) = login_with(&state, SECRET, 66).await;
    assert_eq!(blocked, StatusCode::TOO_MANY_REQUESTS);

    let (installer, _) = login_with(&state, SECRET, 10).await;
    assert_eq!(
        installer,
        StatusCode::NO_CONTENT,
        "the installer must still get in"
    );
}

#[tokio::test]
async fn logging_out_revokes_the_session_and_clears_the_cookie() {
    let state = state_for(installed(), true);
    let cookie = session_of(&state).await;

    let (status, headers, _) =
        send(&state, "DELETE", "/api/session", Some(&cookie), None, 10).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let cleared = headers.get(header::SET_COOKIE).unwrap().to_str().unwrap();
    assert!(cleared.contains("Max-Age=0"));

    let (status, _, _) = send(&state, "GET", "/api/status", Some(&cookie), None, 10).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "the token is dead");

    // Logging out again is harmless.
    let (status, _, _) = send(&state, "DELETE", "/api/session", Some(&cookie), None, 10).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn sessions_are_independent_of_one_another() {
    let state = state_for(installed(), true);
    let first = session_of(&state).await;
    let second = session_of(&state).await;
    assert_ne!(first, second);

    send(&state, "DELETE", "/api/session", Some(&first), None, 10).await;

    let (status, _, _) = send(&state, "GET", "/api/status", Some(&second), None, 10).await;
    assert_eq!(status, StatusCode::OK, "one logout must not end the others");
}

#[tokio::test]
async fn the_session_cookie_is_found_among_others() {
    let state = state_for(installed(), true);
    let cookie = session_of(&state).await;
    let mixed = format!("theme=dark; {cookie}; lang=fr");

    let (status, _, _) = send(&state, "GET", "/api/status", Some(&mixed), None, 10).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn the_status_surface_never_leaks_credentials() {
    let state = state_for(installed(), true);
    let cookie = session_of(&state).await;
    let (_, _, body) = send(&state, "GET", "/api/status", Some(&cookie), None, 10).await;
    let text = body.to_string();

    assert!(!text.contains(AP_PASSPHRASE), "sensor AP passphrase leaked");
    assert!(
        !text.contains(UPLINK_PASSPHRASE),
        "uplink passphrase leaked"
    );
    assert!(!text.contains("passphrase"), "no passphrase field at all");
    assert!(!text.contains("argon2"), "no credential material");
}

#[tokio::test]
async fn every_response_carries_the_browser_protections() {
    let state = state_for(installed(), true);
    // The probe, a refused request and an authenticated one: the
    // headers must not depend on the outcome.
    let cookie = session_of(&state).await;
    for (path, cookie) in [
        ("/health", None),
        ("/api/status", None),
        ("/api/status", Some(cookie.as_str())),
    ] {
        let (_, headers, _) = send(&state, "GET", path, cookie, None, 10).await;
        assert_eq!(
            headers.get("x-content-type-options").unwrap(),
            "nosniff",
            "{path}"
        );
        assert_eq!(headers.get("x-frame-options").unwrap(), "DENY", "{path}");
        assert_eq!(
            headers.get("referrer-policy").unwrap(),
            "no-referrer",
            "{path}"
        );
    }
}

#[tokio::test]
async fn a_refusal_says_what_is_wrong_and_names_no_server_path() {
    let (state, _dir) = writable();
    let cookie = session_of(&state).await;
    let overlapping = serde_json::json!({
        "timezone": "Europe/Paris",
        "weekly": { "monday": [
            { "from": "08:00", "to": "12:00" },
            { "from": "11:00", "to": "14:00" },
        ] },
        "closures": [],
    })
    .to_string();

    let (status, _, body) = send(
        &state,
        "PUT",
        "/api/service-window",
        Some(&cookie),
        Some(overlapping),
        10,
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    let message = body["error"].as_str().unwrap();
    // Someone is typing opening hours into a form: the answer has to name
    // the day that is wrong. Reporting that the configuration as a whole
    // was refused — and naming the file it was refused for — tells them
    // nothing and hands out a server path.
    assert!(message.contains("monday"), "unhelpful message: {message}");
    assert!(!message.contains('/'), "leaks a path: {message}");
}
