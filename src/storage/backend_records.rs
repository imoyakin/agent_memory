pub(crate) fn upsert_record_to_backend(
    root: &Path,
    config: &RuntimeConfig,
    record: &MemoryRecord,
    vector: Option<&[f32]>,
) -> Result<()> {
    ensure_backend(root, config)?;
    qdrant_upsert_record(root, config, record, vector)
}

pub(crate) fn upsert_vector(
    root: &Path,
    config: &RuntimeConfig,
    record: &MemoryRecord,
    vector: &[f32],
) -> Result<()> {
    upsert_record_to_backend(root, config, record, Some(vector))
}

pub(crate) fn read_records_from_backend(
    root: &Path,
    config: &RuntimeConfig,
) -> Result<Vec<MemoryRecord>> {
    qdrant_read_records(root, config)
}

pub(crate) fn get_record_from_backend(
    root: &Path,
    config: &RuntimeConfig,
    memory_id: &str,
) -> Result<Option<MemoryRecord>> {
    qdrant_get_record(root, config, memory_id)
}

pub(crate) fn delete_record_from_backend(
    root: &Path,
    config: &RuntimeConfig,
    memory_id: &str,
) -> Result<bool> {
    ensure_backend(root, config)?;
    let existed = get_record_from_backend(root, config, memory_id)?.is_some();
    if !existed {
        return Ok(false);
    }
    qdrant_delete_record(root, config, memory_id)?;
    Ok(true)
}

pub(crate) fn pending_records_from_backend(
    root: &Path,
    config: &RuntimeConfig,
    limit: Option<usize>,
    retry_failed: bool,
) -> Result<Vec<MemoryRecord>> {
    ensure_backend(root, config)?;
    let max = limit.unwrap_or(usize::MAX);
    Ok(qdrant_read_records(root, config)?
        .into_iter()
        .filter(|record| {
            record.embedding_status == "pending"
                || (retry_failed && record.embedding_status == "failed")
        })
        .take(max)
        .collect())
}
