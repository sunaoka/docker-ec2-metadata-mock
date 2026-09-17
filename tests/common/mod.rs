use std::time::Duration;

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use ec2_metadata_mock::{AppState, Config, app};
use tower::ServiceExt;

pub fn test_app(v1_enabled: bool) -> Router {
    test_app_with_debug(v1_enabled, false)
}

pub fn test_app_with_debug(v1_enabled: bool, debug: bool) -> Router {
    app(AppState::new(Config {
        role_name: "local-role".to_owned(),
        access_key_id: "test-key".to_owned(),
        secret_access_key: "test-secret".to_owned(),
        session_token: "test-token".to_owned(),
        credential_ttl: Duration::from_secs(60),
        imds_v1_enabled: v1_enabled,
        imds_ipv6_enabled: false,
        listen_port: 8181,
        debug,
    }))
}

pub async fn issue_token(app: &Router, ttl_seconds: u64) -> String {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/latest/api/token")
                .header("x-aws-ec2-metadata-token-ttl-seconds", ttl_seconds)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    String::from_utf8(to_bytes(response.into_body(), usize::MAX).await.unwrap().to_vec()).unwrap()
}
