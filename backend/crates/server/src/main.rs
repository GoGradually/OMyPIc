//! `omypic-server`: 환경 변수(`OMYPIC_*`)로 설정을 읽고 `127.0.0.1:<port>`에 띄운다.
//! Ctrl+C(유닉스에서는 SIGTERM도)로 정상 종료한다. 진행 중인 요청은 최대 `SHUTDOWN_GRACE`만
//! 기다리고, 그 뒤에도 남은 블로킹 작업은 기다리지 않고 끝낸다.

use std::io::IsTerminal;
use std::net::{Ipv4Addr, SocketAddr};
use std::process::ExitCode;

use omypic_infrastructure::config::AppConfig;
use tracing_subscriber::EnvFilter;

fn main() -> ExitCode {
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(err) => {
            eprintln!("Failed to start async runtime: {err}");
            return ExitCode::FAILURE;
        }
    };
    let code = runtime.block_on(run());
    // 블로킹 작업 안에서 멈춘 요청(모델 다운로드 등)을 끝까지 기다리지 않는다.
    runtime.shutdown_timeout(omypic_server::RUNTIME_SHUTDOWN_TIMEOUT);
    code
}

async fn run() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .with_ansi(std::io::stderr().is_terminal())
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
