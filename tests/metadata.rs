mod common;

use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use chrono::DateTime;
use serde_json::Value;
use tower::ServiceExt;

use common::{issue_token, test_app, test_app_with_debug};

#[tokio::test]
async fn returns_not_found_for_unmatched_route() {
    let response = test_app(false)
        .oneshot(Request::builder().uri("/latest/meta-data/unknown").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn returns_role_name_without_a_trailing_newline() {
    let app = test_app(false);
    let token = issue_token(&app, 60).await;
    let response = app
        .oneshot(
            Request::builder()
                .uri("/latest/meta-data/iam/security-credentials/")
                .header("x-aws-ec2-metadata-token", token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = String::from_utf8(to_bytes(response.into_body(), usize::MAX).await.unwrap().to_vec()).unwrap();
    assert_eq!(body, "local-role");
}

#[tokio::test]
async fn issues_token_and_reads_credentials() {
    let app = test_app(false);
    let token = issue_token(&app, 60).await;
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/latest/meta-data/iam/security-credentials/local-role")
                .header("x-aws-ec2-metadata-token", token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = String::from_utf8(to_bytes(response.into_body(), usize::MAX).await.unwrap().to_vec()).unwrap();
    let credentials: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(credentials["AccessKeyId"], "test-key");
    assert_eq!(credentials["Code"], "Success");

    for field in ["LastUpdated", "Expiration"] {
        let timestamp = credentials[field].as_str().unwrap();
        assert!(timestamp.ends_with('Z'));
        assert!(!timestamp.contains('.'));
        assert!(DateTime::parse_from_rfc3339(timestamp).is_ok());
    }
}

#[tokio::test]
async fn debug_logging_preserves_credential_response() {
    let app = test_app_with_debug(false, true);
    let token = issue_token(&app, 60).await;
    let response = app
        .oneshot(
            Request::builder()
                .uri("/latest/meta-data/iam/security-credentials/local-role")
                .header("x-aws-ec2-metadata-token", token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = String::from_utf8(to_bytes(response.into_body(), usize::MAX).await.unwrap().to_vec()).unwrap();
    assert!(body.contains("test-secret"));
}

#[tokio::test]
async fn debug_logging_allows_health_check() {
    let response = test_app_with_debug(false, true)
        .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn rejects_unknown_role() {
    let app = test_app(false);
    let token = issue_token(&app, 60).await;
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/latest/meta-data/iam/security-credentials/other")
                .header("x-aws-ec2-metadata-token", token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn permits_v1_when_enabled() {
    let response = test_app(true)
        .oneshot(
            Request::builder()
                .uri("/latest/meta-data/iam/security-credentials/")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}
