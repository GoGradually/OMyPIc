use std::sync::Arc;
use std::time::Instant;

use omypic_domain::error::{AppError, AppResult};

use super::model::SttCommand;
use super::policy::SttPolicy;
use super::port::SttGateway;
use crate::shared::port::MetricsPort;

pub struct SttUseCase {
    stt_gateway: Arc<dyn SttGateway>,
    stt_policy: Arc<dyn SttPolicy>,
    metrics: Arc<dyn MetricsPort>,
}

impl SttUseCase {
    pub fn new(
        stt_gateway: Arc<dyn SttGateway>,
        stt_policy: Arc<dyn SttPolicy>,
        metrics: Arc<dyn MetricsPort>,
    ) -> Self {
        Self {
            stt_gateway,
            stt_policy,
            metrics,
        }
    }

    pub fn transcribe(&self, command: &SttCommand) -> AppResult<String> {
        let file_bytes = self.validate_file(command)?;
        self.metrics.increment_stt_request();
        let start = Instant::now();
        self.transcribe_with_retry(command, file_bytes, start)
            .map_err(|_| {
                self.metrics.increment_stt_error();
                AppError::InvalidState("STT failed".to_string())
            })
    }

    fn validate_file<'a>(&self, command: &'a SttCommand) -> AppResult<&'a [u8]> {
        match command.file_bytes.as_deref() {
            Some(bytes) if bytes.len() as u64 <= self.stt_policy.get_max_file_bytes() => Ok(bytes),
            _ => Err(AppError::InvalidArgument("File too large".to_string())),
        }
    }

    fn transcribe_with_retry(
        &self,
        command: &SttCommand,
        file_bytes: &[u8],
        start: Instant,
    ) -> AppResult<String> {
        let mut last_error = AppError::InvalidState("STT failed".to_string());
        for _ in 0..=self.stt_policy.retry_max() {
            match self.transcribe_once(command, file_bytes, start) {
                Ok(text) => return Ok(text),
                Err(error) => last_error = error,
            }
        }
        Err(last_error)
    }

    fn transcribe_once(
        &self,
        command: &SttCommand,
        file_bytes: &[u8],
        start: Instant,
    ) -> AppResult<String> {
        let text = self.stt_gateway.transcribe(
            file_bytes,
            &command.model,
            &command.api_key,
            command.translate,
            &self.stt_policy.get_vad_settings(),
        )?;
        self.metrics.record_stt_latency(start.elapsed());
        Ok(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stt::model::VadSettings;
    use crate::test_support::{FakeSttPolicy, RecordingMetrics, ScriptedSttGateway, SttCall};
    use omypic_domain::error::AppError;

    fn vad() -> VadSettings {
        VadSettings {
            prefix_padding_ms: 300,
            silence_duration_ms: 500,
            threshold: 0.6,
        }
    }

    fn command_with_bytes(size: usize) -> SttCommand {
        SttCommand {
            file_bytes: Some(vec![0; size]),
            model: "gpt-4o-mini-transcribe".to_string(),
            api_key: "key".to_string(),
            translate: false,
            session_id: None,
        }
    }

    fn setup(
        retry_max: u32,
        script: Vec<AppResult<String>>,
    ) -> (Arc<ScriptedSttGateway>, Arc<RecordingMetrics>, SttUseCase) {
        let gateway = Arc::new(ScriptedSttGateway::new(script));
        let metrics = Arc::new(RecordingMetrics::new());
        let policy = Arc::new(FakeSttPolicy {
            max_file_bytes: 100,
            retry_max,
            vad_settings: vad(),
        });
        let use_case = SttUseCase::new(gateway.clone(), policy, metrics.clone());
        (gateway, metrics, use_case)
    }

    fn failure(message: &str) -> AppResult<String> {
        Err(AppError::Internal(message.to_string()))
    }

    #[test]
    fn transcribe_retries_and_succeeds_within_retry_limit() {
        let (gateway, metrics, use_case) = setup(
            2,
            vec![
                failure("temporary-1"),
                failure("temporary-2"),
                Ok("final-text".to_string()),
            ],
        );

        let result = use_case.transcribe(&command_with_bytes(10)).unwrap();

        assert_eq!("final-text", result);
        let expected = SttCall {
            file_bytes: vec![0; 10],
            model: "gpt-4o-mini-transcribe".to_string(),
            api_key: "key".to_string(),
            translate: false,
            vad_settings: vad(),
        };
        assert_eq!(vec![expected; 3], gateway.calls());
        assert_eq!(1, metrics.count("increment_stt_request"));
        assert_eq!(1, metrics.count("record_stt_latency"));
        assert_eq!(0, metrics.count("increment_stt_error"));
    }

    #[test]
    fn transcribe_throws_and_increments_error_when_retries_exhausted() {
        let (gateway, metrics, use_case) = setup(2, vec![failure("always-fail")]);

        let error = use_case.transcribe(&command_with_bytes(10)).unwrap_err();

        assert_eq!(AppError::InvalidState("STT failed".to_string()), error);
        assert_eq!(3, gateway.calls().len());
        assert_eq!(1, metrics.count("increment_stt_request"));
        assert_eq!(0, metrics.count("record_stt_latency"));
        assert_eq!(1, metrics.count("increment_stt_error"));
    }

    #[test]
    fn transcribe_rejects_null_or_too_large_files() {
        let (gateway, metrics, use_case) = setup(2, vec![Ok("unused".to_string())]);
        let null_file = SttCommand {
            file_bytes: None,
            ..command_with_bytes(1)
        };
        let too_large = command_with_bytes(101);

        let null_error = use_case.transcribe(&null_file).unwrap_err();
        let large_error = use_case.transcribe(&too_large).unwrap_err();

        assert_eq!(
            AppError::InvalidArgument("File too large".to_string()),
            null_error
        );
        assert_eq!(
            AppError::InvalidArgument("File too large".to_string()),
            large_error
        );
        assert!(gateway.calls().is_empty());
        assert_eq!(0, metrics.count("increment_stt_request"));
    }

    #[test]
    fn transcribe_passes_model_api_key_translate_and_vad_to_gateway() {
        let (gateway, metrics, use_case) = setup(2, vec![Ok("ok".to_string())]);
        let command = SttCommand {
            file_bytes: Some(vec![0; 5]),
            model: "whisper-1".to_string(),
            api_key: "secret".to_string(),
            translate: true,
            session_id: None,
        };

        let result = use_case.transcribe(&command).unwrap();

        assert_eq!("ok", result);
        assert_eq!(1, metrics.count("increment_stt_request"));
        assert_eq!(
            vec![SttCall {
                file_bytes: vec![0; 5],
                model: "whisper-1".to_string(),
                api_key: "secret".to_string(),
                translate: true,
                vad_settings: vad(),
            }],
            gateway.calls()
        );
    }

    #[test]
    fn transcribe_with_retry_max_zero_attempts_only_once() {
        let (gateway, metrics, use_case) = setup(0, vec![failure("fail-fast")]);

        let error = use_case.transcribe(&command_with_bytes(5)).unwrap_err();

        assert!(matches!(error, AppError::InvalidState(_)));
        assert_eq!(1, gateway.calls().len());
        assert_eq!(1, metrics.count("increment_stt_request"));
        assert_eq!(1, metrics.count("increment_stt_error"));
    }
}
