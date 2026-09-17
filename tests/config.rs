use std::{env, ffi::OsString, sync::Mutex};

use ec2_metadata_mock::Config;

static ENV_LOCK: Mutex<()> = Mutex::new(());

struct EnvGuard {
    name: &'static str,
    original: Option<OsString>,
}

impl EnvGuard {
    fn new(name: &'static str) -> Self {
        Self {
            name,
            original: env::var_os(name),
        }
    }

    fn set(&self, value: &str) {
        unsafe {
            env::set_var(self.name, value);
        }
    }

    fn remove(&self) {
        unsafe {
            env::remove_var(self.name);
        }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        unsafe {
            match &self.original {
                Some(value) => env::set_var(self.name, value),
                None => env::remove_var(self.name),
            }
        }
    }
}

#[test]
fn parses_listen_port_configuration() {
    let _lock = ENV_LOCK.lock().unwrap();
    let env_guard = EnvGuard::new("IMDS_LISTEN_PORT");

    env_guard.remove();
    assert_eq!(Config::from_env().unwrap().listen_port, 8181);

    env_guard.set("18181");
    assert_eq!(Config::from_env().unwrap().listen_port, 18181);

    for port in ["", "0", "65536", "invalid"] {
        env_guard.set(port);
        assert_eq!(Config::from_env().unwrap_err(), "IMDS_LISTEN_PORT must be an integer from 1 to 65535");
    }
}

fn assert_flag_configuration(name: &'static str, read_flag: fn(&Config) -> bool) {
    let env_guard = EnvGuard::new(name);

    env_guard.remove();
    assert!(!read_flag(&Config::from_env().unwrap()));

    env_guard.set("0");
    assert!(!read_flag(&Config::from_env().unwrap()));

    env_guard.set("1");
    assert!(read_flag(&Config::from_env().unwrap()));

    for value in ["", "true", "false", "invalid"] {
        env_guard.set(value);
        assert!(!read_flag(&Config::from_env().unwrap()));
    }
}

#[test]
fn parses_flag_configuration() {
    let _lock = ENV_LOCK.lock().unwrap();
    assert_flag_configuration("IMDS_V1_ENABLED", |config| config.imds_v1_enabled);
    assert_flag_configuration("IMDS_IPV6_ENABLED", |config| config.imds_ipv6_enabled);
    assert_flag_configuration("DEBUG", |config| config.debug);
}
