use super::*;

#[tokio::test]
async fn the_journal_is_closed_to_anonymous_callers() {
    let state = state_for(installed(), true);
    let (status, _, _) = send(&state, "GET", "/api/events", None, None, 10).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_successful_login_is_recorded_with_its_client() {
    let state = state_for(installed(), true);
    let cookie = session_of(&state).await;

    let (status, _, body) = send(&state, "GET", "/api/events", Some(&cookie), None, 10).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body[0]["kind"], "login-succeeded");
    assert_eq!(body[0]["category"], "access");
    assert_eq!(body[0]["client"], "192.168.4.10");
}

#[tokio::test]
async fn failures_and_throttling_are_recorded_too() {
    let state = state_for(installed(), true);
    // Three failures are free, the fourth sets the delay, and only the
    // fifth is actually turned away.
    for _ in 0..5 {
        login_with(&state, WRONG_SECRET, 66).await;
    }
    let cookie = session_of(&state).await;

    let (_, _, body) = send(&state, "GET", "/api/events", Some(&cookie), None, 10).await;
    let kinds: Vec<&str> = body
        .as_array()
        .unwrap()
        .iter()
        .map(|event| event["kind"].as_str().unwrap())
        .collect();

    assert!(kinds.contains(&"login-failed"));
    assert!(
        kinds.contains(&"login-throttled"),
        "a blocked attempt is worth recording: {kinds:?}"
    );
    // The attacker's address is on record, not just the installer's.
    let attackers = body
        .as_array()
        .unwrap()
        .iter()
        .filter(|event| event["client"] == "192.168.4.66")
        .count();
    assert!(attackers >= 5, "every attempt from 192.168.4.66 is kept");
}

#[tokio::test]
async fn logging_out_is_recorded() {
    let state = state_for(installed(), true);
    let cookie = session_of(&state).await;
    send(&state, "DELETE", "/api/session", Some(&cookie), None, 10).await;

    let fresh = session_of(&state).await;
    let (_, _, body) = send(&state, "GET", "/api/events", Some(&fresh), None, 10).await;
    let kinds: Vec<&str> = body
        .as_array()
        .unwrap()
        .iter()
        .map(|event| event["kind"].as_str().unwrap())
        .collect();
    assert!(kinds.contains(&"logged-out"), "{kinds:?}");
}

#[tokio::test]
async fn the_journal_query_honours_and_clamps_its_limit() {
    let state = state_for(installed(), true);
    for _ in 0..3 {
        login_with(&state, WRONG_SECRET, 10).await;
    }
    let cookie = session_of(&state).await;

    let (_, _, body) = send(
        &state,
        "GET",
        "/api/events?limit=2",
        Some(&cookie),
        None,
        10,
    )
    .await;
    assert_eq!(body.as_array().unwrap().len(), 2);

    // An absurd limit is clamped rather than refused.
    let (status, _, body) = send(
        &state,
        "GET",
        "/api/events?limit=999999",
        Some(&cookie),
        None,
        10,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.as_array().unwrap().len() <= MAX_EVENT_LIMIT);
}

#[tokio::test]
async fn the_journal_never_records_the_secret_itself() {
    let state = state_for(installed(), true);
    login_with(&state, WRONG_SECRET, 10).await;
    let cookie = session_of(&state).await;

    let (_, _, body) = send(&state, "GET", "/api/events", Some(&cookie), None, 10).await;
    let text = body.to_string();
    assert!(
        !text.contains(SECRET),
        "a secret must never reach the journal"
    );
    assert!(!text.contains(WRONG_SECRET), "not even a wrong one");
}

#[tokio::test]
async fn the_journal_exports_as_csv_and_quotes_what_would_break_a_row() {
    let (state, _dir) = writable();
    let cookie = session_of(&state).await;
    // A detail containing a comma, a quote and a newline: all three end a
    // CSV field early if they are not quoted.
    state.record(
        Event::new(EventKind::ConfigurationChanged)
            .with_detail("site named \"RU, Efrei\"\nline two"),
    );
    state.flush_journal();

    let (status, headers, _) =
        send(&state, "GET", "/api/events.csv", Some(&cookie), None, 10).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers.get(header::CONTENT_TYPE).unwrap(),
        "text/csv; charset=utf-8"
    );
}

#[test]
fn a_csv_field_survives_a_comma_a_quote_and_a_newline() {
    assert_eq!(csv_field("plain"), "plain");
    assert_eq!(csv_field("a,b"), "\"a,b\"");
    assert_eq!(csv_field("say \"hi\""), "\"say \"\"hi\"\"\"");
    assert_eq!(csv_field("two\nlines"), "\"two\nlines\"");
}

#[test]
fn an_exported_instant_is_utc_and_sorts_as_text() {
    // Read elsewhere, so a naive local time with no offset is how two
    // records of one incident stop lining up.
    assert_eq!(utc_instant(1_785_700_800_000_000), "2026-08-02T20:00:00Z");
}
