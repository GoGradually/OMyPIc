use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use omypic_domain::error::{AppError, AppResult};
use omypic_domain::question::{QuestionGroupAggregate, QuestionGroupId};
use omypic_domain::rulebook::{Rulebook, RulebookContext, RulebookId};
use omypic_domain::session::{SessionId, SessionState};
use omypic_domain::wrongnote::{WrongNote, WrongNoteId};

use super::lock;
use crate::question::port::QuestionGroupPort;
use crate::rulebook::model::StoredRulebookFile;
use crate::rulebook::port::{RulebookFileStore, RulebookIndexPort, RulebookPort};
use crate::session::port::{SessionStorePort, SharedSessionState};
use crate::wrongnote::port::{WrongNotePort, WrongNoteRecentQueuePort};

fn upsert<T, K: PartialEq>(items: &mut Vec<T>, item: T, key: impl Fn(&T) -> K) {
    let item_key = key(&item);
    match items.iter().position(|existing| key(existing) == item_key) {
        Some(index) => items[index] = item,
        None => items.push(item),
    }
}

#[derive(Debug, Default)]
pub struct InMemoryQuestionGroupPort {
    groups: Mutex<Vec<QuestionGroupAggregate>>,
    find_all_calls: Mutex<usize>,
    save_calls: Mutex<usize>,
    deleted_ids: Mutex<Vec<String>>,
}

impl InMemoryQuestionGroupPort {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_groups(groups: Vec<QuestionGroupAggregate>) -> Self {
        let port = Self::default();
        *lock(&port.groups) = groups;
        port
    }

    /// 저장된 그룹(저장한 순서).
    pub fn groups(&self) -> Vec<QuestionGroupAggregate> {
        lock(&self.groups).clone()
    }

    pub fn find_all_calls(&self) -> usize {
        *lock(&self.find_all_calls)
    }

    pub fn save_calls(&self) -> usize {
        *lock(&self.save_calls)
    }

    pub fn deleted_ids(&self) -> Vec<String> {
        lock(&self.deleted_ids).clone()
    }
}

impl QuestionGroupPort for InMemoryQuestionGroupPort {
    fn find_all(&self) -> AppResult<Vec<QuestionGroupAggregate>> {
        *lock(&self.find_all_calls) += 1;
        Ok(self.groups())
    }

    fn find_all_by_id(&self, ids: &[QuestionGroupId]) -> AppResult<Vec<QuestionGroupAggregate>> {
        Ok(lock(&self.groups)
            .iter()
            .filter(|group| ids.contains(group.id()))
            .cloned()
            .collect())
    }

    fn find_by_id(&self, id: &QuestionGroupId) -> AppResult<Option<QuestionGroupAggregate>> {
        Ok(lock(&self.groups)
            .iter()
            .find(|group| group.id() == id)
            .cloned())
    }

    fn save(&self, group: QuestionGroupAggregate) -> AppResult<QuestionGroupAggregate> {
        *lock(&self.save_calls) += 1;
        upsert(&mut lock(&self.groups), group.clone(), |g| g.id().clone());
        Ok(group)
    }

    fn delete_by_id(&self, id: &QuestionGroupId) -> AppResult<()> {
        lock(&self.deleted_ids).push(id.value().to_string());
        lock(&self.groups).retain(|group| group.id() != id);
        Ok(())
    }

    fn delete_all(&self) -> AppResult<()> {
        lock(&self.groups).clear();
        Ok(())
    }
}

/// 세션 id마다 상태 하나를 들고 있다. 같은 id는 같은 `Arc`를 돌려준다.
#[derive(Debug, Default)]
pub struct InMemorySessionStorePort {
    sessions: Mutex<HashMap<String, SharedSessionState>>,
}

impl InMemorySessionStorePort {
    pub fn new() -> Self {
        Self::default()
    }

    /// 만들어 둔 상태를 넣고, 테스트가 계속 들여다볼 수 있게 핸들을 돌려준다.
    pub fn insert(&self, state: SessionState) -> SharedSessionState {
        let key = state.session_id().value().to_string();
        let handle = Arc::new(Mutex::new(state));
        lock(&self.sessions).insert(key, handle.clone());
        handle
    }

    pub fn get(&self, session_id: &str) -> Option<SharedSessionState> {
        lock(&self.sessions).get(session_id).cloned()
    }
}

impl SessionStorePort for InMemorySessionStorePort {
    fn get_or_create(&self, session_id: &SessionId) -> AppResult<SharedSessionState> {
        Ok(lock(&self.sessions)
            .entry(session_id.value().to_string())
            .or_insert_with(|| Arc::new(Mutex::new(SessionState::new(session_id.clone()))))
            .clone())
    }
}

#[derive(Debug, Default)]
pub struct InMemoryRulebookPort {
    rulebooks: Mutex<Vec<Rulebook>>,
    save_calls: Mutex<usize>,
    deleted_ids: Mutex<Vec<String>>,
}

impl InMemoryRulebookPort {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_rulebooks(rulebooks: Vec<Rulebook>) -> Self {
        let port = Self::default();
        *lock(&port.rulebooks) = rulebooks;
        port
    }

    pub fn rulebooks(&self) -> Vec<Rulebook> {
        lock(&self.rulebooks).clone()
    }

    pub fn save_calls(&self) -> usize {
        *lock(&self.save_calls)
    }

    pub fn deleted_ids(&self) -> Vec<String> {
        lock(&self.deleted_ids).clone()
    }
}

impl RulebookPort for InMemoryRulebookPort {
    fn find_all(&self) -> AppResult<Vec<Rulebook>> {
        Ok(self.rulebooks())
    }

    fn find_by_id(&self, id: &RulebookId) -> AppResult<Option<Rulebook>> {
        Ok(lock(&self.rulebooks).iter().find(|r| r.id() == id).cloned())
    }

    fn save(&self, rulebook: Rulebook) -> AppResult<Rulebook> {
        *lock(&self.save_calls) += 1;
        upsert(&mut lock(&self.rulebooks), rulebook.clone(), |r| {
            r.id().clone()
        });
        Ok(rulebook)
    }

    fn delete_by_id(&self, id: &RulebookId) -> AppResult<()> {
        lock(&self.deleted_ids).push(id.value().to_string());
        lock(&self.rulebooks).retain(|r| r.id() != id);
        Ok(())
    }

    fn delete_all(&self) -> AppResult<()> {
        lock(&self.rulebooks).clear();
        Ok(())
    }
}

/// 파일을 메모리에 둔다. 경로는 `<기준 디렉터리>/<파일 이름>`이다.
#[derive(Debug)]
pub struct InMemoryRulebookFileStore {
    base_dir: String,
    files: Mutex<Vec<(String, Vec<u8>)>>,
    read_text_calls: Mutex<Vec<String>>,
    clear_calls: Mutex<usize>,
}

impl Default for InMemoryRulebookFileStore {
    fn default() -> Self {
        Self::with_base_dir("/tmp")
    }
}

impl InMemoryRulebookFileStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_base_dir(base_dir: &str) -> Self {
        Self {
            base_dir: base_dir.trim_end_matches('/').to_string(),
            files: Mutex::new(Vec::new()),
            read_text_calls: Mutex::new(Vec::new()),
            clear_calls: Mutex::new(0),
        }
    }

    /// 저장된 `(경로, 내용)`.
    pub fn files(&self) -> Vec<(String, Vec<u8>)> {
        lock(&self.files).clone()
    }

    /// 경로 없이 직접 넣는다(가져오기 테스트의 사전 상태).
    pub fn put(&self, path: &str, bytes: &[u8]) {
        upsert(
            &mut lock(&self.files),
            (path.to_string(), bytes.to_vec()),
            |f| f.0.clone(),
        );
    }

    pub fn read_text_calls(&self) -> Vec<String> {
        lock(&self.read_text_calls).clone()
    }

    pub fn clear_calls(&self) -> usize {
        *lock(&self.clear_calls)
    }

    fn find(&self, path: &str) -> AppResult<Vec<u8>> {
        lock(&self.files)
            .iter()
            .find(|(p, _)| p == path)
            .map(|(_, bytes)| bytes.clone())
            .ok_or_else(|| AppError::Internal(format!("File not found: {path}")))
    }
}

impl RulebookFileStore for InMemoryRulebookFileStore {
    fn store(&self, filename: &str, bytes: &[u8]) -> AppResult<StoredRulebookFile> {
        let path = format!("{}/{}", self.base_dir, filename);
        self.put(&path, bytes);
        Ok(StoredRulebookFile { path })
    }

    fn read_text(&self, path: &str) -> AppResult<String> {
        lock(&self.read_text_calls).push(path.to_string());
        String::from_utf8(self.find(path)?)
            .map_err(|e| AppError::Internal(format!("File is not valid UTF-8: {e}")))
    }

    fn read_bytes(&self, path: &str) -> AppResult<Vec<u8>> {
        self.find(path)
    }

    fn clear_all(&self) -> AppResult<()> {
        *lock(&self.clear_calls) += 1;
        lock(&self.files).clear();
        Ok(())
    }
}

/// `search` 호출 하나.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchCall {
    pub query: String,
    pub top_k: usize,
    pub rulebook_ids: HashSet<RulebookId>,
}

type SearchStub = (usize, HashSet<RulebookId>, Vec<RulebookContext>);

/// 색인한 조각을 기록하고, `search`는 미리 정해 둔 응답을 돌려준다
/// (`top_k`와 룰북 id 집합이 모두 같은 응답. 없으면 빈 목록).
#[derive(Debug, Default)]
pub struct InMemoryRulebookIndexPort {
    indexed: Mutex<Vec<(RulebookId, String, Vec<String>)>>,
    stubs: Mutex<Vec<SearchStub>>,
    search_error: Mutex<Option<AppError>>,
    search_calls: Mutex<Vec<SearchCall>>,
    reset_calls: Mutex<usize>,
}

impl InMemoryRulebookIndexPort {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn stub_search(
        &self,
        top_k: usize,
        rulebook_ids: HashSet<RulebookId>,
        contexts: Vec<RulebookContext>,
    ) {
        lock(&self.stubs).push((top_k, rulebook_ids, contexts));
    }

    /// 이후 모든 `search`가 이 오류로 실패한다.
    pub fn fail_search_with(&self, error: AppError) {
        *lock(&self.search_error) = Some(error);
    }

    /// 색인된 `(룰북 id, 파일 이름, 조각들)`.
    pub fn indexed(&self) -> Vec<(RulebookId, String, Vec<String>)> {
        lock(&self.indexed).clone()
    }

    pub fn search_calls(&self) -> Vec<SearchCall> {
        lock(&self.search_calls).clone()
    }

    pub fn reset_calls(&self) -> usize {
        *lock(&self.reset_calls)
    }
}

impl RulebookIndexPort for InMemoryRulebookIndexPort {
    fn index_rulebook_chunks(
        &self,
        rulebook_id: &RulebookId,
        filename: &str,
        chunks: &[String],
    ) -> AppResult<()> {
        lock(&self.indexed).push((rulebook_id.clone(), filename.to_string(), chunks.to_vec()));
        Ok(())
    }

    fn search(
        &self,
        query: &str,
        top_k: usize,
        enabled_rulebook_ids: &HashSet<RulebookId>,
    ) -> AppResult<Vec<RulebookContext>> {
        lock(&self.search_calls).push(SearchCall {
            query: query.to_string(),
            top_k,
            rulebook_ids: enabled_rulebook_ids.clone(),
        });
        if let Some(error) = lock(&self.search_error).clone() {
            return Err(error);
        }
        Ok(lock(&self.stubs)
            .iter()
            .find(|(k, ids, _)| *k == top_k && ids == enabled_rulebook_ids)
            .map(|(_, _, contexts)| contexts.clone())
            .unwrap_or_default())
    }

    fn reset(&self) -> AppResult<()> {
        *lock(&self.reset_calls) += 1;
        lock(&self.indexed).clear();
        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct InMemoryWrongNotePort {
    notes: Mutex<Vec<WrongNote>>,
}

impl InMemoryWrongNotePort {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_notes(notes: Vec<WrongNote>) -> Self {
        let port = Self::default();
        *lock(&port.notes) = notes;
        port
    }

    pub fn notes(&self) -> Vec<WrongNote> {
        lock(&self.notes).clone()
    }

    pub fn note_by_pattern(&self, pattern: &str) -> Option<WrongNote> {
        lock(&self.notes)
            .iter()
            .find(|n| n.pattern() == pattern)
            .cloned()
    }
}

impl WrongNotePort for InMemoryWrongNotePort {
    fn find_all(&self) -> AppResult<Vec<WrongNote>> {
        Ok(self.notes())
    }

    fn find_by_pattern(&self, pattern: &str) -> AppResult<Option<WrongNote>> {
        Ok(self.note_by_pattern(pattern))
    }

    fn save(&self, note: WrongNote) -> AppResult<WrongNote> {
        upsert(&mut lock(&self.notes), note.clone(), |n| n.id().clone());
        Ok(note)
    }

    fn delete_by_id(&self, id: &WrongNoteId) -> AppResult<()> {
        lock(&self.notes).retain(|n| n.id() != id);
        Ok(())
    }

    fn delete_all(&self) -> AppResult<()> {
        lock(&self.notes).clear();
        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct InMemoryWrongNoteRecentQueuePort {
    queue: Mutex<Vec<String>>,
    save_calls: Mutex<usize>,
}

impl InMemoryWrongNoteRecentQueuePort {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_queue(queue: Vec<String>) -> Self {
        let port = Self::default();
        *lock(&port.queue) = queue;
        port
    }

    pub fn queue(&self) -> Vec<String> {
        lock(&self.queue).clone()
    }

    pub fn save_calls(&self) -> usize {
        *lock(&self.save_calls)
    }
}

impl WrongNoteRecentQueuePort for InMemoryWrongNoteRecentQueuePort {
    fn load_global_queue(&self) -> AppResult<Vec<String>> {
        Ok(self.queue())
    }

    fn save_global_queue(&self, patterns: &[String]) -> AppResult<()> {
        *lock(&self.save_calls) += 1;
        *lock(&self.queue) = patterns.to_vec();
        Ok(())
    }

    fn clear_global_queue(&self) -> AppResult<()> {
        lock(&self.queue).clear();
        Ok(())
    }
}
