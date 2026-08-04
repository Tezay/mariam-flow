use super::*;

#[tokio::test]
async fn a_factory_appliance_reports_the_first_onboarding_step() {
    let state = state_for(factory(), false);
    let cookie = session_of(&state).await;
    let (_, _, body) = send(&state, "GET", "/api/status", Some(&cookie), None, 10).await;

    assert_eq!(body["phase"]["phase"], "onboarding");
    assert_eq!(body["phase"]["stage"], "site");
    assert_eq!(body["uplink"]["mode"], "undecided");
    assert_eq!(body["runtime"]["mode"], "idle");
    assert_eq!(body["sensor_ap"]["channel"], 6);
    assert!(body["nodes"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn an_installed_appliance_reports_operational_state() {
    let state = state_for(installed(), true);
    let cookie = session_of(&state).await;
    let (_, _, body) = send(&state, "GET", "/api/status", Some(&cookie), None, 10).await;

    assert_eq!(body["site_name"], "RU EFREI");
    assert_eq!(body["uplink"]["mode"], "wifi");
    assert_eq!(body["uplink"]["ssid"], "campus");
    assert_eq!(body["readiness"]["model_ready"], true);
    assert_eq!(body["nodes"][1]["address"], "192.168.4.51");
}

#[tokio::test]
async fn the_reported_runtime_mode_follows_the_stream_guard() {
    let state = state_for(installed(), true);
    let cookie = session_of(&state).await;
    state
        .with_runtime(|runtime| runtime.start_calibration("s-001", 0))
        .unwrap();

    let (_, _, body) = send(&state, "GET", "/api/status", Some(&cookie), None, 10).await;
    assert_eq!(body["runtime"]["mode"], "calibrating");
    assert_eq!(body["runtime"]["session_id"], "s-001");

    state.with_runtime(Runtime::stop);
    let (_, _, body) = send(&state, "GET", "/api/status", Some(&cookie), None, 10).await;
    assert_eq!(body["runtime"]["mode"], "idle");
}

#[tokio::test]
async fn readiness_is_reported_even_when_the_installation_is_closed() {
    let mut config = installed();
    config.nodes.clear();
    let state = state_for(config, true);
    let cookie = session_of(&state).await;

    assert_eq!(state.phase(), Phase::Operational);
    assert_eq!(state.readiness().stage(), Stage::Nodes);

    let (_, _, body) = send(&state, "GET", "/api/status", Some(&cookie), None, 10).await;
    assert_eq!(body["phase"]["phase"], "operational");
    assert_eq!(body["readiness"]["nodes_paired"], false);
}

#[tokio::test]
async fn naming_the_site_is_stored_and_journalled() {
    let (state, _dir) = writable_with(factory(), false);
    let cookie = session_of(&state).await;

    let (status, body) = put(
        &state,
        "/api/site",
        &cookie,
        json!({ "site_name": "  RU EFREI  " }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["site_name"], "RU EFREI");
    assert_eq!(
        stored(&state).identity.site_name.as_deref(),
        Some("RU EFREI")
    );
    assert!(journal_details(&state).contains("RU EFREI"));
}

#[tokio::test]
async fn a_blank_site_name_is_refused_and_nothing_is_stored() {
    let (state, _dir) = writable_with(factory(), false);
    let cookie = session_of(&state).await;

    let (status, body) = put(&state, "/api/site", &cookie, json!({ "site_name": "   " })).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].as_str().unwrap().contains("site_name"));
    assert!(stored(&state).identity.site_name.is_none());
}

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
async fn the_uplink_is_journalled_by_shape_never_by_passphrase() {
    // The journal is readable by anyone who can read the appliance, and
    // the status surface already refuses to report secrets. A write must
    // not be the way one escapes.
    let (state, _dir) = writable_with(factory(), false);
    let cookie = session_of(&state).await;
    let uplink = json!({
        "mode": "wifi",
        "ssid": "campus",
        "security": { "type": "wpa-personal", "passphrase": UPLINK_PASSPHRASE },
        "addressing": { "method": "dhcp" },
    });

    let (status, body) = put(&state, "/api/uplink", &cookie, uplink).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["readiness"]["uplink_decided"], true);
    assert_eq!(body["uplink"]["mode"], "wifi");
    let details = journal_details(&state);
    assert!(details.contains("campus"));
    assert!(
        !details.contains(UPLINK_PASSPHRASE),
        "the journal carries the passphrase"
    );
}

#[tokio::test]
async fn the_survey_outlives_the_decision_it_led_to() {
    // A site that demands 802.1X leaves the appliance offline. The reason
    // has to survive that, or the request sent to its network
    // administrator cannot be rebuilt later.
    let (state, _dir) = writable_with(factory(), false);
    let cookie = session_of(&state).await;

    let (status, body) = put(
        &state,
        "/api/network-survey",
        &cookie,
        json!({ "authentication": "account", "registration_required": true }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["survey"]["authentication"], "account");

    let (status, body) = put(&state, "/api/uplink", &cookie, json!({ "mode": "offline" })).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["uplink"]["mode"], "offline");
    assert_eq!(body["survey"]["authentication"], "account");

    let stored = stored(&state).network.survey.unwrap();
    assert!(stored.registration_required);
    assert!(!stored.authentication.joinable());
}

#[tokio::test]
async fn a_survey_of_an_ordinary_network_reports_it_as_joinable() {
    let (state, _dir) = writable_with(factory(), false);
    let cookie = session_of(&state).await;

    put(
        &state,
        "/api/network-survey",
        &cookie,
        json!({ "authentication": "shared-password" }),
    )
    .await;

    let survey = stored(&state).network.survey.unwrap();
    assert!(survey.authentication.joinable());
    assert!(!survey.registration_required);
    assert!(!survey.fixed_address);
}

#[tokio::test]
async fn the_survey_cannot_be_written_without_a_session() {
    let (state, _dir) = writable();
    let (status, _, _) = send(
        &state,
        "PUT",
        "/api/network-survey",
        None,
        Some(json!({ "authentication": "nothing" }).to_string()),
        10,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn reopening_the_uplink_question_is_not_the_same_as_going_offline() {
    let (state, _dir) = writable();
    let cookie = session_of(&state).await;

    let (status, body) = put(&state, "/api/uplink", &cookie, serde_json::Value::Null).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["readiness"]["uplink_decided"], false);
    assert!(stored(&state).network.uplink.is_none());
}

#[tokio::test]
async fn an_unfinished_installation_cannot_be_closed() {
    let (state, _dir) = writable_with(factory(), false);
    let cookie = session_of(&state).await;

    let (status, body) = put(
        &state,
        "/api/installation",
        &cookie,
        json!({ "completed": true }),
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT);
    assert!(
        body["error"]
            .as_str()
            .unwrap()
            .contains("site identification")
    );
    assert!(!stored(&state).onboarding_completed);
}

#[tokio::test]
async fn a_finished_installation_closes_and_can_be_reopened() {
    let mut config = installed();
    config.onboarding_completed = false;
    let (state, _dir) = writable_with(config, true);
    state.set_stream_running(true);
    let cookie = session_of(&state).await;

    // Recording a session is what finishes the installation; the model is
    // imported later, from the settings.
    post(&state, "/api/calibration", &cookie, json!({})).await;
    send(
        &state,
        "DELETE",
        "/api/calibration",
        Some(&cookie),
        None,
        10,
    )
    .await;

    let (status, body) = put(
        &state,
        "/api/installation",
        &cookie,
        json!({ "completed": true }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["phase"]["phase"], "operational");
    assert!(stored(&state).onboarding_completed);

    let (status, body) = put(
        &state,
        "/api/installation",
        &cookie,
        json!({ "completed": false }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["phase"]["phase"], "onboarding");
    assert!(!stored(&state).onboarding_completed);
}

#[tokio::test]
async fn the_onboarding_writes_are_closed_to_anonymous_callers() {
    let (state, _dir) = writable();
    for (path, body) in [
        ("/api/site", json!({ "site_name": "x" })),
        ("/api/nodes", json!([])),
        ("/api/uplink", serde_json::Value::Null),
        ("/api/installation", json!({ "completed": false })),
    ] {
        let (status, _, _) = send(&state, "PUT", path, None, Some(body.to_string()), 10).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{path} is open");
    }
}

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
