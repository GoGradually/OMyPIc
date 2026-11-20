use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

use chrono::Utc;
use omypic_domain::error::{AppError, AppResult};
use omypic_domain::question::QuestionGroup;
use omypic_domain::rulebook::{Rulebook, RulebookContext, RulebookId, RulebookScope};
use omypic_domain::shared::text_utils::split_chunks;

use super::policy::RagPolicy;
use super::port::{RulebookFileStore, RulebookIndexPort, RulebookPort};
use crate::shared::port::MetricsPort;

const CHUNK_SIZE: usize = 800;

pub struct RulebookUseCase {
    repository: Arc<dyn RulebookPort>,
    index_port: Arc<dyn RulebookIndexPort>,
    file_store: Arc<dyn RulebookFileStore>,
    rag_policy: Arc<dyn RagPolicy>,
    metrics: Arc<dyn MetricsPort>,
}

impl RulebookUseCase {
    pub fn new(
        repository: Arc<dyn RulebookPort>,
        index_port: Arc<dyn RulebookIndexPort>,
        file_store: Arc<dyn RulebookFileStore>,
        rag_policy: Arc<dyn RagPolicy>,
        metrics: Arc<dyn MetricsPort>,
    ) -> Self {
        Self {
            repository,
            index_port,
            file_store,
            rag_policy,
            metrics,
        }
    }

    pub fn upload(
        &self,
        filename: Option<&str>,
        bytes: &[u8],
        scope: Option<RulebookScope>,
        question_group: Option<&str>,
    ) -> AppResult<Rulebook> {
        let start = Instant::now();
        let filename = validate_markdown_filename(filename)?;
        if std::str::from_utf8(bytes).is_err() {
            return Err(AppError::InvalidArgument(
                "Rulebook must be UTF-8 text".to_string(),
            ));
        }
        let scope = scope.unwrap_or(RulebookScope::Main);
        let question_group = QuestionGroup::from_nullable(question_group);
        // 파일을 쓰기 전에 범위와 질문 그룹 조합을 검증한다.
        Rulebook::create(filename, "", scope, question_group.clone(), Utc::now())?;
        let stored = self.file_store.store(filename, bytes)?;
        // 파일, 색인, 행 순서로 쓰고, 실패하면 앞에서 쓴 것을 지운다. 행이 없는 조각은
        // 검색에 쓰이지 않는다(검색은 활성 룰북 행의 id로 거른다).
        let rulebook =
            match Rulebook::create(filename, &stored.path, scope, question_group, Utc::now()) {
                Ok(rulebook) => rulebook,
                Err(error) => return Err(self.discard_file(&stored.path, error)),
            };
        if let Err(error) = self.index_rulebook(&rulebook, filename) {
            return Err(self.discard_file(&stored.path, error));
        }
        let saved = match self.repository.save(rulebook.clone()) {
            Ok(saved) => saved,
            Err(error) => {
                if let Err(cleanup) = self.index_port.delete_rulebook_chunks(rulebook.id()) {
                    tracing::warn!(
                        "Failed to remove chunks of rulebook {} after failed upload: {cleanup}",
                        rulebook.id().value()
                    );
                }
                return Err(self.discard_file(&stored.path, error));
            }
        };
        self.metrics.record_rulebook_upload_latency(start.elapsed());
        Ok(saved)
    }

    fn index_rulebook(&self, rulebook: &Rulebook, filename: &str) -> AppResult<()> {
        let text = self.file_store.read_text(rulebook.path())?;
        let chunks = split_chunks(Some(&text), CHUNK_SIZE);
        self.index_port
            .index_rulebook_chunks(rulebook.id(), filename, &chunks)
    }

    /// 실패한 업로드가 저장한 파일을 지우고 원래 오류를 돌려준다.
    fn discard_file(&self, path: &str, error: AppError) -> AppError {
        if let Err(cleanup) = self.file_store.delete(path) {
            tracing::warn!("Failed to remove rulebook file {path} after failed upload: {cleanup}");
        }
        error
    }

    pub fn list(&self) -> AppResult<Vec<Rulebook>> {
        self.repository.find_all()
    }

    pub fn toggle(&self, id: &str, enabled: bool) -> AppResult<Rulebook> {
        let mut rulebook = self
            .repository
            .find_by_id(&RulebookId::of(id)?)?
            .ok_or_else(|| AppError::NotFound("No value present".to_string()))?;
        rulebook.toggle(enabled, Utc::now());
        self.repository.save(rulebook)
    }

    /// 색인 조각, 행, 파일 순서로 지운다. 중간에 멈춰도 행 없는 조각은 남지 않는다(행만 남으면
    /// 다시 지우면 되고, 남은 파일은 아무 데도 쓰이지 않는다). 모르는 id면 아무것도 하지 않는다.
    pub fn delete(&self, id: &str) -> AppResult<()> {
        let id = RulebookId::of(id)?;
        let Some(rulebook) = self.repository.find_by_id(&id)? else {
            return Ok(());
        };
        self.index_port.delete_rulebook_chunks(&id)?;
        self.repository.delete_by_id(&id)?;
        if let Err(error) = self.file_store.delete(rulebook.path()) {
            tracing::warn!(
                "Rulebook {} was deleted but its file {} could not be removed: {error}",
                id.value(),
                rulebook.path()
            );
        }
        Ok(())
    }

    pub fn search_contexts(&self, query: &str) -> AppResult<Vec<RulebookContext>> {
        let enabled_ids: HashSet<RulebookId> = self
            .enabled_rulebooks()?
            .iter()
            .map(|rulebook| rulebook.id().clone())
            .collect();
        self.search_by_ids(
            query,
            self.rag_policy.get_max_context_chunks(),
            &enabled_ids,
        )
    }

    pub fn search_contexts_for_turn(
        &self,
        question_group: Option<&QuestionGroup>,
        query: &str,
        max_documents: usize,
    ) -> AppResult<Vec<RulebookContext>> {
        if max_documents == 0 {
            return Ok(Vec::new());
        }
        let enabled = self.enabled_rulebooks()?;
        if enabled.is_empty() {
            return Ok(Vec::new());
        }
        let main_ids = rulebook_ids(&enabled, |r| r.scope() == RulebookScope::Main);
        let question_ids = rulebook_ids(&enabled, |r| {
            r.scope() == RulebookScope::Question
                && question_group.is_some()
                && question_group == r.question_group()
        });
        let mut selected = Vec::new();
        add_unique(
            &mut selected,
            self.search_by_ids(query, 1, &main_ids)?,
            max_documents,
        );
        let remaining = max_documents.saturating_sub(selected.len());
        add_unique(
            &mut selected,
            self.search_by_ids(query, remaining, &question_ids)?,
            max_documents,
        );
        add_unique(
            &mut selected,
            self.search_by_ids(query, max_documents, &main_ids)?,
            max_documents,
        );
        add_unique(
            &mut selected,
            self.search_by_ids(query, max_documents, &question_ids)?,
            max_documents,
        );
        Ok(selected)
    }

    fn enabled_rulebooks(&self) -> AppResult<Vec<Rulebook>> {
        Ok(self
            .repository
            .find_all()?
            .into_iter()
            .filter(Rulebook::is_enabled)
            .collect())
    }

    /// 색인 I/O 오류(`AppError::Internal`)는 검색 결과 없음으로 취급한다.
    fn search_by_ids(
        &self,
        query: &str,
        limit: usize,
        ids: &HashSet<RulebookId>,
    ) -> AppResult<Vec<RulebookContext>> {
        if ids.is_empty() || limit == 0 {
            return Ok(Vec::new());
        }
        match self.index_port.search(query, limit, ids) {
            Err(AppError::Internal(message)) => {
                tracing::warn!(
                    "Rulebook search failed; continuing without rulebook context: {message}"
                );
                Ok(Vec::new())
            }
            other => other,
        }
    }
}

fn validate_markdown_filename(filename: Option<&str>) -> AppResult<&str> {
    match filename {
        Some(name) if name.ends_with(".md") => Ok(name),
        _ => Err(AppError::InvalidArgument(
            "Only .md files are supported".to_string(),
        )),
    }
}

fn rulebook_ids(
    rulebooks: &[Rulebook],
    predicate: impl Fn(&Rulebook) -> bool,
) -> HashSet<RulebookId> {
    rulebooks
        .iter()
        .filter(|r| predicate(r))
        .map(|r| r.id().clone())
        .collect()
}

fn add_unique(target: &mut Vec<RulebookContext>, candidates: Vec<RulebookContext>, limit: usize) {
    for candidate in candidates {
        if target.len() >= limit {
            return;
        }
        let duplicate = target.iter().any(|existing| {
            existing.rulebook_id() == candidate.rulebook_id() && existing.text() == candidate.text()
        });
        if !duplicate {
            target.push(candidate);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        FakeRagPolicy, InMemoryRulebookFileStore, InMemoryRulebookIndexPort, InMemoryRulebookPort,
        RecordingMetrics,
    };
    use chrono::{TimeZone, Utc};
    use omypic_domain::error::AppError;
    use omypic_domain::rulebook::RulebookId;
    use std::collections::HashSet;

    struct Fixture {
        repository: Arc<InMemoryRulebookPort>,
        index: Arc<InMemoryRulebookIndexPort>,
        file_store: Arc<InMemoryRulebookFileStore>,
        metrics: Arc<RecordingMetrics>,
        use_case: RulebookUseCase,
    }

    fn fixture(rulebooks: Vec<Rulebook>) -> Fixture {
        let repository = Arc::new(InMemoryRulebookPort::with_rulebooks(rulebooks));
        let index = Arc::new(InMemoryRulebookIndexPort::new());
        let file_store = Arc::new(InMemoryRulebookFileStore::new());
        let metrics = Arc::new(RecordingMetrics::new());
        let use_case = RulebookUseCase::new(
            repository.clone(),
            index.clone(),
            file_store.clone(),
            Arc::new(FakeRagPolicy {
                max_context_chunks: 3,
            }),
            metrics.clone(),
        );
        Fixture {
            repository,
            index,
            file_store,
            metrics,
            use_case,
        }
    }

    fn rulebook(
        id: &str,
        filename: &str,
        scope: RulebookScope,
        question_group: Option<&str>,
        enabled: bool,
    ) -> Rulebook {
        let at = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        Rulebook::rehydrate(
            RulebookId::of(id).unwrap(),
            filename,
            &format!("/tmp/{filename}"),
            Some(scope),
            question_group.map(|g| QuestionGroup::of(g).unwrap()),
            enabled,
            Some(at),
            Some(at),
        )
        .unwrap()
    }

    fn ids(values: &[&str]) -> HashSet<RulebookId> {
        values.iter().map(|v| RulebookId::of(v).unwrap()).collect()
    }

    fn context(id: &str, filename: &str, text: &str) -> RulebookContext {
        RulebookContext::of(RulebookId::of(id).unwrap(), Some(filename), Some(text))
    }

    #[test]
    fn upload_rejects_non_markdown_files() {
        let f = fixture(vec![]);

        let error = f
            .use_case
            .upload(Some("rulebook.txt"), &[1], Some(RulebookScope::Main), None)
            .unwrap_err();

        assert_eq!(
            AppError::InvalidArgument("Only .md files are supported".to_string()),
            error
        );
        assert!(f.file_store.files().is_empty());
    }

    #[test]
    fn upload_rejects_question_group_in_main_scope() {
        let f = fixture(vec![]);

        let error = f
            .use_case
            .upload(
                Some("rulebook.md"),
                &[1],
                Some(RulebookScope::Main),
                Some("A"),
            )
            .unwrap_err();

        assert!(matches!(error, AppError::InvalidArgument(_)));
        assert!(f.file_store.files().is_empty());
    }

    #[test]
    fn upload_requires_question_group_in_question_scope() {
        let f = fixture(vec![]);

        let error = f
            .use_case
            .upload(
                Some("rulebook.md"),
                &[1],
                Some(RulebookScope::Question),
                Some(" "),
            )
            .unwrap_err();

        assert!(matches!(error, AppError::InvalidArgument(_)));
        assert!(f.file_store.files().is_empty());
    }

    #[test]
    fn upload_stores_saves_reads_and_indexes() {
        let f = fixture(vec![]);
        let bytes = b"line1\nline2\nline3";

        let saved = f
            .use_case
            .upload(
                Some("rulebook.md"),
                bytes,
                Some(RulebookScope::Question),
                Some("A"),
            )
            .unwrap();

        assert_eq!("rulebook.md", saved.filename());
        assert_eq!("/tmp/rulebook.md", saved.path());
        assert_eq!(RulebookScope::Question, saved.scope());
        assert_eq!(
            Some(&QuestionGroup::of("A").unwrap()),
            saved.question_group()
        );
        assert_eq!(
            vec![("/tmp/rulebook.md".to_string(), bytes.to_vec())],
            f.file_store.files()
        );
        assert_eq!(vec!["/tmp/rulebook.md"], f.file_store.read_text_calls());
        let indexed = f.index.indexed();
        assert_eq!(1, indexed.len());
        assert_eq!(saved.id(), &indexed[0].0);
        assert_eq!("rulebook.md", indexed[0].1);
        assert!(!indexed[0].2.is_empty());
        let stored = f.repository.rulebooks();
        assert_eq!(1, stored.len());
        assert_eq!(saved.id(), stored[0].id());
        assert_eq!(1, f.metrics.count("record_rulebook_upload_latency"));
    }

    fn assert_nothing_left_behind(f: &Fixture) {
        assert!(f.repository.rulebooks().is_empty());
        assert!(f.file_store.files().is_empty());
        assert!(f.index.indexed().is_empty());
    }

    #[test]
    fn upload_rejects_non_utf8_markdown_and_leaves_nothing_behind() {
        let f = fixture(vec![]);

        // CP949로 저장한 "규칙"
        let error = f
            .use_case
            .upload(Some("rules.md"), &[0xb1, 0xd4, 0xc4, 0xa2], None, None)
            .unwrap_err();

        assert_eq!(
            AppError::InvalidArgument("Rulebook must be UTF-8 text".to_string()),
            error
        );
        assert_nothing_left_behind(&f);
    }

    #[test]
    fn upload_that_fails_to_index_leaves_nothing_behind() {
        let f = fixture(vec![]);
        let failure =
            AppError::InvalidArgument("Failed to download FastText model after retries".into());
        f.index.fail_index_with(failure.clone());

        let error = f
            .use_case
            .upload(Some("rules.md"), b"# rules", None, None)
            .unwrap_err();

        assert_eq!(failure, error);
        assert_nothing_left_behind(&f);
        assert_eq!(0, f.metrics.count("record_rulebook_upload_latency"));
    }

    #[test]
    fn upload_that_fails_to_save_the_row_leaves_nothing_behind() {
        let f = fixture(vec![]);
        let failure = AppError::Internal("database is locked".to_string());
        f.repository.fail_save_with(failure.clone());

        let error = f
            .use_case
            .upload(Some("rules.md"), b"# rules", None, None)
            .unwrap_err();

        assert_eq!(failure, error);
        assert_nothing_left_behind(&f);
    }

    #[test]
    fn upload_defaults_to_main_scope_when_scope_is_missing() {
        let f = fixture(vec![]);

        let saved = f
            .use_case
            .upload(Some("rulebook.md"), b"hello", None, None)
            .unwrap();

        assert_eq!(RulebookScope::Main, saved.scope());
    }

    #[test]
    fn search_contexts_returns_empty_when_no_enabled_rulebooks() {
        let disabled = rulebook("r1", "rulebook.md", RulebookScope::Main, None, false);
        let f = fixture(vec![disabled]);

        let contexts = f.use_case.search_contexts("grammar").unwrap();

        assert!(contexts.is_empty());
        assert!(f.index.search_calls().is_empty());
    }

    #[test]
    fn search_contexts_returns_empty_when_index_throws_io_exception() {
        let enabled = rulebook("r1", "rulebook.md", RulebookScope::Main, None, true);
        let f = fixture(vec![enabled]);
        f.index
            .fail_search_with(AppError::Internal("index error".to_string()));

        let contexts = f.use_case.search_contexts("grammar").unwrap();

        assert!(contexts.is_empty());
        let calls = f.index.search_calls();
        assert_eq!(1, calls.len());
        assert_eq!("grammar", calls[0].query);
        assert_eq!(3, calls[0].top_k);
    }

    /// `f`를 실행하는 동안 이 스레드에서 남긴 로그를 모은다.
    fn capture_logs(f: impl FnOnce()) -> String {
        use std::sync::Mutex;

        #[derive(Clone, Default)]
        struct Buffer(Arc<Mutex<Vec<u8>>>);
        impl std::io::Write for Buffer {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        let buffer = Buffer::default();
        let writer = buffer.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(move || writer.clone())
            .with_ansi(false)
            .finish();
        tracing::subscriber::with_default(subscriber, f);
        let bytes = buffer.0.lock().unwrap().clone();
        String::from_utf8(bytes).unwrap()
    }

    #[test]
    fn search_contexts_logs_a_warning_when_index_search_fails() {
        let enabled = rulebook("r1", "rulebook.md", RulebookScope::Main, None, true);
        let f = fixture(vec![enabled]);
        f.index
            .fail_search_with(AppError::Internal("index error".to_string()));

        let logs = capture_logs(|| {
            assert!(f.use_case.search_contexts("grammar").unwrap().is_empty());
        });

        assert!(logs.contains("WARN"), "{logs}");
        assert!(logs.contains("index error"), "{logs}");
    }

    #[test]
    fn toggle_updates_enabled_state_and_saves() {
        let existing = rulebook("r1", "rulebook.md", RulebookScope::Main, None, true);
        let f = fixture(vec![existing]);

        let toggled = f.use_case.toggle("r1", false).unwrap();

        assert!(!toggled.is_enabled());
        assert_eq!(1, f.repository.save_calls());
        assert!(!f.repository.rulebooks()[0].is_enabled());
    }

    #[test]
    fn toggle_throws_when_rulebook_is_missing() {
        let f = fixture(vec![]);

        let error = f.use_case.toggle("missing", true).unwrap_err();

        assert_eq!(AppError::NotFound("No value present".to_string()), error);
    }

    #[test]
    fn delete_delegates_to_repository() {
        let f = fixture(vec![rulebook(
            "r1",
            "rulebook.md",
            RulebookScope::Main,
            None,
            true,
        )]);

        f.use_case.delete("r1").unwrap();

        assert_eq!(vec!["r1"], f.repository.deleted_ids());
    }

    #[test]
    fn delete_removes_row_index_chunks_and_file() {
        let f = fixture(vec![]);
        let uploaded = f
            .use_case
            .upload(Some("rules.md"), b"# rules", None, None)
            .unwrap();
        let keep = f
            .use_case
            .upload(Some("keep.md"), b"# keep", None, None)
            .unwrap();

        f.use_case.delete(uploaded.id().value()).unwrap();

        assert_eq!(vec![keep.id().clone()], ids_of(&f.repository.rulebooks()));
        let indexed: Vec<RulebookId> = f.index.indexed().into_iter().map(|e| e.0).collect();
        assert_eq!(vec![keep.id().clone()], indexed);
        let files: Vec<String> = f.file_store.files().into_iter().map(|e| e.0).collect();
        assert_eq!(vec![keep.path().to_string()], files);
    }

    #[test]
    fn delete_that_fails_on_the_row_leaves_no_chunks_and_keeps_the_file() {
        let f = fixture(vec![]);
        let uploaded = f
            .use_case
            .upload(Some("rules.md"), b"# rules", None, None)
            .unwrap();
        f.repository
            .fail_delete_with(AppError::Internal("disk full".to_string()));

        let error = f.use_case.delete(uploaded.id().value()).unwrap_err();

        assert_eq!(AppError::Internal("disk full".to_string()), error);
        // Chunks go first: a rulebook row without chunks can be deleted again; chunks without a
        // row would never be cleaned up. The file goes last (a leftover file is harmless).
        assert!(f.index.indexed().is_empty());
        assert_eq!(1, f.file_store.files().len());
        assert!(f.file_store.deleted().is_empty());
    }

    #[test]
    fn deleting_an_unknown_rulebook_is_a_no_op() {
        let f = fixture(vec![]);

        f.use_case.delete("missing").unwrap();

        assert!(f.file_store.deleted().is_empty());
    }

    fn ids_of(rulebooks: &[Rulebook]) -> Vec<RulebookId> {
        rulebooks.iter().map(|r| r.id().clone()).collect()
    }

    #[test]
    fn search_contexts_for_turn_prioritizes_main_and_question_and_limits_to_two() {
        let main = rulebook("r-main", "main.md", RulebookScope::Main, None, true);
        let question_a = rulebook("r-q-a", "q-a.md", RulebookScope::Question, Some("A"), true);
        let question_b = rulebook("r-q-b", "q-b.md", RulebookScope::Question, Some("B"), true);
        let f = fixture(vec![main, question_a, question_b]);
        let main_ctx = context("r-main", "main.md", "main ctx");
        let group_a_ctx = context("r-q-a", "q-a.md", "group A ctx");
        f.index
            .stub_search(1, ids(&["r-main"]), vec![main_ctx.clone()]);
        f.index
            .stub_search(1, ids(&["r-q-a"]), vec![group_a_ctx.clone()]);
        f.index.stub_search(2, ids(&["r-main"]), vec![main_ctx]);
        f.index.stub_search(2, ids(&["r-q-a"]), vec![group_a_ctx]);

        let contexts = f
            .use_case
            .search_contexts_for_turn(Some(&QuestionGroup::of("A").unwrap()), "answer", 2)
            .unwrap();

        assert_eq!(2, contexts.len());
        assert_eq!("r-main", contexts[0].rulebook_id().value());
        assert_eq!("r-q-a", contexts[1].rulebook_id().value());
    }

    #[test]
    fn search_contexts_for_turn_returns_empty_when_max_documents_is_zero() {
        let f = fixture(vec![rulebook(
            "r1",
            "rulebook.md",
            RulebookScope::Main,
            None,
            true,
        )]);

        let contexts = f
            .use_case
            .search_contexts_for_turn(None, "answer", 0)
            .unwrap();

        assert!(contexts.is_empty());
        assert!(f.index.search_calls().is_empty());
    }
}
