use std::{
    collections::HashMap,
    env,
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{
    Json, Router,
    body::{Body, HttpBody, to_bytes},
    extract::{Path, Request, State},
    http::{
        HeaderMap, Method, StatusCode, Uri,
        header::{CONTENT_LENGTH, HeaderName},
    },
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, put},
};
use chrono::{DateTime, Duration as ChronoDuration, SecondsFormat, Utc};
use serde::Serialize;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};
use uuid::Uuid;

const TOKEN_TTL_HEADER: &str = "x-aws-ec2-metadata-token-ttl-seconds";
const TOKEN_HEADER: &str = "x-aws-ec2-metadata-token";
const MAX_TOKEN_TTL_SECONDS: u64 = 21_600;
const DEFAULT_LISTEN_PORT: &str = "8181";
const MAX_DEBUG_RESPONSE_BODY_BYTES: usize = 65_536;

#[derive(Clone, Debug)]
pub struct Config {
    pub role_name: String,
    pub access_key_id: String,
    pub secret_access_key: String,
    pub session_token: String,
    pub credential_ttl: Duration,
    pub imds_v1_enabled: bool,
    pub imds_ipv6_enabled: bool,
    pub listen_port: u16,
    pub debug: bool,
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        let credential_ttl = env_value("IMDS_CREDENTIAL_TTL_SECONDS", "3600")
            .parse::<u64>()
            .map_err(|_| "IMDS_CREDENTIAL_TTL_SECONDS must be a positive integer".to_owned())?;
        if credential_ttl == 0 {
            return Err("IMDS_CREDENTIAL_TTL_SECONDS must be greater than zero".to_owned());
        }

        let listen_port = parse_listen_port(&env_value("IMDS_LISTEN_PORT", DEFAULT_LISTEN_PORT))?;

        Ok(Self {
            role_name: env_value("IMDS_ROLE_NAME", "local-role"),
            access_key_id: env_value("AWS_ACCESS_KEY_ID", "test"),
            secret_access_key: env_value("AWS_SECRET_ACCESS_KEY", "test"),
            session_token: env_value("AWS_SESSION_TOKEN", "test"),
            credential_ttl: Duration::from_secs(credential_ttl),
            imds_v1_enabled: parse_flag("IMDS_V1_ENABLED"),
            imds_ipv6_enabled: parse_flag("IMDS_IPV6_ENABLED"),
            listen_port,
            debug: parse_flag("DEBUG"),
        })
    }
}

fn env_value(name: &str, default: &str) -> String {
    env::var(name).unwrap_or_else(|_| default.to_owned())
}

fn parse_flag(name: &str) -> bool {
    env::var(name).is_ok_and(|value| value == "1")
}

fn parse_listen_port(value: &str) -> Result<u16, String> {
    value
        .parse::<u16>()
        .ok()
        .filter(|port| *port != 0)
        .ok_or_else(|| "IMDS_LISTEN_PORT must be an integer from 1 to 65535".to_owned())
}

#[derive(Clone)]
pub struct AppState {
    config: Config,
    tokens: Arc<Mutex<HashMap<String, Instant>>>,
}

impl AppState {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            tokens: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

pub fn app(state: AppState) -> Router {
    let debug = state.config.debug;
    let router = Router::new()
        .route("/health", get(health))
        .route("/latest/api/token", put(create_token))
        .route("/latest/meta-data/iam/security-credentials/", get(role_name))
        .route("/latest/meta-data/iam/security-credentials/{role}", get(credentials))
        .fallback(not_found)
        .with_state(state);

    if debug { router.layer(middleware::from_fn(debug_log)) } else { router }
}

async fn health() -> &'static str {
    "ok\n"
}

async fn not_found(method: Method, uri: Uri) -> StatusCode {
    warn!(%method, %uri, "unmatched IMDS request");
    StatusCode::NOT_FOUND
}

async fn debug_log(request: Request, next: Next) -> Response {
    if request.uri().path() == "/health" {
        return next.run(request).await;
    }

    debug!(method = %request.method(), uri = %request.uri(), headers = ?request.headers(), "IMDS request");

    let response = next.run(request).await;
    let (parts, body) = response.into_parts();
    let content_length = parts
        .headers
        .get(CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<usize>().ok())
        .or_else(|| body.size_hint().exact().and_then(|length| usize::try_from(length).ok()));

    if content_length.is_some_and(|length| length <= MAX_DEBUG_RESPONSE_BODY_BYTES) {
        let body = to_bytes(body, usize::MAX).await.expect("IMDS response body must be readable");
        debug!(status = %parts.status, headers = ?parts.headers, body = %String::from_utf8_lossy(&body), "IMDS response");
        Response::from_parts(parts, Body::from(body))
    } else {
        debug!(status = %parts.status, headers = ?parts.headers, body = "<omitted: unknown or exceeds 65536 bytes>", "IMDS response");
        Response::from_parts(parts, body)
    }
}

async fn create_token(State(state): State<AppState>, headers: HeaderMap) -> Result<String, (StatusCode, &'static str)> {
    info!("token requested");

    let ttl = headers
        .get(HeaderName::from_static(TOKEN_TTL_HEADER))
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|ttl| (1..=MAX_TOKEN_TTL_SECONDS).contains(ttl))
        .ok_or((StatusCode::BAD_REQUEST, "invalid token TTL\n"))?;

    let token = Uuid::new_v4().to_string();
    state.tokens.lock().await.insert(token.clone(), Instant::now() + Duration::from_secs(ttl));

    Ok(token)
}

async fn role_name(State(state): State<AppState>, headers: HeaderMap) -> Result<String, (StatusCode, &'static str)> {
    info!("role requested");

    authorize(&state, &headers).await?;

    Ok(state.config.role_name.clone())
}

async fn credentials(State(state): State<AppState>, headers: HeaderMap, Path(role): Path<String>) -> Result<Json<Credentials>, (StatusCode, &'static str)> {
    info!("credentials requested");

    authorize(&state, &headers).await?;
    if role != state.config.role_name {
        return Err((StatusCode::NOT_FOUND, "role not found\n"));
    }

    let expiration =
        Utc::now() + ChronoDuration::from_std(state.config.credential_ttl).map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "invalid credential TTL\n"))?;

    Ok(Json(Credentials::from_config(&state.config, expiration)))
}

async fn authorize(state: &AppState, headers: &HeaderMap) -> Result<(), (StatusCode, &'static str)> {
    let Some(token) = headers.get(HeaderName::from_static(TOKEN_HEADER)).and_then(|value| value.to_str().ok()) else {
        return if state.config.imds_v1_enabled {
            Ok(())
        } else {
            Err((StatusCode::UNAUTHORIZED, "token required\n"))
        };
    };

    let mut tokens = state.tokens.lock().await;
    tokens.retain(|_, expiration| *expiration > Instant::now());
    if tokens.contains_key(token) {
        Ok(())
    } else {
        Err((StatusCode::UNAUTHORIZED, "invalid token\n"))
    }
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct Credentials {
    code: &'static str,
    last_updated: String,
    #[serde(rename = "Type")]
    credential_type: &'static str,
    access_key_id: String,
    secret_access_key: String,
    token: String,
    expiration: String,
}

impl Credentials {
    fn from_config(config: &Config, expiration: DateTime<Utc>) -> Self {
        Self {
            code: "Success",
            last_updated: imds_timestamp(Utc::now()),
            credential_type: "AWS-HMAC",
            access_key_id: config.access_key_id.clone(),
            secret_access_key: config.secret_access_key.clone(),
            token: config.session_token.clone(),
            expiration: imds_timestamp(expiration),
        }
    }
}

fn imds_timestamp(timestamp: DateTime<Utc>) -> String {
    timestamp.to_rfc3339_opts(SecondsFormat::Secs, true)
}

impl IntoResponse for Credentials {
    fn into_response(self) -> axum::response::Response {
        Json(self).into_response()
    }
}
