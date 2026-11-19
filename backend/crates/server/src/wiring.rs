//! 어댑터와 유스케이스 조립. Java `UseCaseConfig`와 각 어댑터의 `@Component` 조합을 따른다.
//!
//! 블로킹 HTTP 클라이언트(`reqwest::blocking`)를 만드는 일이 들어 있으므로 비동기 작업자
//! 스레드가 아닌 블로킹 스레드에서 부른다.

use std::path::Path;
use std::sync::Arc;

use anyhow::{bail, Context};
use omypic_application::apikey::usecase::ApiKeyVerifyUseCase;
use omypic_application::datatransfer::usecase::DataTransferUseCase;
use omypic_application::feedback::port::LlmClient;
use omypic_application::feedback::usecase::FeedbackUseCase;
use omypic_application::question::usecase::QuestionUseCase;
use omypic_application::rulebook::port::EmbeddingPort;
use omypic_application::rulebook::usecase::RulebookUseCase;
use omypic_application::session::usecase::SessionUseCase;
use omypic_application::stt::usecase::SttUseCase;
use omypic_application::voice::usecase::{VoiceSessionDependencies, VoiceSessionUseCase};
use omypic_application::wrongnote::usecase::WrongNoteUseCase;
use omypic_infrastructure::anthropic::{build_anthropic_http_client, AnthropicLlmClient};
use omypic_infrastructure::config::AppConfig;
use omypic_infrastructure::embedding::{FastTextEmbeddingAdapter, LocalHashEmbeddingAdapter};
use omypic_infrastructure::executor::ThreadPoolExecutor;
use omypic_infrastructure::metrics::InMemoryMetrics;
use omypic_infrastructure::openai::{
    build_http_client, build_probe_http_client, HttpApiKeyProbeAdapter, OpenAiLlmClient,
    OpenAiSttGateway, OpenAiTtsGateway,
};
use omypic_infrastructure::session::InMemorySessionStore;
use omypic_infrastructure::sqlite::{
    Database, SqliteQuestionGroupAdapter, SqliteRulebookAdapter, SqliteRulebookIndexAdapter,
    SqliteWrongNoteAdapter, SqliteWrongNoteRecentQueueAdapter,
};
use omypic_infrastructure::storage::FileSystemRulebookStore;
use omypic_presentation::AppState;

/// 데이터 폴더 안의 SQLite 파일 이름.
pub const DATABASE_FILE: &str = "omypic.db";

/// 데이터 폴더를 만들고 DB를 열어 모든 어댑터와 유스케이스를 조립한다.
/// `AppState.metrics`와 유스케이스의 `MetricsPort`는 같은 `InMemoryMetrics`다.
pub fn build_state(config: &AppConfig) -> anyhow::Result<AppState> {
    let data_dir = Path::new(&config.data_dir);
    std::fs::create_dir_all(data_dir)
        .with_context(|| format!("Failed to create data directory {}", data_dir.display()))?;
    let db_path = data_dir.join(DATABASE_FILE);
    let db = Database::open(&db_path)
        .with_context(|| format!("Failed to open database {}", db_path.display()))?;

    let config_arc = Arc::new(config.clone());
    let metrics = Arc::new(InMemoryMetrics::new());
    let session_store = Arc::new(InMemorySessionStore::new());
    let question_groups = Arc::new(SqliteQuestionGroupAdapter::new(db.clone()));
    let rulebooks = Arc::new(SqliteRulebookAdapter::new(db.clone()));
    let wrong_notes = Arc::new(SqliteWrongNoteAdapter::new(db.clone()));
    let wrong_note_queue = Arc::new(SqliteWrongNoteRecentQueueAdapter::new(db.clone()));
    let rulebook_files = Arc::new(FileSystemRulebookStore::new(config_arc.clone()));
    let rulebook_index = Arc::new(SqliteRulebookIndexAdapter::new(db, embedding_port(config)?));

    let openai = &config.integrations.openai;
    let anthropic = &config.integrations.anthropic;
    let openai_http = build_http_client()?;
    let llm_clients: Vec<Arc<dyn LlmClient>> = vec![
        Arc::new(OpenAiLlmClient::new(openai_http.clone(), openai)),
        Arc::new(AnthropicLlmClient::new(
            build_anthropic_http_client()?,
            anthropic,
        )),
    ];
    let stt_gateway = Arc::new(OpenAiSttGateway::new(openai_http.clone(), openai));
    let tts_gateway = Arc::new(OpenAiTtsGateway::new(openai_http, openai));
    let key_probe = Arc::new(HttpApiKeyProbeAdapter::new(
        build_probe_http_client()?,
        openai,
        anthropic,
    ));

    let question = Arc::new(QuestionUseCase::new(
        question_groups.clone(),
        session_store.clone(),
        metrics.clone(),
    ));
    let session = Arc::new(SessionUseCase::new(
        session_store.clone(),
        question_groups.clone(),
    ));
    let rulebook = Arc::new(RulebookUseCase::new(
        rulebooks.clone(),
        rulebook_index.clone(),
        rulebook_files.clone(),
        config_arc.clone(),
        metrics.clone(),
    ));
    let wrong_note = Arc::new(WrongNoteUseCase::new(
        wrong_notes.clone(),
        wrong_note_queue.clone(),
        config_arc.clone(),
    ));
    let feedback = Arc::new(FeedbackUseCase::with_conversation_rebase_turns(
        llm_clients,
        rulebook.clone(),
        config_arc.clone(),
        metrics.clone(),
        session_store,
        wrong_note.clone(),
        i32::try_from(openai.conversation_rebase_turns).unwrap_or(i32::MAX),
    )?);
    let stt = Arc::new(SttUseCase::new(
        stt_gateway,
        config_arc.clone(),
        metrics.clone(),
    ));
    let voice_session = Arc::new(VoiceSessionUseCase::new(VoiceSessionDependencies {
        stt_use_case: stt,
        feedback_use_case: feedback,
        session_use_case: session.clone(),
        question_use_case: question.clone(),
        tts_gateway,
        async_executor: Arc::new(ThreadPoolExecutor::new()),
        voice_policy: config_arc.clone(),
        metrics: metrics.clone(),
    }));
    let api_key_verify = Arc::new(ApiKeyVerifyUseCase::new(key_probe));
    let data_transfer = Arc::new(DataTransferUseCase::new(
        question_groups,
        rulebooks,
        wrong_notes,
        wrong_note_queue,
        rulebook_files,
        rulebook_index,
    ));

    Ok(AppState {
        api_key_verify,
        data_transfer,
        question,
        rulebook,
        session,
        voice_session,
        wrong_note,
        voice_policy: config_arc,
        metrics,
    })
}

/// `omypic.rag.provider`로 임베딩을 고른다. Java의 `@ConditionalOnProperty`와 같이
/// `hash`면 해시, `fasttext`면(기본값) fastText이고 대소문자는 가리지 않는다.
/// fastText 모델은 여기서 읽지 않는다(첫 임베딩 때 읽거나 내려받는다).
pub fn embedding_port(config: &AppConfig) -> anyhow::Result<Arc<dyn EmbeddingPort>> {
    let provider = config.rag.provider.as_str();
    if provider.eq_ignore_ascii_case("hash") {
        return Ok(Arc::new(LocalHashEmbeddingAdapter::new(
            config.rag.embedding_dim,
        )));
    }
    if provider.eq_ignore_ascii_case("fasttext") {
        return Ok(Arc::new(FastTextEmbeddingAdapter::new(
            config.rag.clone(),
            Arc::new(config.clone()),
        )));
    }
    bail!("Unsupported rag provider '{provider}' (OMYPIC_RAG_PROVIDER must be fasttext or hash)")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_with_provider(provider: &str) -> AppConfig {
        let mut config = AppConfig::default();
        config.rag.provider = provider.to_string();
        config
    }

    #[test]
    fn rag_provider_selects_embedding_adapter() {
        assert_eq!(
            "fasttext",
            embedding_port(&AppConfig::default()).unwrap().provider()
        );
        assert_eq!(
            "fasttext",
            embedding_port(&config_with_provider("FastText"))
                .unwrap()
                .provider()
        );
        assert_eq!(
            "hash",
            embedding_port(&config_with_provider("hash"))
                .unwrap()
                .provider()
        );
        assert_eq!(
            "hash",
            embedding_port(&config_with_provider("HASH"))
                .unwrap()
                .provider()
        );
    }

    #[test]
    fn unknown_rag_provider_is_rejected() {
        let error = embedding_port(&config_with_provider("lucene"))
            .err()
            .unwrap();
        assert_eq!(
            "Unsupported rag provider 'lucene' (OMYPIC_RAG_PROVIDER must be fasttext or hash)",
            error.to_string()
        );
    }
}
