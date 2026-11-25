//! Launched by Chrome/Edge with the caller origin as argument (plus
//! `--parent-window=` on Windows), or by the user with `--install <extension-id>`.

use std::io;
use std::process::ExitCode;

use parlance_native_host::{RelayOutcome, TcpBridge, install, run_relay};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [flag, extension_id] if flag == "--install" => run_install(extension_id),
        [flag, ..] if flag == "--install" => {
            eprintln!("usage: parlance-native-host --install <extension-id>");
            ExitCode::FAILURE
        }
        _ => run_host(),
    }
}

fn run_install(extension_id: &str) -> ExitCode {
    match install::install(extension_id) {
        Ok(locations) => {
            locations.iter().for_each(|location| eprintln!("registered {location}"));
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("install failed: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run_host() -> ExitCode {
    let Some(discovery_path) = TcpBridge::default_discovery_path() else {
        eprintln!("parlance-native-host: cannot resolve the local data directory");
        return ExitCode::FAILURE;
    };
    let bridge = TcpBridge::new(discovery_path);
    match run_relay(&mut io::stdin().lock(), &mut io::stdout().lock(), &bridge) {
        Ok(outcome) => {
            if outcome != RelayOutcome::ExtensionClosed {
                eprintln!("parlance-native-host: session ended: {outcome:?}");
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("parlance-native-host: {error}");
            ExitCode::FAILURE
        }
    }
}
