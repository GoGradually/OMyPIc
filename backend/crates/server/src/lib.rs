//! OMyPIc 백엔드 서버: 조립(`wiring`)과 실행/종료. 실행 파일과 데스크톱 셸이 함께 쓴다.

pub mod wiring;

use std::net::SocketAddr;
use std::sync::{Arc, Weak};
use std::time::Duration;

use anyhow::{anyhow, Context};
use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::header::CONTENT_TYPE;
use axum::middleware::{self, Next};
use axum::response::Response;
use futures_util::StreamExt;
use omypic_application::voice::usecase::{effective_voice_idle_timeout_ms, VoiceSessionUseCase};
use omypic_infrastructure::config::AppConfig;
use omypic_presentation::AppState;
use tokio::net::TcpListener;
use tokio::sync::watch;
use tokio::task::JoinHandle;

/// 종료 신호 뒤 진행 중인(SSE가 아닌) 요청을 기다리는 최대 시간. 넘기면 그 요청은 버린다.
pub const SHUTDOWN_GRACE: Duration = Duration::from_secs(5);
/// 조립한 객체를 놓는 데 기다리는 최대 시간.
pub const RELEASE_GRACE: Duration = Duration::from_secs(1);
/// 런타임을 끝낼 때 남은 블로킹 작업(멈춘 요청)을 기다리는 최대 시간. 런타임을 가진 쪽(실행
/// 파일, 데스크톱 셸)이 `Runtime::shutdown_timeout`에 넘긴다. 그러지 않고 런타임을 그냥 놓으면
/// 블로킹 작업이 끝날 때까지 프로세스가 끝나지 않는다.
pub const RUNTIME_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(1);

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
    /// 메모리에 둔 음성 세션 수. 정리를 일으키지 않으므로 시험이 주기 정리를 지켜볼 때 쓴다.
    pub fn kept_voice_sessions(&self) -> usize {
        self.state
            .as_ref()
            .map_or(0, |state| state.voice_session.kept_session_count())
    }

    /// 새 연결을 받지 않고, 열린 SSE 스트림을 끝내고, 진행 중인 요청이 끝나기를
    /// (최대 `SHUTDOWN_GRACE`) 기다린 뒤 종료한다. 블로킹 작업 안에서 멈춘 요청은 기다리지
    /// 않으므로 `SHUTDOWN_GRACE + RELEASE_GRACE` 안에 돌아온다. 그런 작업의 스레드는 남으니
    /// 런타임을 가진 쪽은 `RUNTIME_SHUTDOWN_TIMEOUT`으로 런타임을 끝낸다.
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

    let sweep_every = voice_sweep_interval(config.voice.idle_timeout_ms);
    // 블로킹 HTTP 클라이언트는 비동기 작업자 스레드에서 만들 수 없다.
    let state = tokio::task::spawn_blocking(move || wiring::build_state(&config))
        .await
        .context("Server assembly panicked")??;

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let app = omypic_presentation::router(state.clone()).layer(middleware::from_fn_with_state(
        shutdown_rx.clone(),
        end_event_streams_on_shutdown,
    ));
    tokio::spawn(sweep_voice_sessions(
        Arc::downgrade(&state.voice_session),
        sweep_every,
        shutdown_rx.clone(),
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

/// 버려진 음성 세션을 찾는 간격: 유예 시간의 4분의 1, 1초에서 30초 사이. 유예 시간은 세션과
/// 같은 규칙으로 읽는다(0 이하이면 기본값).
fn voice_sweep_interval(idle_timeout_ms: i64) -> Duration {
    let idle_timeout_ms = effective_voice_idle_timeout_ms(idle_timeout_ms);
    let quarter = u64::try_from(idle_timeout_ms / 4).unwrap_or(0);
    Duration::from_millis(quarter.clamp(1_000, 30_000))
}

/// 요청이 없어도 버려진 음성 세션과 보관 기간이 지난 정지 세션이 정리되도록 주기적으로 부른다.
/// 종료 신호가 오거나 유스케이스가 사라지면 끝난다. 유스케이스는 `Weak`로만 쥐고 블로킹
/// 스레드 안에서만 잠시 붙잡으므로, 이 작업이 마지막 주인이 되어 블로킹 HTTP 클라이언트가 든
/// 객체 그래프를 비동기 작업자에서 놓는 일이 없다.
async fn sweep_voice_sessions(
    voice_session: Weak<VoiceSessionUseCase>,
    every: Duration,
    mut shutdown: watch::Receiver<bool>,
) {
    let mut ticks = tokio::time::interval(every);
    ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        let stopped = tokio::select! {
            _ = ticks.tick() => false,
            _ = shutdown.wait_for(|stop| *stop) => true,
        };
        if stopped {
            return;
        }
        let voice_session = Weak::clone(&voice_session);
        let alive = tokio::task::spawn_blocking(move || match voice_session.upgrade() {
            Some(voice_session) => {
                voice_session.reap_idle_sessions();
                true
            }
            None => false,
        })
        .await;
        if !matches!(alive, Ok(true)) {
            return;
        }
    }
}

async fn release_off_async_workers(state: Option<AppState>) {
    if let Some(state) = state {
        let release = tokio::task::spawn_blocking(move || drop(state));
        match tokio::time::timeout(RELEASE_GRACE, release).await {
            Ok(Ok(())) => {}
            Ok(Err(err)) => tracing::error!("Releasing server resources failed: {err}"),
            Err(_) => tracing::warn!("Releasing server resources is taking long; not waiting"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sweep_interval_uses_the_same_idle_timeout_as_the_sessions() {
        // 0 or less means the default 10 minutes for the sessions, so a 30 s sweep.
        assert_eq!(Duration::from_secs(30), voice_sweep_interval(0));
        assert_eq!(Duration::from_secs(30), voice_sweep_interval(-1_000));
        assert_eq!(Duration::from_secs(30), voice_sweep_interval(600_000));
        assert_eq!(Duration::from_secs(2), voice_sweep_interval(8_000));
        assert_eq!(Duration::from_secs(1), voice_sweep_interval(1_000));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_sweep_does_not_keep_the_voice_sessions_alive() {
        let dir = tempfile::tempdir().unwrap();
        let defaults = AppConfig::default();
        let config = AppConfig {
            data_dir: dir.path().to_string_lossy().into_owned(),
            rag: omypic_infrastructure::config::RagConfig {
                provider: "hash".to_string(),
                ..defaults.rag.clone()
            },
            ..defaults
        };
        let state = tokio::task::spawn_blocking(move || wiring::build_state(&config))
            .await
            .unwrap()
            .unwrap();
        let weak = Arc::downgrade(&state.voice_session);
        let (_shutdown_tx, shutdown_rx) = watch::channel(false);
        let sweep = tokio::spawn(sweep_voice_sessions(
            Arc::downgrade(&state.voice_session),
            Duration::from_millis(50),
            shutdown_rx,
        ));
        tokio::time::sleep(Duration::from_millis(120)).await;

        // The owner releases the graph on a blocking thread (as `RunningServer` does).
        tokio::task::spawn_blocking(move || drop(state))
            .await
            .unwrap();

        // A sweep that is running right now holds a strong reference for a moment: poll briefly.
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        while weak.strong_count() > 0 && tokio::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(
            0,
            weak.strong_count(),
            "the sweep task still owns the voice session use case"
        );
        tokio::time::timeout(Duration::from_secs(2), sweep)
            .await
            .expect("the sweep ends by itself once the use case is gone")
            .unwrap();
    }
}
