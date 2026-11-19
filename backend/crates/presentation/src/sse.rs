//! `GET /api/voice/sessions/{id}/events`의 SSE 스트림.
//!
//! Java `VoiceSessionController`와 같게 보낸다. 이벤트마다 `event:`(이름)와 `data:`(JSON)만 있고
//! `id:` 줄, 첫 주석, `retry:`, keep-alive는 없다(재연결 위치는 payload의 `eventId`와
//! `sinceEventId` 질의로 맞춘다). 30분이 지나면 스트림을 끝낸다. 클라이언트가 끊으면
//! (응답 본문이 drop되면) 등록한 싱크를 그 `Arc` 그대로 해제한다.
//!
//! 음성 세션 유스케이스는 이벤트 잠금을 쥔 채 `VoiceEventSink::send`를 부른다. 그래서 싱크는
//! 절대 막히지 않는 unbounded 채널에 넣기만 하고, 유스케이스를 다시 부르지 않는다.

use std::convert::Infallible;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use axum::response::sse::{Event, Sse};
use futures_core::Stream;
use omypic_application::voice::model::VoiceEventSink;
use omypic_application::voice::usecase::VoiceSessionUseCase;
use omypic_domain::error::AppResult;
use serde_json::Value;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::time::{sleep, Sleep};

use crate::error::{blocking, ApiError};

/// Java `SseEmitter(Duration.ofMinutes(30))`.
pub const SSE_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// 싱크를 등록하고 해제하는 쪽. 음성 세션 유스케이스가 구현한다.
pub trait SinkRegistry: Send + Sync {
    fn register(
        &self,
        voice_session_id: &str,
        sink: Arc<dyn VoiceEventSink>,
        since_event_id: Option<i64>,
    ) -> AppResult<()>;

    /// 등록할 때 쓴 것과 같은 `Arc`로 해제한다.
    fn unregister(&self, voice_session_id: &str, sink: &Arc<dyn VoiceEventSink>);
}

impl SinkRegistry for VoiceSessionUseCase {
    fn register(
        &self,
        voice_session_id: &str,
        sink: Arc<dyn VoiceEventSink>,
        since_event_id: Option<i64>,
    ) -> AppResult<()> {
        self.register_sink_since(voice_session_id, sink, since_event_id)
    }

    fn unregister(&self, voice_session_id: &str, sink: &Arc<dyn VoiceEventSink>) {
        self.unregister_sink(voice_session_id, sink);
    }
}

type QueuedEvent = (String, Value);

struct ChannelSink {
    tx: UnboundedSender<QueuedEvent>,
}

impl VoiceEventSink for ChannelSink {
    fn send(&self, event: &str, payload: Value) -> bool {
        self.tx.send((event.to_string(), payload)).is_ok()
    }
}

/// 등록을 쥐고 있다가 drop되면 해제한다. 해제는 이벤트 잠금을 기다릴 수 있으므로
/// 비동기 워커가 아닌 blocking 풀에서 한다.
struct Registration {
    registry: Arc<dyn SinkRegistry>,
    voice_session_id: String,
    sink: Arc<dyn VoiceEventSink>,
}

impl Drop for Registration {
    fn drop(&mut self) {
        let registry = Arc::clone(&self.registry);
        let voice_session_id = std::mem::take(&mut self.voice_session_id);
        let sink = Arc::clone(&self.sink);
        let job = move || registry.unregister(&voice_session_id, &sink);
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                handle.spawn_blocking(job);
            }
            Err(_) => job(),
        }
    }
}

pub struct EventStream {
    rx: UnboundedReceiver<QueuedEvent>,
    deadline: Pin<Box<Sleep>>,
    registration: Option<Registration>,
}

impl Stream for EventStream {
    type Item = Result<Event, Infallible>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = &mut *self;
        if this.registration.is_none() {
            return Poll::Ready(None);
        }
        // 이미 쌓인 이벤트를 먼저 내보낸 뒤에 시간 초과를 본다.
        loop {
            match this.rx.poll_recv(cx) {
                Poll::Ready(Some((name, payload))) => {
                    match Event::default().event(name).json_data(&payload) {
                        Ok(event) => return Poll::Ready(Some(Ok(event))),
                        Err(error) => {
                            tracing::warn!(%error, "dropping unserializable SSE payload");
                        }
                    }
                }
                Poll::Ready(None) => {
                    this.registration = None;
                    return Poll::Ready(None);
                }
                Poll::Pending => break,
            }
        }
        if this.deadline.as_mut().poll(cx).is_ready() {
            // 이후 `send`는 `false`를 돌려주어 유스케이스가 싱크를 버린다.
            this.rx.close();
            this.registration = None;
            return Poll::Ready(None);
        }
        Poll::Pending
    }
}

/// 싱크를 등록하고 SSE 응답을 만든다. 등록은 이전 이벤트 재전송을 포함하며 blocking 풀에서 한다.
pub async fn open_event_stream(
    registry: Arc<dyn SinkRegistry>,
    voice_session_id: String,
    since_event_id: Option<i64>,
    timeout: Duration,
) -> Result<Sse<EventStream>, ApiError> {
    let (tx, rx) = unbounded_channel();
    let sink: Arc<dyn VoiceEventSink> = Arc::new(ChannelSink { tx });
    {
        let registry = Arc::clone(&registry);
        let sink = Arc::clone(&sink);
        let voice_session_id = voice_session_id.clone();
        blocking(move || registry.register(&voice_session_id, sink, since_event_id)).await?;
    }
    Ok(Sse::new(EventStream {
        rx,
        deadline: Box::pin(sleep(timeout)),
        registration: Some(Registration {
            registry,
            voice_session_id,
            sink,
        }),
    }))
}
