mod actuator;
mod apikey;
mod datatransfer;
mod meta;
mod mode;
mod question;
mod rulebook;
mod voice;
mod wrongnote;

use axum::Router;
use chrono::{DateTime, SecondsFormat, Utc};

use crate::state::AppState;

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .merge(apikey::routes())
        .merge(datatransfer::routes())
        .merge(meta::routes())
        .merge(mode::routes())
        .merge(question::routes())
        .merge(rulebook::routes())
        .merge(voice::routes())
        .merge(wrongnote::routes())
        .merge(actuator::routes())
}

/// Java `Instant`의 JSON 표현(ISO-8601, 소수 자릿수는 필요한 만큼).
pub(crate) fn iso(instant: DateTime<Utc>) -> String {
    instant.to_rfc3339_opts(SecondsFormat::AutoSi, true)
}

/// JSON 목록의 `null` 원소는 빈 문자열로 바꾼다(도메인이 빈 태그를 버린다).
pub(crate) fn strings(values: Option<Vec<Option<String>>>) -> Vec<String> {
    values
        .unwrap_or_default()
        .into_iter()
        .map(Option::unwrap_or_default)
        .collect()
}
