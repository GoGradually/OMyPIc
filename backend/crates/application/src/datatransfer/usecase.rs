use std::collections::HashMap;
use std::io::{Cursor, Read, Write};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use omypic_domain::error::{AppError, AppResult};
use omypic_domain::question::{
    QuestionGroup, QuestionGroupAggregate, QuestionGroupId, QuestionItem, QuestionItemId,
};
use omypic_domain::rulebook::{Rulebook, RulebookId, RulebookScope};
use omypic_domain::shared::text_utils::{java_is_blank, split_chunks};
use omypic_domain::wrongnote::{WrongNote, WrongNoteId};
use serde::de::DeserializeOwned;
use serde::Serialize;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use super::model::{
    BackupManifest, DataTransferImportResult, QuestionGroupSnapshot, QuestionItemSnapshot,
    RulebookSnapshot, WrongNoteQueueSnapshot, WrongNoteSnapshot,
};
use crate::question::port::QuestionGroupPort;
use crate::rulebook::port::{RulebookFileStore, RulebookIndexPort, RulebookPort};
use crate::wrongnote::port::{WrongNotePort, WrongNoteRecentQueuePort};

const SCHEMA_VERSION: i64 = 1;
const MANIFEST_PATH: &str = "manifest.json";
const QUESTION_GROUPS_PATH: &str = "data/question-groups.json";
const RULEBOOKS_PATH: &str = "data/rulebooks.json";
const WRONG_NOTES_PATH: &str = "data/wrong-notes.json";
const WRONG_NOTE_QUEUE_PATH: &str = "data/wrong-note-queue.json";
const RULEBOOK_FILE_PREFIX: &str = "files/rulebooks/";
const CHUNK_SIZE: usize = 800;

pub struct DataTransferUseCase {
    question_group_port: Arc<dyn QuestionGroupPort>,
    rulebook_port: Arc<dyn RulebookPort>,
    wrong_note_port: Arc<dyn WrongNotePort>,
    wrong_note_recent_queue_port: Arc<dyn WrongNoteRecentQueuePort>,
    rulebook_file_store: Arc<dyn RulebookFileStore>,
    rulebook_index_port: Arc<dyn RulebookIndexPort>,
}

struct ImportedRulebook {
    id: RulebookId,
    filename: String,
    scope: Option<RulebookScope>,
    question_group: Option<QuestionGroup>,
    enabled: bool,
    created_at: Option<DateTime<Utc>>,
    updated_at: Option<DateTime<Utc>>,
    bytes: Vec<u8>,
}

struct ImportedPayload {
    question_groups: Vec<QuestionGroupAggregate>,
    rulebooks: Vec<ImportedRulebook>,
    wrong_notes: Vec<WrongNote>,
    wrong_note_queue: Vec<String>,
}

impl DataTransferUseCase {
    pub fn new(
        question_group_port: Arc<dyn QuestionGroupPort>,
        rulebook_port: Arc<dyn RulebookPort>,
        wrong_note_port: Arc<dyn WrongNotePort>,
        wrong_note_recent_queue_port: Arc<dyn WrongNoteRecentQueuePort>,
        rulebook_file_store: Arc<dyn RulebookFileStore>,
        rulebook_index_port: Arc<dyn RulebookIndexPort>,
    ) -> Self {
        Self {
            question_group_port,
            rulebook_port,
            wrong_note_port,
            wrong_note_recent_queue_port,
            rulebook_file_store,
            rulebook_index_port,
        }
    }

    pub fn export_zip(&self) -> AppResult<Vec<u8>> {
        let question_groups: Vec<QuestionGroupSnapshot> = self
            .question_group_port
            .find_all()?
            .iter()
            .map(question_group_snapshot)
            .collect();
        let mut rulebook_files: Vec<(String, Vec<u8>)> = Vec::new();
        let rulebooks = self.collect_rulebook_snapshots(&mut rulebook_files)?;
        let wrong_notes: Vec<WrongNoteSnapshot> = self
            .wrong_note_port
            .find_all()?
            .iter()
            .map(wrong_note_snapshot)
            .collect();
        let queue = WrongNoteQueueSnapshot {
            patterns: Some(self.wrong_note_recent_queue_port.load_global_queue()?),
        };
        let manifest = BackupManifest {
            schema_version: SCHEMA_VERSION,
            app: Some("omypic".to_string()),
            exported_at: Some(Utc::now()),
        };

        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        write_json(&mut zip, MANIFEST_PATH, &manifest)?;
        write_json(&mut zip, QUESTION_GROUPS_PATH, &question_groups)?;
        write_json(&mut zip, RULEBOOKS_PATH, &rulebooks)?;
        write_json(&mut zip, WRONG_NOTES_PATH, &wrong_notes)?;
        write_json(&mut zip, WRONG_NOTE_QUEUE_PATH, &queue)?;
        for (entry_path, bytes) in &rulebook_files {
            write_bytes(&mut zip, entry_path, bytes)?;
        }
        let cursor = zip.finish().map_err(io_error)?;
        Ok(cursor.into_inner())
    }

    pub fn import_zip(&self, zip_bytes: &[u8]) -> AppResult<DataTransferImportResult> {
        if zip_bytes.is_empty() {
            return Err(invalid("Import file is empty"));
        }
        let entries = read_zip_entries(zip_bytes)?;
        for path in [
            MANIFEST_PATH,
            QUESTION_GROUPS_PATH,
            RULEBOOKS_PATH,
            WRONG_NOTES_PATH,
            WRONG_NOTE_QUEUE_PATH,
        ] {
            if !entries.contains_key(path) {
                return Err(invalid(&format!("Required entry is missing: {path}")));
            }
        }

        let manifest: BackupManifest = read_json(&entries[MANIFEST_PATH])?;
        if manifest.schema_version != SCHEMA_VERSION {
            return Err(invalid(&format!(
                "Unsupported backup schema version: {}",
                manifest.schema_version
            )));
        }

        let question_groups: Option<Vec<QuestionGroupSnapshot>> =
            read_json(&entries[QUESTION_GROUPS_PATH])?;
        let rulebooks: Option<Vec<RulebookSnapshot>> = read_json(&entries[RULEBOOKS_PATH])?;
        let wrong_notes: Option<Vec<WrongNoteSnapshot>> = read_json(&entries[WRONG_NOTES_PATH])?;
        let queue: Option<WrongNoteQueueSnapshot> = read_json(&entries[WRONG_NOTE_QUEUE_PATH])?;
        let (Some(question_groups), Some(rulebooks), Some(wrong_notes), Some(queue)) =
            (question_groups, rulebooks, wrong_notes, queue)
        else {
            return Err(invalid("Backup payload is invalid"));
        };
        validate_rulebook_entries(&rulebooks, &entries)?;

        let payload =
            to_imported_payload(question_groups, rulebooks, wrong_notes, queue, &entries)?;
        self.apply_imported_payload(&payload)?;

        Ok(DataTransferImportResult {
            imported_at: Utc::now(),
            question_group_count: payload.question_groups.len(),
            rulebook_count: payload.rulebooks.len(),
            wrong_note_count: payload.wrong_notes.len(),
            wrong_note_queue_size: payload.wrong_note_queue.len(),
            restart_required: true,
        })
    }

    fn collect_rulebook_snapshots(
        &self,
        rulebook_files: &mut Vec<(String, Vec<u8>)>,
    ) -> AppResult<Vec<RulebookSnapshot>> {
        let mut snapshots = Vec::new();
        for rulebook in self.rulebook_port.find_all()? {
            let entry_path = to_rulebook_entry_path(rulebook.id().value(), rulebook.filename());
            let bytes = self.rulebook_file_store.read_bytes(rulebook.path())?;
            rulebook_files.push((entry_path.clone(), bytes));
            snapshots.push(rulebook_snapshot(&rulebook, entry_path));
        }
        Ok(snapshots)
    }

    fn apply_imported_payload(&self, payload: &ImportedPayload) -> AppResult<()> {
        self.question_group_port.delete_all()?;
        self.rulebook_port.delete_all()?;
        self.wrong_note_port.delete_all()?;
        self.wrong_note_recent_queue_port.clear_global_queue()?;
        self.rulebook_file_store.clear_all()?;
        self.rulebook_index_port.reset()?;

        for group in &payload.question_groups {
            self.question_group_port.save(group.clone())?;
        }
        for imported in &payload.rulebooks {
            let stored = self
                .rulebook_file_store
                .store(&imported.filename, &imported.bytes)?;
            let rulebook = Rulebook::rehydrate(
                imported.id.clone(),
                &imported.filename,
                &stored.path,
                imported.scope,
                imported.question_group.clone(),
                imported.enabled,
                imported.created_at,
                imported.updated_at,
            )?;
            self.rulebook_port.save(rulebook.clone())?;
            let text = self.rulebook_file_store.read_text(&stored.path)?;
            let chunks = split_chunks(Some(&text), CHUNK_SIZE);
            self.rulebook_index_port.index_rulebook_chunks(
                rulebook.id(),
                rulebook.filename(),
                &chunks,
            )?;
        }
        for note in &payload.wrong_notes {
            self.wrong_note_port.save(note.clone())?;
        }
        self.wrong_note_recent_queue_port
            .save_global_queue(&payload.wrong_note_queue)
    }
}

fn invalid(message: &str) -> AppError {
    AppError::InvalidArgument(message.to_string())
}

fn io_error(error: impl std::fmt::Display) -> AppError {
    AppError::Internal(error.to_string())
}

/// Java의 `[^a-zA-Z0-9._-]` -> `_` 치환. Java `char`(UTF-16 유닛) 단위로 치환한다.
fn to_rulebook_entry_path(rulebook_id: &str, filename: &str) -> String {
    let mut safe_name = String::new();
    for c in filename.chars() {
        if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
            safe_name.push(c);
        } else {
            safe_name.push_str(&"_".repeat(c.len_utf16()));
        }
    }
    if java_is_blank(&safe_name) {
        safe_name = "rulebook.md".to_string();
    }
    format!("{RULEBOOK_FILE_PREFIX}{rulebook_id}_{safe_name}")
}

fn write_json<T: Serialize + ?Sized>(
    zip: &mut ZipWriter<Cursor<Vec<u8>>>,
    path: &str,
    value: &T,
) -> AppResult<()> {
    let bytes = serde_json::to_vec_pretty(value).map_err(io_error)?;
    write_bytes(zip, path, &bytes)
}

fn write_bytes(zip: &mut ZipWriter<Cursor<Vec<u8>>>, path: &str, bytes: &[u8]) -> AppResult<()> {
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    zip.start_file(path, options).map_err(io_error)?;
    zip.write_all(bytes).map_err(io_error)
}

fn read_json<T: DeserializeOwned>(bytes: &[u8]) -> AppResult<T> {
    serde_json::from_slice(bytes).map_err(|e| invalid(&format!("Backup payload is invalid: {e}")))
}

fn read_zip_entries(zip_bytes: &[u8]) -> AppResult<HashMap<String, Vec<u8>>> {
    let mut archive = ZipArchive::new(Cursor::new(zip_bytes))
        .map_err(|e| invalid(&format!("Import file is not a valid zip: {e}")))?;
    let mut entries = HashMap::new();
    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|e| invalid(&format!("Import file is not a valid zip: {e}")))?;
        if file.is_dir() {
            continue;
        }
        let name = file.name().to_string();
        validate_entry_name(&name)?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).map_err(io_error)?;
        if entries.insert(name.clone(), bytes).is_some() {
            return Err(invalid(&format!("Duplicated entry in backup file: {name}")));
        }
    }
    Ok(entries)
}

fn validate_entry_name(name: &str) -> AppResult<()> {
    if java_is_blank(name) {
        return Err(invalid("Backup entry name is invalid"));
    }
    if is_unsafe_entry_name(name) {
        return Err(invalid(&format!("Backup entry name is unsafe: {name}")));
    }
    Ok(())
}

/// 절대 경로(`/`, `\`, 드라이브 문자)이거나 `..`인 경로 구성 요소가 있는 이름.
/// `my..rules.md`처럼 이름 안에 점이 둘 있는 것은 괜찮다.
fn is_unsafe_entry_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    let absolute = name.starts_with(['/', '\\'])
        || (bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':');
    absolute || name.split(['/', '\\']).any(|segment| segment == "..")
}

fn validate_rulebook_entries(
    rulebooks: &[RulebookSnapshot],
    entries: &HashMap<String, Vec<u8>>,
) -> AppResult<()> {
    for rulebook in rulebooks {
        let entry = match rulebook.file_entry.as_deref() {
            Some(entry) if !java_is_blank(entry) => entry,
            _ => return Err(invalid("Rulebook file entry is missing")),
        };
        validate_entry_name(entry)?;
        if !entry.starts_with(RULEBOOK_FILE_PREFIX) {
            return Err(invalid(&format!(
                "Rulebook file entry is outside allowed path: {entry}"
            )));
        }
        if !entries.contains_key(entry) {
            return Err(invalid(&format!(
                "Rulebook file is missing from backup: {entry}"
            )));
        }
    }
    Ok(())
}

fn parse_scope(scope: Option<&str>) -> AppResult<Option<RulebookScope>> {
    match scope {
        None => Ok(None),
        Some("MAIN") => Ok(Some(RulebookScope::Main)),
        Some("QUESTION") => Ok(Some(RulebookScope::Question)),
        Some(other) => Err(invalid(&format!("Unknown rulebook scope: {other}"))),
    }
}

fn scope_name(scope: RulebookScope) -> &'static str {
    match scope {
        RulebookScope::Main => "MAIN",
        RulebookScope::Question => "QUESTION",
    }
}

fn to_imported_payload(
    question_groups: Vec<QuestionGroupSnapshot>,
    rulebooks: Vec<RulebookSnapshot>,
    wrong_notes: Vec<WrongNoteSnapshot>,
    queue: WrongNoteQueueSnapshot,
    entries: &HashMap<String, Vec<u8>>,
) -> AppResult<ImportedPayload> {
    let question_groups = question_groups
        .into_iter()
        .map(question_group_from_snapshot)
        .collect::<AppResult<Vec<_>>>()?;
    let rulebooks = rulebooks
        .into_iter()
        .map(|snapshot| imported_rulebook(snapshot, entries))
        .collect::<AppResult<Vec<_>>>()?;
    let wrong_notes = wrong_notes
        .into_iter()
        .map(wrong_note_from_snapshot)
        .collect::<AppResult<Vec<_>>>()?;
    Ok(ImportedPayload {
        question_groups,
        rulebooks,
        wrong_notes,
        wrong_note_queue: queue.patterns.unwrap_or_default(),
    })
}

fn question_group_from_snapshot(
    snapshot: QuestionGroupSnapshot,
) -> AppResult<QuestionGroupAggregate> {
    let items = snapshot
        .questions
        .unwrap_or_default()
        .into_iter()
        .map(|item| {
            Ok(QuestionItem::rehydrate(
                QuestionItemId::of(item.id.as_deref().unwrap_or_default())?,
                item.text.as_deref().unwrap_or_default(),
                item.question_type.as_deref(),
            ))
        })
        .collect::<AppResult<Vec<_>>>()?;
    QuestionGroupAggregate::rehydrate(
        QuestionGroupId::of(snapshot.id.as_deref().unwrap_or_default())?,
        snapshot.name.as_deref().unwrap_or_default(),
        snapshot.tags.unwrap_or_default(),
        items,
        snapshot.created_at,
        snapshot.updated_at,
    )
}

fn imported_rulebook(
    snapshot: RulebookSnapshot,
    entries: &HashMap<String, Vec<u8>>,
) -> AppResult<ImportedRulebook> {
    let filename = snapshot
        .filename
        .ok_or_else(|| invalid("Rulebook filename is missing"))?;
    let imported = ImportedRulebook {
        id: RulebookId::of(snapshot.id.as_deref().unwrap_or_default())?,
        filename,
        scope: parse_scope(snapshot.scope.as_deref())?,
        question_group: QuestionGroup::from_nullable(snapshot.question_group.as_deref()),
        enabled: snapshot.enabled,
        created_at: snapshot.created_at,
        updated_at: snapshot.updated_at,
        bytes: snapshot
            .file_entry
            .as_deref()
            .and_then(|entry| entries.get(entry))
            .cloned()
            .unwrap_or_default(),
    };
    // 기존 데이터를 지우기 전에 범위와 질문 그룹 조합을 검증한다.
    Rulebook::rehydrate(
        imported.id.clone(),
        &imported.filename,
        "",
        imported.scope,
        imported.question_group.clone(),
        imported.enabled,
        imported.created_at,
        imported.updated_at,
    )?;
    Ok(imported)
}

fn wrong_note_from_snapshot(snapshot: WrongNoteSnapshot) -> AppResult<WrongNote> {
    Ok(WrongNote::rehydrate(
        WrongNoteId::of(snapshot.id.as_deref().unwrap_or_default())?,
        snapshot.pattern.as_deref().unwrap_or_default(),
        snapshot.count,
        snapshot.short_summary.as_deref().unwrap_or_default(),
        snapshot.last_seen_at,
    ))
}

fn question_group_snapshot(group: &QuestionGroupAggregate) -> QuestionGroupSnapshot {
    let mut tags = group.tags().to_vec();
    // Java `String` 순서(UTF-16 유닛)
    tags.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
    QuestionGroupSnapshot {
        id: Some(group.id().value().to_string()),
        name: Some(group.name().to_string()),
        tags: Some(tags),
        questions: Some(
            group
                .questions()
                .iter()
                .map(|item| QuestionItemSnapshot {
                    id: Some(item.id().value().to_string()),
                    text: Some(item.text().to_string()),
                    question_type: item.question_type().map(str::to_string),
                })
                .collect(),
        ),
        created_at: Some(group.created_at()),
        updated_at: Some(group.updated_at()),
    }
}

fn rulebook_snapshot(rulebook: &Rulebook, file_entry: String) -> RulebookSnapshot {
    RulebookSnapshot {
        id: Some(rulebook.id().value().to_string()),
        filename: Some(rulebook.filename().to_string()),
        scope: Some(scope_name(rulebook.scope()).to_string()),
        question_group: rulebook.question_group().map(|g| g.value().to_string()),
        enabled: rulebook.is_enabled(),
        created_at: Some(rulebook.created_at()),
        updated_at: Some(rulebook.updated_at()),
        file_entry: Some(file_entry),
    }
}

fn wrong_note_snapshot(note: &WrongNote) -> WrongNoteSnapshot {
    WrongNoteSnapshot {
        id: Some(note.id().value().to_string()),
        pattern: Some(note.pattern().to_string()),
        count: note.count(),
        short_summary: Some(note.short_summary().to_string()),
        last_seen_at: Some(note.last_seen_at()),
    }
}
