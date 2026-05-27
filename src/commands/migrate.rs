fn cmd_migrate(
    root_arg: Option<PathBuf>,
    to_backend: BackendKind,
    remote_uri: Option<String>,
    remote_token: Option<String>,
    new_instance: bool,
    verify_remote: bool,
) -> Result<Value> {
    let root = runtime_root(root_arg)?;
    let mut config = load_runtime_config(&root)?;
    let mut records = read_records_from_backend(&root, &config)?;
    let old_storage = config.storage.clone();
    let mut new_storage = storage_for_backend(
        to_backend,
        remote_uri.or_else(|| old_storage.milvus_remote.uri.clone()),
        remote_token.or_else(|| old_storage.milvus_remote.token.clone()),
        new_instance,
    );
    if !new_instance {
        new_storage.instance_uuid = config.storage.instance_uuid.clone();
        normalize_storage_for_runtime(&mut new_storage, &root, &config.install_scope);
    }
    config.storage = new_storage;
    ensure_backend(&root, &config, verify_remote)?;
    write_runtime_config(&root, &config)?;
    let timestamp = now();
    for record in &mut records {
        record.embedding_status = "pending".to_string();
        record.embedding_error = None;
        record.embedding_attempts = 0;
        record.embedding_provider = config.embedding_provider.clone();
        record.embedding_model = config.embedding_model.clone();
        record.embedding_dim = config.embedding_dim;
        record.updated_at = timestamp.clone();
        upsert_record_to_backend(&root, &config, record, None)?;
    }
    let worker = cmd_worker(Some(root.clone()), None, true)?;
    persist_storage_to_user_config(&root, &config.storage)?;
    Ok(
        json!({"ok": true, "from": old_storage, "to": config.storage, "records_migrated": records.len(), "worker": worker["worker"].clone()}),
    )
}
