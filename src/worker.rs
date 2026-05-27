use anyhow::Result;
use serde_json::{json, Value};
use std::path::PathBuf;

use crate::config::load_runtime_config;
use crate::discovery::runtime_root;
use crate::embedding::embed;
use crate::records::embedding_text;
use crate::storage::{ensure_backend, pending_records_from_backend, upsert_vector};
use crate::util::now;

pub(crate) fn cmd_worker(
    root_arg: Option<PathBuf>,
    limit: Option<usize>,
    retry_failed: bool,
) -> Result<Value> {
    let root = runtime_root(root_arg)?;
    let config = load_runtime_config(&root)?;
    ensure_backend(&root, &config)?;

    let mut processed = 0usize;
    let mut embedded = 0usize;
    let mut failed = 0usize;
    let mut failures = Vec::new();
    let mut records = pending_records_from_backend(&root, &config, limit, retry_failed)?;

    for record in &mut records {
        processed += 1;
        record.embedding_status = "embedding".to_string();
        record.embedding_attempts += 1;
        record.embedding_error = None;
        record.updated_at = now();
        crate::records::upsert_record(&root, record, None)?;

        let text = embedding_text(record);
        match embed(&config, &text).and_then(|vector| {
            record.embedding_status = "embedded".to_string();
            record.embedding_error = None;
            record.updated_at = now();
            upsert_vector(&root, &config, record, &vector)
        }) {
            Ok(()) => {
                embedded += 1;
            }
            Err(error) => {
                let message = error.to_string();
                record.embedding_status = "failed".to_string();
                record.embedding_error = Some(message.chars().take(500).collect());
                record.updated_at = now();
                crate::records::upsert_record(&root, record, None)?;
                failures.push(json!({"memory_id": record.uuid, "error": message}));
                failed += 1;
            }
        }
    }
    Ok(
        json!({"ok": true, "worker": {"processed": processed, "embedded": embedded, "failed": failed, "failures": failures}}),
    )
}
