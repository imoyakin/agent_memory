use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::path::Path;

use crate::config::load_runtime_config;
use crate::models::MemoryRecord;
use crate::storage::{
    delete_record_from_backend, get_record_from_backend, read_records_from_backend,
    upsert_record_to_backend,
};
use crate::util::now;
use crate::MAX_CONTENT_BYTES;

pub(crate) fn filtered_records(
    root: &Path,
    memory_type: Option<&str>,
    scope: Option<&str>,
    tags: &[String],
) -> Result<Vec<MemoryRecord>> {
    let required_tags: HashSet<_> = tags.iter().collect();
    Ok(read_records(root)?
        .into_iter()
        .filter(|record| {
            memory_type
                .map(|value| record.memory_type == value)
                .unwrap_or(true)
        })
        .filter(|record| scope.map(|value| record.scope == value).unwrap_or(true))
        .filter(|record| {
            required_tags.is_empty() || required_tags.iter().all(|tag| record.tags.contains(tag))
        })
        .collect())
}

pub(crate) fn read_records(root: &Path) -> Result<Vec<MemoryRecord>> {
    let config = load_runtime_config(root)?;
    read_records_from_backend(root, &config)
}

pub(crate) fn get_record(root: &Path, memory_id: &str) -> Result<MemoryRecord> {
    let config = load_runtime_config(root)?;
    get_record_from_backend(root, &config, memory_id)?
        .ok_or_else(|| anyhow!("memory_id not found: {}", memory_id))
}

pub(crate) fn delete_record(root: &Path, memory_id: &str) -> Result<bool> {
    let config = load_runtime_config(root)?;
    delete_record_from_backend(root, &config, memory_id)
}

pub(crate) fn upsert_record(
    root: &Path,
    record: &MemoryRecord,
    vector: Option<&[f32]>,
) -> Result<()> {
    let config = load_runtime_config(root)?;
    upsert_record_to_backend(root, &config, record, vector)
}

pub(crate) fn mark_accessed(root: &Path, ids: Vec<String>) -> Result<()> {
    if ids.is_empty() {
        return Ok(());
    }
    let target: HashSet<_> = ids.into_iter().collect();
    let mut records = read_records(root)?;
    let timestamp = now();
    for record in &mut records {
        if target.contains(&record.uuid) {
            record.access_count += 1;
            record.last_accessed_at = Some(timestamp.clone());
            if let Err(error) = upsert_record(root, record, None) {
                if is_lite_lock_error(&error) {
                    continue;
                }
                return Err(error);
            }
        }
    }
    Ok(())
}

fn is_lite_lock_error(error: &anyhow::Error) -> bool {
    let message = error.to_string();
    message.contains("DataDirLockedError") || message.contains("another process holds the lock")
}

pub(crate) fn record_value(record: &MemoryRecord) -> Value {
    let mut value = serde_json::to_value(record).unwrap();
    value["memory_id"] = json!(record.uuid);
    value
}

impl MemoryRecord {
    pub(crate) fn with_summary(mut self) -> Self {
        self.summary = summarize_content(&self.content);
        self
    }
}

pub(crate) fn summarize_content(content: &str) -> String {
    let normalized = content.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.chars().count() <= 360 {
        normalized
    } else {
        normalized.chars().take(357).collect::<String>() + "..."
    }
}

pub(crate) fn summarize(source_ref: &str, timestamp: &str, memory_type: &str) -> String {
    format!("{memory_type} memory from {source_ref} at {timestamp}")
}

pub(crate) fn embedding_text(record: &MemoryRecord) -> String {
    if record.keys.is_empty() {
        record.content.clone()
    } else {
        format!(
            "{}\n\nSearch keys: {}",
            record.content,
            record.keys.join(", ")
        )
    }
}

pub(crate) fn record_search_text(record: &MemoryRecord) -> String {
    format!(
        "{} {} {} {} {} {} {}",
        record.keys.join(" "),
        record.keys.join(" "),
        record.content,
        record.summary,
        record.source_ref,
        record.memory_type,
        record.tags.join(" ")
    )
    .to_lowercase()
}

pub(crate) fn derive_keys(content: &str, source_ref: &str, tags: &[String]) -> Vec<String> {
    let mut keys = Vec::new();
    let mut seen = HashSet::new();
    for item in std::iter::once(source_ref)
        .chain(tags.iter().map(String::as_str))
        .chain(content.split_whitespace())
    {
        let normalized = item.trim_matches(|ch: char| " ,.;:()[]{}<>`'\"".contains(ch));
        if normalized.len() < 3 || !seen.insert(normalized.to_string()) {
            continue;
        }
        keys.push(normalized.to_string());
        if keys.len() >= 12 {
            break;
        }
    }
    keys
}

pub(crate) fn validate_content(content: &str) -> Result<()> {
    let size = content.len();
    if size == 0 {
        bail!("content must not be empty");
    }
    if size > MAX_CONTENT_BYTES {
        bail!("content is {size} bytes; maximum is {MAX_CONTENT_BYTES} bytes");
    }
    Ok(())
}
