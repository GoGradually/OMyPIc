//! OMyPIc 백엔드 서버: 조립(`wiring`)과 실행/종료. 실행 파일과 데스크톱 셸이 함께 쓴다.

pub mod wiring;

use std::net::SocketAddr;
use std::time::Duration;

use anyhow::{anyhow, Context};
use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::header::CONTENT_TYPE;
use axum::middleware::{self, Next};
use axum::response::Response;
use futures_util::StreamExt;
use omypic_infrastructure::config::AppConfig;
use omypic_presentation::AppState;
use tokio::net::TcpListener;
use tokio::sync::watch;
use tokio::task::JoinHandle;

/// 종료 신호 뒤 진행 중인 요청을 기다리는 최대 시간(Spring 기본 `timeout-per-shutdown-phase`).
pub const SHUTDOWN_GRACE: Duration = Duration::from_secs(30);

/// 실행 중인 서버. `shutdown`으로 끈다. 그냥 버리면 종료 신호만 보낸다.
pub struct RunningServer {
    /// 실제로 리슨하는 주소(포트 0으로 시작했으면 OS가 고른 포트).
    pub addr: SocketAddr,
    shutdown_tx: watch::Sender<bool>,
    serve: Option<JoinHandle<std::io::Result<()>>>,
    /// 조립한 객체 그래프. 블로킹 HTTP 클라이언트를 갖고 있으므로 블로킹 스레드에서 놓는다.
    state: Option<AppState>,
}

impl RunningServer {
    /// 새 연결을 받지 않고, 열린 SSE 스트림을 끝내고, 진행 중인 요청이 끝나기를
    /// (최대 `SHUTDOWN_GRACE`) 기다린 뒤 종료한다.
    pub async fn shutdown(mut self) -> anyhow::Result<()> {
        let _ = self.shutdown_tx.send(true);
        let result = match self.serve.take() {
            Some(mut serve) => match tokio::time::timeout(SHUTDOWN_GRACE, &mut serve).await {
                Ok(joined) => joined
                    .context("Server task failed")?
                    .context("Server stopped with an error"),
                Err(_) => {
                    tracing::warn!(
                        "Requests still running after {}s; closing them",
                        SHUTDOWN_GRACE.as_secs()
                    );
                    serve.abort();
                    Ok(())
                }
            },
            None => Ok(()),
        };
        release_off_async_workers(self.state.take()).await;
        result
    }
}

impl Drop for RunningServer {
    fn drop(&mut self) {
        let _ = self.shutdown_tx.send(true);
        if let Some(state) = self.state.take() {
            std::thread::spawn(move || drop(state));
        }
    }
}

/// 데이터 폴더를 만들고 `<data_dir>/omypic.db`를 열고 어댑터와 유스케이스를 조립한 뒤
/// `bind`에 리슨한다. 포트가 0이면 OS가 고른 포트가 `addr`에 들어간다.
pub async fn start(config: AppConfig, bind: SocketAddr) -> anyhow::Result<RunningServer> {
    let listener = TcpListener::bind(bind)
        .await
        .map_err(|err| anyhow!("Failed to bind {bind}: {err}"))?;
    let addr = listener.local_addr()?;

    // 블로킹 HTTP 클라이언트는 비동기 작업자 스레드에서 만들 수 없다.
    let state = tokio::task::spawn_blocking(move || wiring::build_state(&config))
        .await
        .context("Server assembly panicked")??;

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let app = omypic_presentation::router(state.clone()).layer(middleware::from_fn_with_state(
        shutdown_rx.clone(),
        end_event_streams_on_shutdown,
    ));
    let mut signal = shutdown_rx;
    let serve = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = signal.wait_for(|stop| *stop).await;
            })
            .await
    });
    tracing::info!("OMyPIc server listening on http://{addr}");
    Ok(RunningServer {
        addr,
        shutdown_tx,
        serve: Some(serve),
        state: Some(state),
    })
}

/// SSE 응답은 30분까지 열려 있으므로 종료 신호가 오면 끝낸다. 그러지 않으면 정상 종료가
/// 열린 스트림을 기다린다. 스트림이 끝나면 SSE 등록도 풀린다.
async fn end_event_streams_on_shutdown(
    State(shutdown): State<watch::Receiver<bool>>,
    request: Request,
    next: Next,
) -> Response {
    let response = next.run(request).await;
    let is_event_stream = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("text/event-stream"));
    if !is_event_stream {
        return response;
    }
    let (parts, body) = response.into_parts();
    let mut shutdown = shutdown;
    let stopped = async move {
        let _ = shutdown.wait_for(|stop| *stop).await;
    };
    let stream = body.into_data_stream().take_until(stopped);
    Response::from_parts(parts, Body::from_stream(stream))
}

async fn release_off_async_workers(state: Option<AppState>) {
    if let Some(state) = state {
        if let Err(err) = tokio::task::spawn_blocking(move || drop(state)).await {
            tracing::error!("Releasing server resources failed: {err}");
        }
    }
}
