use std::{
    io::{Read, Write},
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener, TcpStream},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const CONFIGURATION_VARIABLES: [&str; 10] = [
    "IMDS_ROLE_NAME",
    "AWS_ACCESS_KEY_ID",
    "AWS_SECRET_ACCESS_KEY",
    "AWS_SESSION_TOKEN",
    "IMDS_CREDENTIAL_TTL_SECONDS",
    "IMDS_V1_ENABLED",
    "IMDS_IPV6_ENABLED",
    "IMDS_LISTEN_PORT",
    "DEBUG",
    "RUST_LOG",
];

struct Server {
    child: Child,
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop();
    }
}

impl Server {
    fn stop(&mut self) {
        if self.child.try_wait().unwrap().is_some() {
            return;
        }

        let process_id = self.child.id().to_string();
        let _ = Command::new("kill").args(["-INT", &process_id]).status();
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if self.child.try_wait().unwrap().is_some() {
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }

        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn binary_command() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ec2-metadata-mock"));
    for name in CONFIGURATION_VARIABLES {
        command.env_remove(name);
    }
    command
}

fn available_port() -> u16 {
    TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap().local_addr().unwrap().port()
}

fn spawn_server(port: u16, debug: bool, ipv6_enabled: bool, rust_log: &str) -> Server {
    let child = binary_command()
        .env("IMDS_LISTEN_PORT", port.to_string())
        .env("DEBUG", if debug { "1" } else { "0" })
        .env("IMDS_IPV6_ENABLED", if ipv6_enabled { "1" } else { "0" })
        .env("RUST_LOG", rust_log)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    Server { child }
}

fn assert_health(address: SocketAddr) {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if let Ok(mut stream) = TcpStream::connect_timeout(&address, Duration::from_millis(25)) {
            stream.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
            stream.set_write_timeout(Some(Duration::from_secs(1))).unwrap();
            stream
                .write_all(b"GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
                .unwrap();

            let mut response = String::new();
            stream.read_to_string(&mut response).unwrap();
            assert!(response.starts_with("HTTP/1.1 200"));
            return;
        }

        assert!(Instant::now() < deadline, "server did not listen on {address}");
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn binary_handles_invalid_config_and_serves_ipv4_and_ipv6() {
    let output = binary_command().env("IMDS_CREDENTIAL_TTL_SECONDS", "0").output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "configuration error: IMDS_CREDENTIAL_TTL_SECONDS must be greater than zero\n"
    );

    let ipv4_port = available_port();
    let mut ipv4_server = spawn_server(ipv4_port, false, false, "[");
    assert_health(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), ipv4_port));
    ipv4_server.stop();

    let dual_stack_port = available_port();
    let mut dual_stack_server = spawn_server(dual_stack_port, true, true, "info");
    assert_health(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), dual_stack_port));
    assert_health(SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), dual_stack_port));
    dual_stack_server.stop();
}
