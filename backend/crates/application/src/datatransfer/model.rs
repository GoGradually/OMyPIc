//! 백업 zip 안의 JSON 스냅샷과 가져오기 결과.
//!
//! 필드 이름(camelCase)은 Java 0.2.x 백업과 같다. 읽을 때는 알 수 없는 필드를 무시하고,
//! 빠진 필드는 Java 필드 기본값(목록은 빈 목록, 숫자/불리언은 0/false, 문자열은 null)을 쓴다.
//! 시각은 ISO-8601 문자열과 초 단위 숫자(소수 가능)를 모두 받는다.

use chrono::{DateTime, SecondsFormat, TimeZone, Utc};
use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize, Serializer};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataTransferImportResult {
    pub imported_at: DateTime<Utc>,
    pub question_group_count: usize,
    pub rulebook_count: usize,
    pub wrong_note_count: usize,
    pub wrong_note_queue_size: usize,
    pub restart_required: bool,
}

fn serialize_instant<S: Serializer>(
    value: &Option<DateTime<Utc>>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match value {
        Some(instant) => {
            serializer.serialize_str(&instant.to_rfc3339_opts(SecondsFormat::AutoSi, true))
        }
        None => serializer.serialize_none(),
    }
}

fn deserialize_instant<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<DateTime<Utc>>, D::Error> {
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    match value {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::String(text)) => DateTime::parse_from_rfc3339(&text)
            .map(|parsed| Some(parsed.with_timezone(&Utc)))
            .map_err(|e| de::Error::custom(format!("invalid timestamp '{text}': {e}"))),
        Some(serde_json::Value::Number(number)) => {
            let seconds = number
                .as_f64()
                .ok_or_else(|| de::Error::custom("invalid timestamp number"))?;
            let total_nanos = (seconds * 1e9).round() as i128;
            let secs = total_nanos.div_euclid(1_000_000_000) as i64;
            let nanos = total_nanos.rem_euclid(1_000_000_000) as u32;
            Utc.timestamp_opt(secs, nanos)
                .single()
                .map(Some)
                .ok_or_else(|| de::Error::custom("timestamp out of range"))
        }
        Some(other) => Err(de::Error::custom(format!("invalid timestamp: {other}"))),
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(super) struct BackupManifest {
    pub schema_version: i64,
    pub app: Option<String>,
    #[serde(
        serialize_with = "serialize_instant",
        deserialize_with = "deserialize_instant"
    )]
    pub exported_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(super) struct WrongNoteQueueSnapshot {
    pub patterns: Option<Vec<String>>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(super) struct QuestionGroupSnapshot {
    pub id: Option<String>,
    pub name: Option<String>,
    pub tags: Option<Vec<String>>,
    pub questions: Option<Vec<QuestionItemSnapshot>>,
    #[serde(
        serialize_with = "serialize_instant",
        deserialize_with = "deserialize_instant"
    )]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(
        serialize_with = "serialize_instant",
        deserialize_with = "deserialize_instant"
    )]
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(super) struct QuestionItemSnapshot {
    pub id: Option<String>,
    pub text: Option<String>,
    pub question_type: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(super) struct RulebookSnapshot {
    pub id: Option<String>,
    pub filename: Option<String>,
    pub scope: Option<String>,
    pub question_group: Option<String>,
    pub enabled: bool,
    #[serde(
        serialize_with = "serialize_instant",
        deserialize_with = "deserialize_instant"
    )]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(
        serialize_with = "serialize_instant",
        deserialize_with = "deserialize_instant"
    )]
    pub updated_at: Option<DateTime<Utc>>,
    pub file_entry: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(super) struct WrongNoteSnapshot {
    pub id: Option<String>,
    pub pattern: Option<String>,
    pub count: i32,
    pub short_summary: Option<String>,
    #[serde(
        serialize_with = "serialize_instant",
        deserialize_with = "deserialize_instant"
    )]
    pub last_seen_at: Option<DateTime<Utc>>,
}
