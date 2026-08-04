use super::*;

#[tokio::test]
async fn a_capture_claims_the_stream_and_seals_its_directory() {
    let (state, dir) = recording_ready();
    let cookie = session_of(&state).await;

    let (status, body) = post(&state, "/api/calibration", &cookie, json!({})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["runtime"]["mode"], "calibrating");
    assert!(state.is_recording());

    // While recording, the directory carries the suffix that tells a
    // truncated capture from a clean one.
    let sessions = dir.path().join("sessions");
    let recording: Vec<_> = std::fs::read_dir(&sessions)
        .unwrap()
        .filter_map(|entry| Some(entry.ok()?.file_name().to_string_lossy().into_owned()))
        .collect();
    assert_eq!(recording.len(), 1);
    assert!(recording[0].ends_with(".recording"), "{recording:?}");

    let (status, _, body) = send(
        &state,
        "DELETE",
        "/api/calibration",
        Some(&cookie),
        None,
        10,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["runtime"]["mode"], "idle");
    assert!(!state.is_recording());

    let sealed: Vec<_> = std::fs::read_dir(&sessions)
        .unwrap()
        .filter_map(|entry| Some(entry.ok()?.file_name().to_string_lossy().into_owned()))
        .collect();
    assert!(!sealed[0].ends_with(".recording"), "{sealed:?}");
}

#[tokio::test]
async fn what_the_classes_mean_travels_with_the_session() {
    // Answered once for the site, but each recorded session carries a
    // copy: the stored format has to stay readable on its own, long after
    // the appliance that produced it.
    let (state, dir) = recording_ready();
    let cookie = session_of(&state).await;
    put(
        &state,
        "/api/classes",
        &cookie,
        json!({
            "empty": "personne",
            "low": "quelques personnes",
            "medium": "file constituée",
            "saturated": "file au-delà de la porte",
        }),
    )
    .await;

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

    let session = std::fs::read_dir(dir.path().join("sessions"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let meta: serde_json::Value =
        serde_json::from_slice(&std::fs::read(session.join("meta.json")).unwrap()).unwrap();
    assert_eq!(
        meta["class_mapping"]["saturated"],
        "file au-delà de la porte"
    );
}

#[tokio::test]
async fn a_second_capture_is_refused_while_one_is_running() {
    // The stream has one consumer at a time; the refusal is what keeps a
    // running capture from being cut short by a stray request.
    let (state, _dir) = recording_ready();
    let cookie = session_of(&state).await;
    post(&state, "/api/calibration", &cookie, json!({})).await;

    let (status, _) = post(&state, "/api/calibration", &cookie, json!({})).await;

    assert_eq!(status, StatusCode::CONFLICT);
    assert!(state.is_recording(), "the running capture survived");
}

#[tokio::test]
async fn a_capture_is_refused_while_nothing_is_being_read() {
    // A capture started with no stream records labels against no frames,
    // and whoever is labelling finds out an hour later.
    let (state, _dir) = writable();
    let cookie = session_of(&state).await;
    state.set_stream_running(false);

    let (status, body) = post(&state, "/api/calibration", &cookie, json!({})).await;

    assert_eq!(status, StatusCode::CONFLICT);
    assert!(body["error"].as_str().unwrap().contains("stream"));
    assert!(!state.is_recording());
}

#[tokio::test]
async fn a_capture_needs_a_receiver_to_record_anything() {
    let (state, _dir) = writable_with(factory(), false);
    let cookie = session_of(&state).await;

    let (status, body) = post(&state, "/api/calibration", &cookie, json!({})).await;

    assert_eq!(status, StatusCode::CONFLICT);
    assert!(body["error"].as_str().unwrap().contains("receiver"));
}

#[tokio::test]
async fn labels_are_stamped_by_the_appliance_and_refused_without_a_capture() {
    let (state, _dir) = recording_ready();
    let cookie = session_of(&state).await;

    // Classes travel as the integers the canonical label format freezes,
    // not as names. The labelling phone and the appliance are also two
    // machines, so the timestamp is the appliance's.
    let (status, _) = post(
        &state,
        "/api/calibration/label",
        &cookie,
        json!({ "class": 1 }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);

    post(&state, "/api/calibration", &cookie, json!({})).await;
    let (status, _) = post(
        &state,
        "/api/calibration/label",
        &cookie,
        json!({ "class": 1 }),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn a_session_identifier_that_leaves_the_sessions_root_is_refused() {
    // The identifier names a directory. A value that could climb out of
    // the root is not a mistyped session, it is not a session at all —
    // refused rather than sanitised into something that looks fine.
    let (state, _dir) = recording_ready();
    let cookie = session_of(&state).await;

    for hostile in ["..", "../../etc", "a/b", "..%2Fetc"] {
        let path = format!("/api/sessions/{hostile}/archive");
        let (status, _, _) = send(&state, "GET", &path, Some(&cookie), None, 10).await;
        assert_ne!(status, StatusCode::OK, "{hostile} was served");

        let path = format!("/api/sessions/{hostile}");
        let (status, _, _) = send(&state, "DELETE", &path, Some(&cookie), None, 10).await;
        assert_ne!(status, StatusCode::NO_CONTENT, "{hostile} was deleted");
    }
}

#[tokio::test]
async fn an_unknown_session_is_not_found_rather_than_an_error() {
    let (state, _dir) = recording_ready();
    let cookie = session_of(&state).await;

    let (status, _, _) = send(
        &state,
        "GET",
        "/api/sessions/kit-0042-20260731T140000Z/archive",
        Some(&cookie),
        None,
        10,
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_recorded_session_is_listed_then_downloadable_then_removable() {
    let (state, dir) = recording_ready();
    let cookie = session_of(&state).await;
    post(
        &state,
        "/api/calibration",
        &cookie,
        json!({ "environment": "midi" }),
    )
    .await;
    send(
        &state,
        "DELETE",
        "/api/calibration",
        Some(&cookie),
        None,
        10,
    )
    .await;

    let (_, _, listed) = send(&state, "GET", "/api/sessions", Some(&cookie), None, 10).await;
    let id = listed[0]["session_id"].as_str().unwrap().to_owned();
    assert_eq!(listed[0]["environment"], "midi");
    assert_eq!(listed[0]["sealed"], true);

    let (status, headers, _) = send(
        &state,
        "GET",
        &format!("/api/sessions/{id}/archive"),
        Some(&cookie),
        None,
        10,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers.get(header::CONTENT_TYPE).unwrap(),
        "application/gzip"
    );
    // The archive is streamed from a file that is unlinked immediately, so
    // nothing half-built survives in the sessions directory.
    let leftovers: Vec<_> = std::fs::read_dir(dir.path().join("sessions"))
        .unwrap()
        .flatten()
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "gz"))
        .collect();
    assert!(leftovers.is_empty(), "an archive was left behind");

    let (status, _, _) = send(
        &state,
        "DELETE",
        &format!("/api/sessions/{id}"),
        Some(&cookie),
        None,
        10,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, _, listed) = send(&state, "GET", "/api/sessions", Some(&cookie), None, 10).await;
    assert_eq!(listed.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn a_renamed_session_keeps_its_identifier_and_is_downloaded_under_its_new_name() {
    let (state, _dir) = recording_ready();
    let cookie = session_of(&state).await;
    post(
        &state,
        "/api/calibration",
        &cookie,
        json!({ "environment": "midi" }),
    )
    .await;
    send(
        &state,
        "DELETE",
        "/api/calibration",
        Some(&cookie),
        None,
        10,
    )
    .await;
    let (_, _, listed) = send(&state, "GET", "/api/sessions", Some(&cookie), None, 10).await;
    let id = listed[0]["session_id"].as_str().unwrap().to_owned();

    let (status, _, _) = send(
        &state,
        "PATCH",
        &format!("/api/sessions/{id}"),
        Some(&cookie),
        Some(json!({ "name": "Service du midi, pluie" }).to_string()),
        10,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (_, _, listed) = send(&state, "GET", "/api/sessions", Some(&cookie), None, 10).await;
    assert_eq!(listed[0]["environment"], "Service du midi, pluie");
    assert_eq!(
        listed[0]["session_id"], id,
        "the identifier is the handle and does not move"
    );

    let (status, headers, _) = send(
        &state,
        "GET",
        &format!("/api/sessions/{id}/archive"),
        Some(&cookie),
        None,
        10,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let disposition = headers
        .get(header::CONTENT_DISPOSITION)
        .unwrap()
        .to_str()
        .unwrap();
    // Recognisable in a downloads folder, and still unique per capture.
    assert!(
        disposition.contains("service-du-midi-pluie-"),
        "{disposition} does not carry the name"
    );
    assert!(disposition.ends_with(".tar.gz\""));
}

#[tokio::test]
async fn a_name_made_only_of_punctuation_cannot_reach_the_download_header() {
    // The operator's own words land in a header; a quote or a newline
    // there would be a header injection.
    let (state, _dir) = recording_ready();
    let cookie = session_of(&state).await;
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
    let (_, _, listed) = send(&state, "GET", "/api/sessions", Some(&cookie), None, 10).await;
    let id = listed[0]["session_id"].as_str().unwrap().to_owned();
    send(
        &state,
        "PATCH",
        &format!("/api/sessions/{id}"),
        Some(&cookie),
        Some(json!({ "name": "\"; rm -rf /\r\nX-Evil: 1" }).to_string()),
        10,
    )
    .await;

    let (status, headers, _) = send(
        &state,
        "GET",
        &format!("/api/sessions/{id}/archive"),
        Some(&cookie),
        None,
        10,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let disposition = headers
        .get(header::CONTENT_DISPOSITION)
        .unwrap()
        .to_str()
        .unwrap();
    assert_eq!(
        disposition,
        format!(
            "attachment; filename=\"rm-rf-x-evil-1-{}.tar.gz\"",
            id.rsplit('-').next().unwrap()
        )
    );
}

#[tokio::test]
async fn a_blank_name_is_refused_rather_than_stored() {
    let (state, _dir) = recording_ready();
    let cookie = session_of(&state).await;
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
    let (_, _, listed) = send(&state, "GET", "/api/sessions", Some(&cookie), None, 10).await;
    let id = listed[0]["session_id"].as_str().unwrap().to_owned();

    let (status, _, _) = send(
        &state,
        "PATCH",
        &format!("/api/sessions/{id}"),
        Some(&cookie),
        Some(json!({ "name": "   " }).to_string()),
        10,
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn the_session_surface_is_closed_to_anonymous_callers() {
    let (state, _dir) = writable();
    for (method, path) in [
        ("GET", "/api/sessions"),
        ("GET", "/api/sessions/s-001/archive"),
        ("DELETE", "/api/sessions/s-001"),
        ("PATCH", "/api/sessions/s-001"),
    ] {
        let (status, _, _) = send(&state, method, path, None, None, 10).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {path} is open");
    }
}

#[tokio::test]
async fn calibration_is_closed_to_anonymous_callers() {
    let (state, _dir) = writable();
    for (method, path) in [
        ("POST", "/api/calibration"),
        ("DELETE", "/api/calibration"),
        ("POST", "/api/calibration/label"),
    ] {
        let (status, _, _) = send(&state, method, path, None, Some("{}".to_owned()), 10).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {path} is open");
    }
}
