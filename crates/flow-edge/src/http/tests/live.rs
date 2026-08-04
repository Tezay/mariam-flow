use super::*;

#[tokio::test]
async fn an_estimate_published_with_nobody_listening_is_still_kept() {
    // The pipeline runs whether or not a browser is watching. A channel
    // that dropped values when unobserved would leave the appliance
    // reporting nothing to the first client to connect.
    let state = state_for(installed(), true);
    assert!(state.latest_estimate().is_none());

    state.publish_estimate(flow_infer::WaitEstimate {
        ts_us: 1_800_000_000_000_000,
        wait_minutes: 4.5,
        people: 12.0,
        level: 1.8,
        display_class: flow_core::DensityClass::Medium,
        confidence: 0.71,
        reliable: true,
    });

    let kept = state.latest_estimate().expect("kept without a subscriber");
    assert!((kept.wait_minutes - 4.5).abs() < 1e-6);

    let cookie = session_of(&state).await;
    let (status, _, body) = send(&state, "GET", "/api/status", Some(&cookie), None, 10).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["stream"]["running"], false, "no pipeline in this test");
}

#[tokio::test]
async fn the_waiting_time_is_published_without_a_session() {
    // The one thing the product exists to say, and the only route that
    // says it to anyone.
    let (state, _dir) = serving(estimate_at(now_us(), true));

    let (status, headers, body) = send(&state, "GET", "/estimate", None, None, 10).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
    assert_eq!(body["class"], "medium");
    assert!((body["wait_min"].as_f64().unwrap() - 6.5).abs() < 0.01);
    // A hall display is not hosted by the appliance.
    assert_eq!(
        headers.get(header::ACCESS_CONTROL_ALLOW_ORIGIN).unwrap(),
        "*"
    );
}

#[tokio::test]
async fn a_closed_site_is_not_reported_as_a_fault() {
    // Every display in every hall announcing a breakdown all night is how
    // nobody notices a real one.
    let (state, _dir) = serving(estimate_at(now_us(), true));
    let cookie = session_of(&state).await;
    send(
        &state,
        "PUT",
        "/api/service-window",
        Some(&cookie),
        Some(never_open().to_string()),
        10,
    )
    .await;

    let (_, _, body) = send(&state, "GET", "/estimate", None, None, 10).await;

    assert_eq!(body["status"], "closed");
    assert!(
        body.get("wait_min").is_none(),
        "a closed site has no number"
    );
}

#[tokio::test]
async fn a_doubtful_or_aged_number_is_never_published() {
    for (label, estimate) in [
        ("unreliable", estimate_at(now_us(), false)),
        (
            "stale",
            estimate_at(now_us() - 10 * PUBLIC_MAX_AGE_US, true),
        ),
    ] {
        let (state, _dir) = serving(estimate);
        let (_, _, body) = send(&state, "GET", "/estimate", None, None, 10).await;
        assert_eq!(body["status"], "unavailable", "{label} was published");
    }
}

#[tokio::test]
async fn the_public_route_never_carries_a_measurement() {
    // The privacy invariant, asserted rather than assumed: only the
    // aggregate leaves the appliance.
    let (state, _dir) = serving(estimate_at(now_us(), true));

    let (_, _, body) = send(&state, "GET", "/estimate", None, None, 10).await;

    let fields: Vec<&String> = body.as_object().unwrap().keys().collect();
    assert_eq!(
        fields,
        vec!["class", "confidence", "status", "ts_us", "wait_min"],
        "the public payload gained a field"
    );
}

#[tokio::test]
async fn the_live_stream_ends_when_the_daemon_is_asked_to_stop() {
    // Without this the stream never completes, so a graceful shutdown
    // waits for as long as one dashboard is open — until a supervisor
    // kills the process, taking the still-buffered journal with it.
    let (state, _dir) = writable();
    let cookie = session_of(&state).await;

    let mut request = axum::http::Request::builder()
        .method("GET")
        .uri("/api/live")
        .header(header::COOKIE, cookie)
        .body(axum::body::Body::empty())
        .unwrap();
    request
        .extensions_mut()
        .insert(ConnectInfo(SocketAddr::from((
            Ipv4Addr::new(192, 168, 4, 10),
            51_000,
        ))));
    let response = router(state.clone()).oneshot(request).await.unwrap();

    state.begin_shutdown();

    let drained = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        response.into_body().collect(),
    )
    .await;
    assert!(drained.is_ok(), "the live stream never ended");
}
