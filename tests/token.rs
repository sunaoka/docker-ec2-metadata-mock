mod common;

use std::{thread, time::Duration};

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use tower::ServiceExt;

use common::{issue_token, test_app};

#[tokio::test]
async fn rejects_invalid_and_missing_tokens_when_v1_is_disabled() {
    let app = test_app(false);
    for request in [
        Request::builder()
            .uri("/latest/meta-data/iam/security-credentials/")
            .body(Body::empty())
            .unwrap(),
        Request::builder()
            .uri("/latest/meta-data/iam/security-credentials/")
            .header("x-aws-ec2-metadata-token", "invalid")
            .body(Body::empty())
            .unwrap(),
    ] {
        assert_eq!(app.clone().oneshot(request).await.unwrap().status(), StatusCode::UNAUTHORIZED);
    }
}

#[tokio::test]
async fn rejects_invalid_token_ttl() {
    let response = test_app(false)
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/latest/api/token")
                .header("x-aws-ec2-metadata-token-ttl-seconds", "21601")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn rejects_expired_token() {
    let app = test_app(false);
    let token = issue_token(&app, 1).await;
    thread::sleep(Duration::from_millis(1_100));
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
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
