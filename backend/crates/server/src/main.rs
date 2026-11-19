//! `omypic-server`: 환경 변수(`OMYPIC_*`)로 설정을 읽고 `127.0.0.1:<port>`에 띄운다.
//! Ctrl+C(유닉스에서는 SIGTERM도)로 정상 종료한다.

use std::net::{Ipv4Addr, SocketAddr};
use std::process::ExitCode;

use omypic_infrastructure::config::AppConfig;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let config = match AppConfig::from_env() {
        Ok(config) => config,
        Err(err) => {
            eprintln!("{err}");
            return ExitCode::FAILURE;
        }
    };
    tracing::info!("Data directory: {}", config.data_dir);
    let bind = SocketAddr::from((Ipv4Addr::LOCALHOST, config.port));
    let server = match omypic_server::start(config, bind).await {
        Ok(server) => server,
        Err(err) => {
            eprintln!("{err:#}");
            return ExitCode::FAILURE;
        }
    };

    wait_for_stop_signal().await;
    tracing::info!("Shutting down");
    match server.shutdown().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{err:#}");
            ExitCode::FAILURE
        }
    }
}

async fn wait_for_stop_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        match signal(SignalKind::terminate()) {
            Ok(mut terminate) => {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {}
                    _ = terminate.recv() => {}
                }
            }
            Err(_) => {
                let _ = tokio::signal::ctrl_c().await;
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
