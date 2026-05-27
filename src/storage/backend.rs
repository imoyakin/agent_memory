pub(crate) fn ensure_backend(
    root: &Path,
    config: &RuntimeConfig,
    verify_remote: bool,
) -> Result<()> {
    match config.storage.backend {
        BackendKind::Qdrant => {
            ensure_qdrant_server(root, &config.storage.qdrant)?;
            ensure_qdrant_collection(config)?;
            Ok(())
        }
        BackendKind::MilvusLite => {
            let db = resolve_under_root(root, &config.storage.milvus_lite.db_path);
            lite_bridge(
                &[
                    "ensure".to_string(),
                    "--db".to_string(),
                    db.to_string_lossy().to_string(),
                    "--collection".to_string(),
                    config.collection_name.clone(),
                    "--dim".to_string(),
                    config.embedding_dim.to_string(),
                ],
                None,
            )?;
            Ok(())
        }
        BackendKind::MilvusRemote => {
            let uri = config
                .storage
                .milvus_remote
                .uri
                .as_deref()
                .ok_or_else(|| anyhow!("remote uri is required"))?;
            if verify_remote {
                remote_health(uri, config.storage.milvus_remote.token.as_deref())?;
            }
            create_remote_database(config)?;
            ensure_remote_collection(config)?;
            Ok(())
        }
    }
}

pub(crate) fn create_remote_database(config: &RuntimeConfig) -> Result<()> {
    let remote = &config.storage.milvus_remote;
    milvus_rest_post(
        remote,
        "/v2/vectordb/databases/create",
        json!({"dbName": remote.database}),
        true,
    )?;
    Ok(())
}

pub(crate) fn ensure_remote_collection(config: &RuntimeConfig) -> Result<()> {
    let remote = &config.storage.milvus_remote;
    milvus_rest_post(
        remote,
        "/v2/vectordb/collections/create",
        json!({
            "dbName": remote.database,
            "collectionName": active_collection_name(config),
            "dimension": config.embedding_dim,
            "metricType": "COSINE",
            "primaryFieldName": PRIMARY_FIELD,
            "idType": "VarChar",
            "autoId": false,
            "vectorFieldName": VECTOR_FIELD,
            "params": {
                "max_length": "64"
            }
        }),
        true,
    )?;
    Ok(())
}

pub(crate) fn remote_health(uri: &str, token: Option<&str>) -> Result<()> {
    let runtime = tokio::runtime::Runtime::new()?;
    let uri = uri.to_string();
    runtime.block_on(async {
        let _client =
            if let Some((username, password)) = token.and_then(|value| value.split_once(':')) {
                milvus::client::ClientBuilder::new(uri)
                    .username(username)
                    .password(password)
                    .build()
                    .await?
            } else {
                milvus::client::Client::new(uri).await?
            };
        Ok::<(), anyhow::Error>(())
    })?;
    Ok(())
}
