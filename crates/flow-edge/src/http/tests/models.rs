use super::*;

#[tokio::test]
async fn the_model_surface_is_closed_to_anonymous_callers() {
    let (state, _dir) = writable();
    for (method, path) in [
        ("POST", "/api/model"),
        ("GET", "/api/models"),
        ("POST", "/api/models/m-001"),
        ("PATCH", "/api/models/m-001"),
        ("DELETE", "/api/models/m-001"),
    ] {
        let (status, _, _) = send(&state, method, path, None, Some("{}".to_owned()), 10).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {path} is open");
    }
}
