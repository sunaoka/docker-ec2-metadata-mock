use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use ec2_metadata_mock::{AppState, Config, app};
use tracing::info;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    let config = Config::from_env().unwrap_or_else(|error| {
        eprintln!("configuration error: {error}");
        std::process::exit(2);
    });
    let default_log_filter = if config.debug { "debug" } else { "info" };

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| default_log_filter.into()))
        .init();

    let ipv4_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), config.listen_port);
    let ipv4_listener = tokio::net::TcpListener::bind(ipv4_address).await.expect("bind IMDS IPv4 listener");
    let state = AppState::new(config.clone());

    info!(address = %ipv4_address, "server started");

    if config.imds_ipv6_enabled {
        let ipv6_address = SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), config.listen_port);
        let ipv6_listener = tokio::net::TcpListener::bind(ipv6_address).await.expect("bind IMDS IPv6 listener");

        info!(address = %ipv6_address, "server started");

        tokio::try_join!(
            axum::serve(ipv4_listener, app(state.clone())).with_graceful_shutdown(shutdown_signal()),
            axum::serve(ipv6_listener, app(state)).with_graceful_shutdown(shutdown_signal())
        )
        .expect("serve IMDS");
    } else {
        axum::serve(ipv4_listener, app(state))
            .with_graceful_shutdown(shutdown_signal())
            .await
            .expect("serve IMDS");
    }
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c().await.expect("listen for shutdown signal");
}
