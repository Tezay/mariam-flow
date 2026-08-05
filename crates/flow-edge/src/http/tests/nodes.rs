use super::*;

#[tokio::test]
async fn a_confirmed_pairing_replaces_the_node_list() {
    let (state, _dir) = writable_with(factory(), false);
    let cookie = session_of(&state).await;
    let nodes = json!([
        { "node_id": "tx-1", "role": "tx", "mac": "1a:00:00:00:00:00" },
        { "node_id": "rx-1", "role": "rx", "mac": "aa:bb:cc:00:00:01", "address": "192.168.4.51" },
    ]);

    let (status, body) = put(&state, "/api/nodes", &cookie, nodes).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["readiness"]["nodes_paired"], true);
    assert_eq!(stored(&state).nodes.len(), 2);
}

#[tokio::test]
async fn a_pairing_the_appliance_cannot_use_is_refused_by_its_reason() {
    let (state, _dir) = writable_with(factory(), false);
    let cookie = session_of(&state).await;
    let two_transmitters = json!([
        { "node_id": "tx-1", "role": "tx", "mac": "1a:00:00:00:00:00" },
        { "node_id": "tx-2", "role": "tx", "mac": "1a:00:00:00:00:01" },
    ]);

    let (status, body) = put(&state, "/api/nodes", &cookie, two_transmitters).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    let message = body["error"].as_str().unwrap();
    assert!(message.contains("transmitter"), "unhelpful: {message}");
    assert!(!message.contains('/'), "leaks a path: {message}");
    assert!(stored(&state).nodes.is_empty());
}

#[tokio::test]
async fn discovery_is_closed_to_anonymous_callers() {
    let state = state_for(installed(), true);
    let (status, _, _) = send(&state, "GET", "/api/discovery", None, None, 10).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn discovery_offers_what_the_intake_has_seen() {
    let state = state_for(installed(), true);
    let cookie = session_of(&state).await;
    state.set_observations(vec![flow_ingest::SenderObservation {
        source: std::net::IpAddr::from([192, 168, 4, 53]),
        tx_macs: vec!["1a:00:00:00:00:00".parse().unwrap()],
        datagrams: 900,
        first_seen_us: now_us() - 9_000_000,
        last_seen_us: now_us(),
        sampled: 32,
    }]);

    let (status, _, body) = send(&state, "GET", "/api/discovery", Some(&cookie), None, 10).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["candidates"][0]["address"], "192.168.4.53");
    // rx-1 is already configured, so the box just plugged in is offered
    // the next free identifier rather than a colliding one.
    assert_eq!(body["proposal"]["receivers"][0]["node_id"], "rx-2");
    assert_eq!(body["proposal"]["tx_mac"], "1a:00:00:00:00:00");
}

#[tokio::test]
async fn a_node_is_told_where_it_sits_and_can_be_told_it_is_unknown_again() {
    let (state, _dir) = recording_ready();
    let cookie = session_of(&state).await;

    let (status, _, body) = send(
        &state,
        "PATCH",
        "/api/nodes/rx-1",
        Some(&cookie),
        Some(json!({ "position": "  above the entrance  " }).to_string()),
        10,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let placed = body["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["node_id"] == "rx-1")
        .unwrap()
        .clone();
    assert_eq!(placed["position"], "above the entrance");

    let (_, _, body) = send(
        &state,
        "PATCH",
        "/api/nodes/rx-1",
        Some(&cookie),
        Some(json!({ "position": "   " }).to_string()),
        10,
    )
    .await;
    // Blank is an answer here — "I do not know" — not a refusal.
    let cleared = body["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["node_id"] == "rx-1")
        .unwrap()
        .clone();
    assert!(cleared.get("position").is_none());
}

#[tokio::test]
async fn replacing_a_receiver_keeps_the_identifier_a_model_was_trained_for() {
    let (state, _dir) = recording_ready();
    let cookie = session_of(&state).await;
    send(
        &state,
        "PATCH",
        "/api/nodes/rx-1",
        Some(&cookie),
        Some(json!({ "position": "above the entrance" }).to_string()),
        10,
    )
    .await;

    let (status, _, body) = send(
        &state,
        "POST",
        "/api/nodes/rx-1/hardware",
        Some(&cookie),
        Some(json!({ "address": "192.168.4.57" }).to_string()),
        10,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let node = body["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["node_id"] == "rx-1")
        .unwrap()
        .clone();
    assert_eq!(node["address"], "192.168.4.57");
    // What the swap exists to protect: the identifier is what sessions
    // are written against and what a model was validated for.
    assert_eq!(node["node_id"], "rx-1");
    assert_eq!(node["position"], "above the entrance");
}

#[tokio::test]
async fn each_role_is_adopted_by_the_one_thing_that_identifies_it() {
    let (state, _dir) = recording_ready();
    let cookie = session_of(&state).await;

    for (node_id, body) in [
        ("rx-1", json!({ "mac": "aa:bb:cc:00:00:09" })),
        ("tx-1", json!({ "address": "192.168.4.57" })),
    ] {
        let (status, _, _) = send(
            &state,
            "POST",
            &format!("/api/nodes/{node_id}/hardware"),
            Some(&cookie),
            Some(body.to_string()),
            10,
        )
        .await;
        assert_eq!(
            status,
            StatusCode::BAD_REQUEST,
            "{node_id} accepted the wrong kind"
        );
    }
}

#[tokio::test]
async fn adopting_an_address_another_node_holds_is_refused() {
    let (state, _dir) = recording_ready();
    let cookie = session_of(&state).await;
    send(
        &state,
        "PUT",
        "/api/nodes",
        Some(&cookie),
        Some(
            json!([
                { "node_id": "rx-1", "role": "rx", "address": "192.168.4.51" },
                { "node_id": "rx-2", "role": "rx", "address": "192.168.4.52" },
            ])
            .to_string(),
        ),
        10,
    )
    .await;

    let (status, _, body) = send(
        &state,
        "POST",
        "/api/nodes/rx-2/hardware",
        Some(&cookie),
        Some(json!({ "address": "192.168.4.51" }).to_string()),
        10,
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        body["error"].as_str().unwrap().contains("192.168.4.51"),
        "{body} does not name the clash"
    );
}

#[tokio::test]
async fn a_node_the_appliance_does_not_have_is_not_found() {
    let (state, _dir) = recording_ready();
    let cookie = session_of(&state).await;
    for (method, path, body) in [
        ("PATCH", "/api/nodes/rx-9", json!({ "position": "nowhere" })),
        (
            "POST",
            "/api/nodes/rx-9/hardware",
            json!({ "address": "192.168.4.57" }),
        ),
    ] {
        let (status, _, _) = send(
            &state,
            method,
            path,
            Some(&cookie),
            Some(body.to_string()),
            10,
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{method} {path}");
    }
}

#[tokio::test]
async fn the_node_surface_is_closed_to_anonymous_callers() {
    let (state, _dir) = writable();
    for (method, path) in [
        ("PUT", "/api/nodes"),
        ("PATCH", "/api/nodes/rx-1"),
        ("POST", "/api/nodes/rx-1/hardware"),
    ] {
        let (status, _, _) = send(&state, method, path, None, Some("{}".to_owned()), 10).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {path} is open");
    }
}
