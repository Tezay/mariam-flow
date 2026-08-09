use super::*;

#[tokio::test]
async fn the_model_surface_is_closed_to_anonymous_callers() {
    let (state, _dir) = writable();
    for (method, path) in [
        ("POST", "/api/model"),
        ("GET", "/api/models"),
        ("GET", "/api/models/m-001"),
        ("POST", "/api/models/m-001"),
        ("PATCH", "/api/models/m-001"),
        ("DELETE", "/api/models/m-001"),
    ] {
        let (status, _, _) = send(&state, method, path, None, Some("{}".to_owned()), 10).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {path} is open");
    }
}

/// A model in the library, written straight to disk: these tests exercise the
/// route that reads the library, not the import that fills it.
///
/// Handles are stamp-first, as `model_id` writes them: the listing reads the
/// import date back out of them.
fn hold_model(dir: &tempfile::TempDir, id: &str, evaluation: Option<&[u8]>) {
    let path = dir.path().join("models").join(id);
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(path.join("model.onnx"), b"stands in for a graph").unwrap();
    std::fs::write(
        path.join("analysis.json"),
        br#"{"window_us":5000000,"hop_us":1000000}"#,
    )
    .unwrap();
    if let Some(bytes) = evaluation {
        std::fs::write(path.join("evaluation.json"), bytes).unwrap();
    }
}

const EVALUATION: &[u8] = br#"{"schema":1,"accuracy":0.674,"baseline_accuracy":0.312,
     "confusion":[[612,74,11,3],[88,401,118,22],[14,131,356,96],[6,27,142,330]],
     "windows":2841,"splits":4,"receivers":["rx-1","rx-2"],
     "sessions":[{"session_id":"bench-001","windows":2841,"support":[700,710,715,716]}]}"#;

#[tokio::test]
async fn a_model_answers_with_the_scores_its_run_earned() {
    let (state, dir) = writable();
    hold_model(&dir, "20260812T101500Z-campagne-juin", Some(EVALUATION));
    let cookie = session_of(&state).await;

    let (status, _, body) = send(
        &state,
        "GET",
        "/api/models/20260812T101500Z-campagne-juin",
        Some(&cookie),
        None,
        10,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["id"], "20260812T101500Z-campagne-juin");
    assert_eq!(body["evaluation"]["accuracy"], 0.674);
    // Rows are truth: truly saturated, predicted medium 142 times.
    assert_eq!(body["evaluation"]["confusion"][3][2], 142);
    assert_eq!(body["evaluation"]["sessions"][0]["session_id"], "bench-001");
}

#[tokio::test]
async fn a_model_trained_before_runs_reported_their_scores_answers_without_them() {
    // Absent rather than empty: an evaluation of zeroes would read as a model
    // that scored nothing, which is a different fact.
    let (state, dir) = writable();
    hold_model(&dir, "20260701T090000Z-essai-mai", None);
    let cookie = session_of(&state).await;

    let (status, _, body) = send(
        &state,
        "GET",
        "/api/models/20260701T090000Z-essai-mai",
        Some(&cookie),
        None,
        10,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert!(body.get("evaluation").is_none());
    assert_eq!(body["has_evaluation"], false);
}

#[tokio::test]
async fn the_list_says_which_models_have_scores_to_open() {
    let (state, dir) = writable();
    hold_model(&dir, "20260812T101500Z-campagne-juin", Some(EVALUATION));
    hold_model(&dir, "20260701T090000Z-essai-mai", None);
    let cookie = session_of(&state).await;

    let (status, _, body) = send(&state, "GET", "/api/models", Some(&cookie), None, 10).await;

    assert_eq!(status, StatusCode::OK);
    let listed = body.as_array().unwrap();
    assert_eq!(listed[0]["id"], "20260812T101500Z-campagne-juin");
    assert_eq!(listed[0]["has_evaluation"], true);
    assert_eq!(listed[1]["has_evaluation"], false);
    // Deliberate: the matrix is what the detail route is for.
    assert!(listed[0].get("evaluation").is_none());
}

#[tokio::test]
async fn a_handle_that_names_no_model_is_answered_as_missing() {
    let (state, _dir) = writable();
    let cookie = session_of(&state).await;

    let (status, _, _) = send(
        &state,
        "GET",
        "/api/models/never-imported",
        Some(&cookie),
        None,
        10,
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_handle_reaching_outside_the_library_is_refused() {
    // The handle is a directory name. Percent-encoding is decoded before the
    // handler sees it, so the guard cannot be left to the router.
    let (state, _dir) = writable();
    let cookie = session_of(&state).await;

    for path in [
        "/api/models/..%2F..%2Fetc",
        "/api/models/.%2E",
        "/api/models/%2Fetc%2Fpasswd",
    ] {
        let (status, _, _) = send(&state, "GET", path, Some(&cookie), None, 10).await;
        assert_ne!(status, StatusCode::OK, "{path} was served");
    }
}
