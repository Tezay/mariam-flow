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
    let (state, _dir) = writable_with(factory(), false);
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
async fn describing_the_queue_stores_the_words_and_the_counts_together() {
    let (state, _dir) = writable();
    let cookie = session_of(&state).await;

    let (status, body) = put(
        &state,
        "/api/queue",
        &cookie,
        json!({
            "classes": { "empty": "personne", "low": "quelques-uns",
                         "medium": "jusqu'aux colonnes", "saturated": "dehors" },
            "wait": { "people_per_class": [0.0, 4.0, 12.0, 25.0], "service_rate_per_min": 6.0 },
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["readiness"]["queue_described"], json!(true));
    assert_eq!(body["classes"]["medium"], json!("jusqu'aux colonnes"));
    assert_eq!(body["wait"]["service_rate_per_min"], json!(6.0));
}

#[tokio::test]
async fn a_queue_that_is_never_served_is_refused() {
    // Dividing a head count by zero people per minute is not a long wait, it
    // is no answer at all.
    let (state, _dir) = writable_with(factory(), false);
    let cookie = session_of(&state).await;

    let (status, _) = put(
        &state,
        "/api/queue",
        &cookie,
        json!({
            "wait": { "people_per_class": [0.0, 4.0, 12.0, 25.0], "service_rate_per_min": 0.0 },
        }),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(!state.status().readiness.queue_described);
}

#[tokio::test]
async fn an_installation_stops_at_the_queue_until_it_is_described() {
    // Without this step an appliance could finish installing and never turn a
    // density into a waiting time.
    let (state, _dir) = writable_with(factory(), false);
    let cookie = session_of(&state).await;
    put(&state, "/api/site", &cookie, json!({ "site_name": "RU" })).await;
    put(
        &state,
        "/api/nodes",
        &cookie,
        json!([
            { "node_id": "tx-1", "role": "tx", "mac": "1a:00:00:00:00:00" },
            { "node_id": "rx-1", "role": "rx", "address": "192.168.4.51" },
        ]),
    )
    .await;
    put(&state, "/api/uplink", &cookie, json!({ "mode": "offline" })).await;

    let (_, _, body) = send(&state, "GET", "/api/status", Some(&cookie), None, 10).await;

    assert_eq!(body["phase"]["stage"], json!("queue"));
}
