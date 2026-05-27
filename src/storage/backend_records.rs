pub(crate) fn upsert_record_to_backend(
    root: &Path,
    config: &RuntimeConfig,
    record: &MemoryRecord,
    vector: Option<&[f32]>,
) -> Result<()> {
    ensure_backend(root, config, false)?;
    match config.storage.backend {
        BackendKind::Qdrant => qdrant_upsert_record(root, config, record, vector),
        BackendKind::MilvusLite => {
            let db = resolve_under_root(root, &config.storage.milvus_lite.db_path);
            let payload = json!({
                "record": record_value(record),
                "vector": vector,
                "reliability": reliability_score(record),
            });
            lite_bridge(
                &[
                    "upsert".to_string(),
                    "--db".to_string(),
                    db.to_string_lossy().to_string(),
                    "--collection".to_string(),
                    config.collection_name.clone(),
                    "--dim".to_string(),
                    config.embedding_dim.to_string(),
                ],
                Some(payload),
            )?;
            Ok(())
        }
        BackendKind::MilvusRemote => remote_upsert_record(config, record, vector),
    }
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
    match config.storage.backend {
        BackendKind::Qdrant => qdrant_read_records(root, config),
        BackendKind::MilvusLite => {
            let db = resolve_under_root(root, &config.storage.milvus_lite.db_path);
            if let Some(uri) = active_lite_viewer_uri(root, &db)? {
                let response = lite_bridge(
                    &[
                        "list".to_string(),
                        "--uri".to_string(),
                        uri,
                        "--collection".to_string(),
                        config.collection_name.clone(),
                    ],
                    None,
                )?;
                return records_from_value(&response["records"]);
            }
            let response = lite_bridge(
                &[
                    "list".to_string(),
                    "--db".to_string(),
                    db.to_string_lossy().to_string(),
                    "--collection".to_string(),
                    config.collection_name.clone(),
                ],
                None,
            )?;
            records_from_value(&response["records"])
        }
        BackendKind::MilvusRemote => remote_query_records(config, ""),
    }
}

pub(crate) fn get_record_from_backend(
    root: &Path,
    config: &RuntimeConfig,
    memory_id: &str,
) -> Result<Option<MemoryRecord>> {
    match config.storage.backend {
        BackendKind::Qdrant => qdrant_get_record(root, config, memory_id),
        BackendKind::MilvusLite => {
            let db = resolve_under_root(root, &config.storage.milvus_lite.db_path);
            if let Some(uri) = active_lite_viewer_uri(root, &db)? {
                let response = lite_bridge(
                    &[
                        "get".to_string(),
                        "--uri".to_string(),
                        uri,
                        "--collection".to_string(),
                        config.collection_name.clone(),
                        "--id".to_string(),
                        memory_id.to_string(),
                    ],
                    None,
                )?;
                return response
                    .get("record")
                    .filter(|value| !value.is_null())
                    .map(record_from_entity)
                    .transpose();
            }
            let response = lite_bridge(
                &[
                    "get".to_string(),
                    "--db".to_string(),
                    db.to_string_lossy().to_string(),
                    "--collection".to_string(),
                    config.collection_name.clone(),
                    "--id".to_string(),
                    memory_id.to_string(),
                ],
                None,
            )?;
            response
                .get("record")
                .filter(|value| !value.is_null())
                .map(record_from_entity)
                .transpose()
        }
        BackendKind::MilvusRemote => remote_get_record(config, memory_id),
    }
}

pub(crate) fn delete_record_from_backend(
    root: &Path,
    config: &RuntimeConfig,
    memory_id: &str,
) -> Result<bool> {
    ensure_backend(root, config, false)?;
    let existed = get_record_from_backend(root, config, memory_id)?.is_some();
    if !existed {
        return Ok(false);
    }
    match config.storage.backend {
        BackendKind::Qdrant => {
            qdrant_delete_record(root, config, memory_id)?;
        }
        BackendKind::MilvusLite => {
            let db = resolve_under_root(root, &config.storage.milvus_lite.db_path);
            lite_bridge(
                &[
                    "delete".to_string(),
                    "--db".to_string(),
                    db.to_string_lossy().to_string(),
                    "--collection".to_string(),
                    config.collection_name.clone(),
                    "--id".to_string(),
                    memory_id.to_string(),
                ],
                None,
            )?;
        }
        BackendKind::MilvusRemote => {
            let remote = &config.storage.milvus_remote;
            milvus_rest_post(
                remote,
                "/v2/vectordb/entities/delete",
                json!({
                    "dbName": remote.database,
                    "collectionName": active_collection_name(config),
                    "filter": format!(
                        "{} == \"{}\"",
                        PRIMARY_FIELD,
                        escape_milvus_string(memory_id)
                    )
                }),
                false,
            )?;
        }
    }
    Ok(true)
}

pub(crate) fn pending_records_from_backend(
    root: &Path,
    config: &RuntimeConfig,
    limit: Option<usize>,
    retry_failed: bool,
) -> Result<Vec<MemoryRecord>> {
    ensure_backend(root, config, false)?;
    let max = limit.unwrap_or(usize::MAX);
    let records = match config.storage.backend {
        BackendKind::Qdrant => qdrant_read_records(root, config)?
            .into_iter()
            .filter(|record| {
                record.embedding_status == "pending"
                    || (retry_failed && record.embedding_status == "failed")
            })
            .take(max)
            .collect(),
        BackendKind::MilvusLite => {
            let db = resolve_under_root(root, &config.storage.milvus_lite.db_path);
            let response = lite_bridge(
                &[
                    "pending".to_string(),
                    "--db".to_string(),
                    db.to_string_lossy().to_string(),
                    "--collection".to_string(),
                    config.collection_name.clone(),
                    "--limit".to_string(),
                    max.to_string(),
                    if retry_failed {
                        "--retry-failed".to_string()
                    } else {
                        "--no-retry-failed".to_string()
                    },
                ],
                None,
            )?;
            records_from_value(&response["records"])?
        }
        BackendKind::MilvusRemote => remote_query_records(config, "")?
            .into_iter()
            .filter(|record| {
                record.embedding_status == "pending"
                    || (retry_failed && record.embedding_status == "failed")
            })
            .take(max)
            .collect(),
    };
    Ok(records)
}
