use std::collections::BTreeMap;
use std::io::{Cursor, Read, Write};
use std::sync::Arc;

use chrono::{DateTime, TimeZone, Utc};
use omypic_domain::error::AppError;
use omypic_domain::question::{
    QuestionGroupAggregate, QuestionGroupId, QuestionItem, QuestionItemId,
};
use omypic_domain::rulebook::{Rulebook, RulebookId, RulebookScope};
use omypic_domain::wrongnote::{WrongNote, WrongNoteId};
use serde_json::{json, Value};
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

use super::usecase::{read_zip_entries, DataTransferUseCase, ZipLimits};
use crate::rulebook::port::{RulebookFileStore, RulebookPort};
use crate::test_support::{
    InMemoryQuestionGroupPort, InMemoryRulebookFileStore, InMemoryRulebookIndexPort,
    InMemoryRulebookPort, InMemoryWrongNotePort, InMemoryWrongNoteRecentQueuePort,
};

fn fixed_time() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 2, 19, 0, 0, 0).unwrap()
}

struct Fixture {
    groups: Arc<InMemoryQuestionGroupPort>,
    rulebooks: Arc<InMemoryRulebookPort>,
    notes: Arc<InMemoryWrongNotePort>,
    queue: Arc<InMemoryWrongNoteRecentQueuePort>,
    files: Arc<InMemoryRulebookFileStore>,
    index: Arc<InMemoryRulebookIndexPort>,
    use_case: DataTransferUseCase,
}

fn fixture() -> Fixture {
    fixture_with(
        InMemoryQuestionGroupPort::new(),
        InMemoryRulebookPort::new(),
        InMemoryWrongNotePort::new(),
        InMemoryWrongNoteRecentQueuePort::new(),
    )
}

fn fixture_with(
    groups: InMemoryQuestionGroupPort,
    rulebooks: InMemoryRulebookPort,
    notes: InMemoryWrongNotePort,
    queue: InMemoryWrongNoteRecentQueuePort,
) -> Fixture {
    let (groups, rulebooks, notes, queue) = (
        Arc::new(groups),
        Arc::new(rulebooks),
        Arc::new(notes),
        Arc::new(queue),
    );
    let files = Arc::new(InMemoryRulebookFileStore::new());
    let index = Arc::new(InMemoryRulebookIndexPort::new());
    let use_case = DataTransferUseCase::new(
        groups.clone(),
        rulebooks.clone(),
        notes.clone(),
        queue.clone(),
        files.clone(),
        index.clone(),
    );
    Fixture {
        groups,
        rulebooks,
        notes,
        queue,
        files,
        index,
        use_case,
    }
}

fn to_zip(entries: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        zip.start_file(*name, SimpleFileOptions::default()).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

fn unzip_entries(zipped: &[u8]) -> BTreeMap<String, Vec<u8>> {
    let mut archive = ZipArchive::new(Cursor::new(zipped)).unwrap();
    let mut entries = BTreeMap::new();
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).unwrap();
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).unwrap();
        entries.insert(file.name().to_string(), bytes);
    }
    entries
}

/// 백업 zip을 만든다. `time`은 `createdAt` 등에 쓰는 JSON 값(ISO 문자열 또는 숫자).
fn backup_entries(schema_version: i64, time: Value) -> Vec<(&'static str, Vec<u8>)> {
    let bytes = |value: Value| serde_json::to_vec(&value).unwrap();
    vec![
        (
            "manifest.json",
            bytes(json!({"schemaVersion": schema_version, "app": "omypic", "exportedAt": time})),
        ),
        (
            "data/question-groups.json",
            bytes(json!([{
                "id": "g1", "name": "Travel", "tags": ["travel"],
                "questions": [{"id": "q1", "text": "Q1", "questionType": "habit"}],
                "createdAt": time, "updatedAt": time
            }])),
        ),
        (
            "data/rulebooks.json",
            bytes(json!([{
                "id": "r1", "filename": "rules.md", "scope": "MAIN", "questionGroup": null,
                "enabled": true, "createdAt": time, "updatedAt": time,
                "fileEntry": "files/rulebooks/r1_rules.md"
            }])),
        ),
        (
            "data/wrong-notes.json",
            bytes(json!([{
                "id": "w1", "pattern": "pattern", "count": 1,
                "shortSummary": "summary", "lastSeenAt": time
            }])),
        ),
        (
            "data/wrong-note-queue.json",
            bytes(json!({"patterns": ["pattern"]})),
        ),
        ("files/rulebooks/r1_rules.md", b"# rules".to_vec()),
    ]
}

fn replace_entry(entries: &mut [(&'static str, Vec<u8>)], name: &str, value: Value) {
    let entry = entries.iter_mut().find(|(n, _)| *n == name).unwrap();
    entry.1 = serde_json::to_vec(&value).unwrap();
}

fn iso() -> Value {
    json!("2026-02-19T00:00:00Z")
}

#[test]
fn export_zip_includes_required_entries() {
    let group = QuestionGroupAggregate::rehydrate(
        QuestionGroupId::of("g1").unwrap(),
        "Travel",
        ["travel"],
        vec![QuestionItem::rehydrate(
            QuestionItemId::of("q1").unwrap(),
            "Q1",
            Some("habit"),
        )],
        Some(fixed_time()),
        Some(fixed_time()),
    )
    .unwrap();
    let rulebook = Rulebook::rehydrate(
        RulebookId::of("r1").unwrap(),
        "rules.md",
        "/tmp/rules.md",
        Some(RulebookScope::Main),
        None,
        true,
        Some(fixed_time()),
        Some(fixed_time()),
    )
    .unwrap();
    let note = WrongNote::rehydrate(
        WrongNoteId::of("w1").unwrap(),
        "pattern",
        2,
        "summary",
        Some(fixed_time()),
    );
    let f = fixture_with(
        InMemoryQuestionGroupPort::with_groups(vec![group]),
        InMemoryRulebookPort::with_rulebooks(vec![rulebook]),
        InMemoryWrongNotePort::with_notes(vec![note]),
        InMemoryWrongNoteRecentQueuePort::with_queue(vec!["pattern".to_string()]),
    );
    f.files.put("/tmp/rules.md", b"# rules");

    let exported = f.use_case.export_zip().unwrap();
    let entries = unzip_entries(&exported);

    for name in [
        "manifest.json",
        "data/question-groups.json",
        "data/rulebooks.json",
        "data/wrong-notes.json",
        "data/wrong-note-queue.json",
        "files/rulebooks/r1_rules.md",
    ] {
        assert!(entries.contains_key(name), "missing {name}");
    }
    assert_eq!(b"# rules".to_vec(), entries["files/rulebooks/r1_rules.md"]);

    let manifest: Value = serde_json::from_slice(&entries["manifest.json"]).unwrap();
    assert_eq!(json!(1), manifest["schemaVersion"]);
    assert_eq!(json!("omypic"), manifest["app"]);
    assert!(manifest["exportedAt"].as_str().unwrap().ends_with('Z'));
    let groups: Value = serde_json::from_slice(&entries["data/question-groups.json"]).unwrap();
    assert_eq!(
        json!([{
            "id": "g1", "name": "Travel", "tags": ["travel"],
            "questions": [{"id": "q1", "text": "Q1", "questionType": "habit"}],
            "createdAt": "2026-02-19T00:00:00Z", "updatedAt": "2026-02-19T00:00:00Z"
        }]),
        groups
    );
    let rulebooks: Value = serde_json::from_slice(&entries["data/rulebooks.json"]).unwrap();
    assert_eq!(
        json!([{
            "id": "r1", "filename": "rules.md", "scope": "MAIN", "questionGroup": null,
            "enabled": true, "createdAt": "2026-02-19T00:00:00Z",
            "updatedAt": "2026-02-19T00:00:00Z", "fileEntry": "files/rulebooks/r1_rules.md"
        }]),
        rulebooks
    );
    let notes: Value = serde_json::from_slice(&entries["data/wrong-notes.json"]).unwrap();
    assert_eq!(
        json!([{
            "id": "w1", "pattern": "pattern", "count": 2,
            "shortSummary": "summary", "lastSeenAt": "2026-02-19T00:00:00Z"
        }]),
        notes
    );
    let queue: Value = serde_json::from_slice(&entries["data/wrong-note-queue.json"]).unwrap();
    assert_eq!(json!({"patterns": ["pattern"]}), queue);
}

#[test]
fn import_zip_replaces_all_data_and_reindexes() {
    let f = fixture();
    let zip = to_zip(&backup_entries(1, iso()));

    let result = f.use_case.import_zip(&zip).unwrap();

    assert_eq!(1, result.question_group_count);
    assert_eq!(1, result.rulebook_count);
    assert_eq!(1, result.wrong_note_count);
    assert_eq!(1, result.wrong_note_queue_size);
    assert!(result.restart_required);

    // 지우기가 저장보다 먼저 실행된다.
    assert_eq!(vec!["delete_all", "save"], f.groups.call_log());
    assert_eq!(vec!["delete_all", "save"], f.rulebooks.call_log());
    assert_eq!(vec!["delete_all", "save"], f.notes.call_log());
    assert_eq!(
        vec!["clear_global_queue", "save_global_queue"],
        f.queue.call_log()
    );
    assert_eq!(1, f.files.clear_calls());
    assert_eq!(1, f.index.reset_calls());

    assert_eq!(1, f.groups.groups().len());
    assert_eq!(1, f.rulebooks.rulebooks().len());
    assert_eq!(1, f.notes.notes().len());
    assert_eq!(vec!["pattern".to_string()], f.queue.queue());
    assert_eq!(
        vec![("/tmp/rules.md".to_string(), b"# rules".to_vec())],
        f.files.files()
    );
    let rulebook = &f.rulebooks.rulebooks()[0];
    assert_eq!("r1", rulebook.id().value());
    assert_eq!("/tmp/rules.md", rulebook.path());
    assert_eq!(
        vec![(
            RulebookId::of("r1").unwrap(),
            "rules.md".to_string(),
            vec!["# rules".to_string()]
        )],
        f.index.indexed()
    );
}

#[test]
fn import_zip_rejects_unsupported_schema_version() {
    let f = fixture();
    let zip = to_zip(&backup_entries(2, iso()));

    let result = f.use_case.import_zip(&zip);

    assert_eq!(
        Err(AppError::InvalidArgument(
            "Unsupported backup schema version: 2".to_string()
        )),
        result
    );
    assert_eq!(0, f.groups.delete_all_calls());
}

#[test]
fn import_accepts_iso_timestamps() {
    let f = fixture();
    let zip = to_zip(&backup_entries(1, iso()));

    f.use_case.import_zip(&zip).unwrap();

    assert_eq!(fixed_time(), f.groups.groups()[0].created_at());
    assert_eq!(fixed_time(), f.groups.groups()[0].updated_at());
    assert_eq!(fixed_time(), f.rulebooks.rulebooks()[0].created_at());
    assert_eq!(fixed_time(), f.notes.notes()[0].last_seen_at());
}

#[test]
fn import_accepts_epoch_second_timestamps() {
    let f = fixture();
    let zip_bytes = to_zip(&backup_entries(1, json!(1771459200.000000000_f64)));
    // Java가 쓰던 숫자 형식 그대로(소수점 아홉 자리)인 문서도 받는다.
    let mut entries = backup_entries(1, json!(0));
    entries[1].1 = br#"[{"id":"g1","name":"Travel","tags":["travel"],"questions":[],
        "createdAt":1771459200.000000000,"updatedAt":1771459200}]"#
        .to_vec();
    let literal_zip = to_zip(&entries);

    f.use_case.import_zip(&zip_bytes).unwrap();
    assert_eq!(fixed_time(), f.groups.groups()[0].created_at());
    assert_eq!(fixed_time(), f.rulebooks.rulebooks()[0].updated_at());
    assert_eq!(fixed_time(), f.notes.notes()[0].last_seen_at());

    f.use_case.import_zip(&literal_zip).unwrap();
    assert_eq!(fixed_time(), f.groups.groups()[0].created_at());
    assert_eq!(fixed_time(), f.groups.groups()[0].updated_at());
}

#[test]
fn import_accepts_fractional_epoch_seconds() {
    let f = fixture();
    let zip = to_zip(&backup_entries(1, json!(1771459200.5)));

    f.use_case.import_zip(&zip).unwrap();

    assert_eq!(
        fixed_time() + chrono::Duration::milliseconds(500),
        f.groups.groups()[0].created_at()
    );
}

#[test]
fn import_ignores_unknown_fields() {
    let f = fixture();
    let mut entries = backup_entries(1, iso());
    replace_entry(
        &mut entries,
        "manifest.json",
        json!({"schemaVersion": 1, "app": "omypic", "exportedAt": iso(), "extra": 1}),
    );
    replace_entry(
        &mut entries,
        "data/question-groups.json",
        json!([{"id": "g1", "name": "Travel", "tags": ["travel"], "extra": 1,
            "questions": [{"id": "q1", "text": "Q1", "questionType": "habit", "extra": 1}],
            "createdAt": iso(), "updatedAt": iso()}]),
    );
    replace_entry(
        &mut entries,
        "data/rulebooks.json",
        json!([{"id": "r1", "filename": "rules.md", "scope": "MAIN", "enabled": true,
            "createdAt": iso(), "updatedAt": iso(), "extra": 1,
            "fileEntry": "files/rulebooks/r1_rules.md"}]),
    );
    replace_entry(
        &mut entries,
        "data/wrong-notes.json",
        json!([{"id": "w1", "pattern": "pattern", "count": 1, "shortSummary": "summary",
            "lastSeenAt": iso(), "extra": 1}]),
    );
    replace_entry(
        &mut entries,
        "data/wrong-note-queue.json",
        json!({"patterns": ["pattern"], "extra": 1}),
    );

    let result = f.use_case.import_zip(&to_zip(&entries)).unwrap();

    assert_eq!(1, result.question_group_count);
    assert_eq!(1, result.rulebook_count);
    assert_eq!(1, result.wrong_note_count);
    assert_eq!(1, result.wrong_note_queue_size);
}

#[test]
fn import_fills_missing_optional_fields() {
    let f = fixture();
    let mut entries = backup_entries(1, iso());
    replace_entry(
        &mut entries,
        "data/question-groups.json",
        json!([{"id": "g1", "name": "Travel", "createdAt": iso(), "updatedAt": iso()}]),
    );
    replace_entry(
        &mut entries,
        "data/rulebooks.json",
        json!([{"id": "r1", "filename": "rules.md", "scope": "MAIN", "enabled": true,
            "createdAt": iso(), "updatedAt": iso(),
            "fileEntry": "files/rulebooks/r1_rules.md"}]),
    );

    f.use_case.import_zip(&to_zip(&entries)).unwrap();

    let group = &f.groups.groups()[0];
    assert!(group.tags().is_empty());
    assert!(group.questions().is_empty());
    assert!(f.rulebooks.rulebooks()[0].question_group().is_none());
}

#[test]
fn export_then_import_round_trips_korean_rulebook_filename() {
    let source = fixture();
    source
        .files
        .put("/tmp/오픽 규칙.md", "# 규칙\n내용".as_bytes());
    let rulebook = Rulebook::rehydrate(
        RulebookId::of("r1").unwrap(),
        "오픽 규칙.md",
        "/tmp/오픽 규칙.md",
        Some(RulebookScope::Question),
        Some(omypic_domain::question::QuestionGroup::of("여행").unwrap()),
        false,
        Some(fixed_time()),
        Some(fixed_time()),
    )
    .unwrap();
    source.rulebooks.save(rulebook).unwrap();

    let exported = source.use_case.export_zip().unwrap();
    let entries = unzip_entries(&exported);
    assert!(entries.contains_key("files/rulebooks/r1______.md"));

    let target = fixture();
    target.use_case.import_zip(&exported).unwrap();

    let imported = &target.rulebooks.rulebooks()[0];
    assert_eq!("오픽 규칙.md", imported.filename());
    assert_eq!(RulebookScope::Question, imported.scope());
    assert_eq!("여행", imported.question_group().unwrap().value());
    assert!(!imported.is_enabled());
    assert_eq!(
        "# 규칙\n내용",
        target.files.read_text(imported.path()).unwrap()
    );
}

#[test]
fn import_rejects_entry_that_escapes_rulebook_directory() {
    let f = fixture();
    let mut entries = backup_entries(1, iso());
    entries.push(("files/rulebooks/../../evil.md", b"evil".to_vec()));

    let result = f.use_case.import_zip(&to_zip(&entries));

    assert_eq!(
        Err(AppError::InvalidArgument(
            "Backup entry name is unsafe: files/rulebooks/../../evil.md".to_string()
        )),
        result
    );
    assert_eq!(0, f.groups.delete_all_calls());
    assert!(f.files.files().is_empty());
}

#[test]
fn import_rejects_rulebook_file_entry_outside_allowed_path() {
    for entry in [
        "files/rulebooks/../../evil.md",
        "/etc/passwd",
        "data/question-groups.json",
    ] {
        let f = fixture();
        let mut entries = backup_entries(1, iso());
        replace_entry(
            &mut entries,
            "data/rulebooks.json",
            json!([{"id": "r1", "filename": "rules.md", "scope": "MAIN", "enabled": true,
                "fileEntry": entry}]),
        );

        let result = f.use_case.import_zip(&to_zip(&entries));

        assert!(
            matches!(result, Err(AppError::InvalidArgument(_))),
            "{entry}: {result:?}"
        );
        assert!(f.files.files().is_empty());
        assert_eq!(0, f.rulebooks.delete_all_calls());
    }
}

#[test]
fn import_rejects_empty_file_and_missing_entries() {
    let f = fixture();
    assert_eq!(
        Err(AppError::InvalidArgument(
            "Import file is empty".to_string()
        )),
        f.use_case.import_zip(&[])
    );

    let mut entries = backup_entries(1, iso());
    entries.retain(|(name, _)| *name != "data/wrong-notes.json");
    assert_eq!(
        Err(AppError::InvalidArgument(
            "Required entry is missing: data/wrong-notes.json".to_string()
        )),
        f.use_case.import_zip(&to_zip(&entries))
    );

    let mut entries = backup_entries(1, iso());
    entries.retain(|(name, _)| *name != "files/rulebooks/r1_rules.md");
    assert_eq!(
        Err(AppError::InvalidArgument(
            "Rulebook file is missing from backup: files/rulebooks/r1_rules.md".to_string()
        )),
        f.use_case.import_zip(&to_zip(&entries))
    );
}

#[test]
fn import_rejects_invalid_rulebook_before_deleting_existing_data() {
    let f = fixture();
    let mut entries = backup_entries(1, iso());
    replace_entry(
        &mut entries,
        "data/rulebooks.json",
        json!([{"id": "r1", "filename": "rules.md", "scope": "QUESTION", "enabled": true,
            "fileEntry": "files/rulebooks/r1_rules.md"}]),
    );

    let result = f.use_case.import_zip(&to_zip(&entries));

    assert_eq!(
        Err(AppError::InvalidArgument(
            "QUESTION scope requires questionGroup".to_string()
        )),
        result
    );
    assert_eq!(0, f.groups.delete_all_calls());
}

#[test]
fn export_then_import_round_trips_filename_with_double_dots() {
    let source = fixture();
    source.files.put("/tmp/my..rules.md", b"# dots");
    let rulebook = Rulebook::rehydrate(
        RulebookId::of("r1").unwrap(),
        "my..rules.md",
        "/tmp/my..rules.md",
        Some(RulebookScope::Main),
        None,
        true,
        Some(fixed_time()),
        Some(fixed_time()),
    )
    .unwrap();
    source.rulebooks.save(rulebook).unwrap();

    let exported = source.use_case.export_zip().unwrap();
    assert!(unzip_entries(&exported).contains_key("files/rulebooks/r1_my..rules.md"));

    let target = fixture();
    target.use_case.import_zip(&exported).unwrap();

    let imported = &target.rulebooks.rulebooks()[0];
    assert_eq!("my..rules.md", imported.filename());
    assert_eq!("# dots", target.files.read_text(imported.path()).unwrap());
}

#[test]
fn import_accepts_java_backup_entry_with_double_dots_in_name() {
    let f = fixture();
    let mut entries = backup_entries(1, iso());
    replace_entry(
        &mut entries,
        "data/rulebooks.json",
        json!([{"id": "r1", "filename": "v1..final.md", "scope": "MAIN", "enabled": true,
            "createdAt": iso(), "updatedAt": iso(),
            "fileEntry": "files/rulebooks/r1_v1..final.md"}]),
    );
    entries.retain(|(name, _)| *name != "files/rulebooks/r1_rules.md");
    entries.push(("files/rulebooks/r1_v1..final.md", b"# final".to_vec()));

    let result = f.use_case.import_zip(&to_zip(&entries)).unwrap();

    assert_eq!(1, result.rulebook_count);
    assert_eq!("v1..final.md", f.rulebooks.rulebooks()[0].filename());
}

#[test]
fn import_rejects_parent_segments_and_absolute_entry_names() {
    for name in [
        "files/rulebooks/../../evil.md",
        "../evil.md",
        "files/rulebooks/..",
        "files\\rulebooks\\..\\evil.md",
        "/etc/passwd",
        "\\evil.md",
        "C:/evil.md",
    ] {
        let f = fixture();
        let mut entries = backup_entries(1, iso());
        entries.push((name, b"evil".to_vec()));

        let result = f.use_case.import_zip(&to_zip(&entries));

        assert_eq!(
            Err(AppError::InvalidArgument(format!(
                "Backup entry name is unsafe: {name}"
            ))),
            result,
            "{name}"
        );
    }
}

fn deflated_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        zip.start_file(*name, options).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

fn small_limits() -> ZipLimits {
    ZipLimits {
        max_entries: 8,
        max_entry_bytes: 1_000,
        max_total_bytes: 2_500,
    }
}

#[test]
fn zip_reader_rejects_entry_larger_than_entry_limit() {
    let zip = deflated_zip(&[("a.bin", &[0u8; 1_001])]);

    let result = read_zip_entries(&zip, &small_limits());

    assert_eq!(
        Err(AppError::InvalidArgument(
            "Backup entry is too large when decompressed: a.bin (limit 1000 bytes)".to_string()
        )),
        result.map(|_| ())
    );
}

#[test]
fn zip_reader_rejects_total_larger_than_total_limit() {
    let block = [0u8; 900];
    let zip = deflated_zip(&[("a.bin", &block), ("b.bin", &block), ("c.bin", &block)]);

    let result = read_zip_entries(&zip, &small_limits());

    assert_eq!(
        Err(AppError::InvalidArgument(
            "Backup is too large when decompressed (limit 2500 bytes)".to_string()
        )),
        result.map(|_| ())
    );
}

#[test]
fn zip_reader_rejects_too_many_entries() {
    let names: Vec<String> = (0..9).map(|n| format!("e{n}.txt")).collect();
    let entries: Vec<(&str, &[u8])> = names.iter().map(|n| (n.as_str(), &b"x"[..])).collect();
    let zip = deflated_zip(&entries);

    let result = read_zip_entries(&zip, &small_limits());

    assert_eq!(
        Err(AppError::InvalidArgument(
            "Backup has too many entries (limit 8)".to_string()
        )),
        result.map(|_| ())
    );
}

#[test]
fn zip_reader_accepts_entries_within_limits() {
    let block = [7u8; 1_000];
    let zip = deflated_zip(&[("a.bin", &block), ("b.bin", &block)]);

    let entries = read_zip_entries(&zip, &small_limits()).unwrap();

    assert_eq!(block.to_vec(), entries["a.bin"]);
    assert_eq!(block.to_vec(), entries["b.bin"]);
}

#[test]
fn import_limits_are_512_mb_total_64_mb_per_entry_and_10000_entries() {
    assert_eq!(
        ZipLimits {
            max_entries: 10_000,
            max_entry_bytes: 64 * 1024 * 1024,
            max_total_bytes: 512 * 1024 * 1024,
        },
        ZipLimits::IMPORT
    );
}

#[test]
fn import_rejects_oversized_entry_before_touching_data() {
    let f = fixture();
    let mut entries = backup_entries(1, iso());
    entries.push(("files/rulebooks/huge.md", vec![b'a'; 64 * 1024 * 1024 + 1]));
    let entries: Vec<(&str, &[u8])> = entries.iter().map(|(n, b)| (*n, b.as_slice())).collect();
    let zip = deflated_zip(&entries);
    assert!(zip.len() < 1024 * 1024);

    let result = f.use_case.import_zip(&zip);

    assert_eq!(
        Err(AppError::InvalidArgument(
            "Backup entry is too large when decompressed: files/rulebooks/huge.md (limit 64 MB)"
                .to_string()
        )),
        result
    );
    assert_eq!(0, f.groups.delete_all_calls());
}
