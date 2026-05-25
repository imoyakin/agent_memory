use chrono::Utc;
use std::collections::HashSet;
use uuid::Uuid;

pub(crate) fn parse_csv(raw: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    raw.split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .filter(|item| seen.insert(item.to_string()))
        .map(ToString::to_string)
        .collect()
}

pub(crate) fn now() -> String {
    Utc::now().to_rfc3339()
}

pub(crate) fn safe_timestamp() -> String {
    now().replace(':', "").replace('+', "Z")
}

pub(crate) fn uuid_hex(value: &str) -> String {
    value.replace('-', "")
}

pub(crate) fn new_uuid_string() -> String {
    Uuid::new_v4().to_string()
}
