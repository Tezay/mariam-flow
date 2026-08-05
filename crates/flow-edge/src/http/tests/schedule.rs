use super::*;

#[tokio::test]
async fn a_schedule_is_stored_and_answered_with_the_resulting_state() {
    let (state, _dir) = writable();
    let cookie = session_of(&state).await;
    let body = always_open().to_string();

    let (status, _, answer) = send(
        &state,
        "PUT",
        "/api/service-window",
        Some(&cookie),
        Some(body),
        10,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(answer["open"], true);
    // Reloaded from disk, not from the copy held in memory: the point of
    // the write is that it survives a restart.
    let path = state.config_path();
    let stored = ApplianceConfig::load(&path).unwrap();
    assert_eq!(stored.service.unwrap().timezone, "Europe/Paris");
}

#[tokio::test]
async fn the_stored_schedule_can_be_read_back_for_editing() {
    let (state, _dir) = writable();
    let cookie = session_of(&state).await;

    // Nothing declared yet: the screen that edits hours has to tell an
    // appliance with no schedule from one it failed to read.
    let (status, _, body) = send(
        &state,
        "GET",
        "/api/service-window",
        Some(&cookie),
        None,
        10,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.is_null());

    send(
        &state,
        "PUT",
        "/api/service-window",
        Some(&cookie),
        Some(always_open().to_string()),
        10,
    )
    .await;

    let (status, _, body) = send(
        &state,
        "GET",
        "/api/service-window",
        Some(&cookie),
        None,
        10,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["timezone"], "Europe/Paris");
    assert_eq!(body["weekly"]["monday"][0]["from"], "00:00");
}

#[tokio::test]
async fn the_schedule_cannot_be_read_without_a_session() {
    let (state, _dir) = writable();
    let (status, _, _) = send(&state, "GET", "/api/service-window", None, None, 10).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn clearing_the_schedule_returns_the_appliance_to_estimating() {
    let (state, _dir) = writable();
    let cookie = session_of(&state).await;
    let body = always_open().to_string();
    send(
        &state,
        "PUT",
        "/api/service-window",
        Some(&cookie),
        Some(body),
        10,
    )
    .await;

    let (status, _, answer) = send(
        &state,
        "PUT",
        "/api/service-window",
        Some(&cookie),
        Some("null".to_owned()),
        10,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(answer["open"], true);
    let path = state.config_path();
    assert!(ApplianceConfig::load(&path).unwrap().service.is_none());
}

#[tokio::test]
async fn a_refused_schedule_leaves_the_stored_one_untouched() {
    let (state, _dir) = writable();
    let cookie = session_of(&state).await;
    let body = always_open().to_string();
    send(
        &state,
        "PUT",
        "/api/service-window",
        Some(&cookie),
        Some(body),
        10,
    )
    .await;

    let unknown_zone =
        serde_json::json!({ "timezone": "Mars/Olympus", "weekly": {}, "closures": [] }).to_string();
    let (status, _, _) = send(
        &state,
        "PUT",
        "/api/service-window",
        Some(&cookie),
        Some(unknown_zone),
        10,
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    let path = state.config_path();
    let stored = ApplianceConfig::load(&path).unwrap();
    assert_eq!(stored.service.unwrap().timezone, "Europe/Paris");
}

#[tokio::test]
async fn the_schedule_cannot_be_set_without_a_session() {
    let (state, _dir) = writable();
    let body = always_open().to_string();

    let (status, _, _) = send(&state, "PUT", "/api/service-window", None, Some(body), 10).await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let path = state.config_path();
    assert!(ApplianceConfig::load(&path).unwrap().service.is_none());
}
