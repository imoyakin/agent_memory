pub(crate) fn remote_upsert_record(
    config: &RuntimeConfig,
    record: &MemoryRecord,
    vector: Option<&[f32]>,
) -> Result<()> {
    let remote = &config.storage.milvus_remote;
    let vector = match vector {
        Some(vector) => vector.to_vec(),
        None => remote_get_vector(config, &record.uuid)?
            .unwrap_or_else(|| zero_vector(config.embedding_dim)),
    };
    let mut entity = entity_for_record(record, &vector)?;
    entity.insert("reliability".to_string(), json!(reliability_score(record)));
    milvus_rest_post(
        remote,
        "/v2/vectordb/entities/upsert",
        json!({
            "dbName": remote.database,
            "collectionName": active_collection_name(config),
            "data": [Value::Object(entity)]
        }),
        false,
    )?;
    Ok(())
}

pub(crate) fn milvus_rest_post(
    remote: &MilvusRemoteConfig,
    path: &str,
    payload: Value,
    accept_exists: bool,
) -> Result<Value> {
    let uri = remote
        .uri
        .as_deref()
        .ok_or_else(|| anyhow!("remote uri is required"))?;
    let client = reqwest::blocking::Client::new();
    let mut request = client
        .post(format!("{}{}", uri.trim_end_matches('/'), path))
        .json(&payload);
    if let Some(token) = &remote.token {
        request = request.bearer_auth(token);
    }
    let response = request
        .send()
        .with_context(|| format!("failed to call Milvus REST API {path}"))?;
    let status = response.status();
    let body_text = response.text().unwrap_or_default();
    if !status.is_success() {
        bail!("Milvus REST API {path} failed with HTTP {status}: {body_text}");
    }
    let body: Value = serde_json::from_str(&body_text).unwrap_or_else(|_| json!({}));
    let code = body.get("code").and_then(Value::as_i64).unwrap_or(0);
    if code != 0 {
        let message = body
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("unknown error");
        if accept_exists && message.to_lowercase().contains("exist") {
            return Ok(body);
        }
        bail!("Milvus REST API {path} failed: {message}");
    }
    Ok(body)
}

pub(crate) fn active_collection_name(config: &RuntimeConfig) -> String {
    match config.storage.backend {
        BackendKind::Qdrant => config.collection_name.clone(),
        BackendKind::MilvusLite => config.collection_name.clone(),
        BackendKind::MilvusRemote => config.storage.milvus_remote.collection.clone(),
    }
}

pub(crate) fn lite_bridge(args: &[String], stdin_json: Option<Value>) -> Result<Value> {
    let bridge = skill_bin_path("agent-memory-lite-bridge")?;
    let mut command = if bridge.is_file() {
        let mut command = Command::new(&bridge);
        command.args(args);
        command
    } else {
        let skill_root = skill_root()?;
        let mut command = Command::new("uv");
        command
            .arg("run")
            .arg("--project")
            .arg(&skill_root)
            .arg("python")
            .arg("-m")
            .arg("agent_memory.lite_bridge")
            .args(args);
        command
    };
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    if stdin_json.is_some() {
        command.stdin(Stdio::piped());
    }
    let mut child = command
        .spawn()
        .with_context(|| "failed to start Python Milvus Lite bridge")?;
    if let Some(payload) = stdin_json {
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("failed to open bridge stdin"))?;
        stdin.write_all(serde_json::to_string(&payload)?.as_bytes())?;
    }
    let output = child.wait_with_output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Milvus Lite bridge failed: {}", stderr.trim());
    }
    let stdout = String::from_utf8(output.stdout)?;
    Ok(serde_json::from_str(stdout.trim()).unwrap_or_else(|_| json!({"ok": true})))
}

pub(crate) fn search_vector_backend(
    root: &Path,
    config: &RuntimeConfig,
    vector: &[f32],
    limit: usize,
) -> Result<Vec<VectorHit>> {
    match config.storage.backend {
        BackendKind::Qdrant => qdrant_search_vectors(root, config, vector, limit),
        BackendKind::MilvusLite => {
            let db = resolve_under_root(root, &config.storage.milvus_lite.db_path);
            let payload = json!({"vector": vector});
            if let Some(uri) = active_lite_viewer_uri(root, &db)? {
                let response = lite_bridge(
                    &[
                        "search".to_string(),
                        "--uri".to_string(),
                        uri,
                        "--collection".to_string(),
                        config.collection_name.clone(),
                        "--limit".to_string(),
                        limit.to_string(),
                    ],
                    Some(payload),
                )?;
                return parse_vector_hits(&response["hits"]);
            }
            let response = lite_bridge(
                &[
                    "search".to_string(),
                    "--db".to_string(),
                    db.to_string_lossy().to_string(),
                    "--collection".to_string(),
                    config.collection_name.clone(),
                    "--limit".to_string(),
                    limit.to_string(),
                ],
                Some(payload),
            )?;
            parse_vector_hits(&response["hits"])
        }
        BackendKind::MilvusRemote => remote_search_vectors(config, vector, limit),
    }
}

pub(crate) fn remote_search_vectors(
    config: &RuntimeConfig,
    vector: &[f32],
    limit: usize,
) -> Result<Vec<VectorHit>> {
    let remote = &config.storage.milvus_remote;
    let response = milvus_rest_post(
        remote,
        "/v2/vectordb/entities/search",
        json!({
            "dbName": remote.database,
            "collectionName": active_collection_name(config),
            "data": [vector],
            "annsField": VECTOR_FIELD,
            "filter": "embedding_status == \"embedded\"",
            "limit": limit,
            "outputFields": [PRIMARY_FIELD],
            "searchParams": {
                "metricType": "COSINE",
                "params": {}
            }
        }),
        false,
    )?;
    parse_vector_hits(&response["data"])
}

pub(crate) fn parse_vector_hits(value: &Value) -> Result<Vec<VectorHit>> {
    let mut hits = Vec::new();
    let Some(items) = value.as_array() else {
        return Ok(hits);
    };
    for item in items {
        let Some(object) = item.as_object() else {
            continue;
        };
        let uuid = object
            .get(PRIMARY_FIELD)
            .or_else(|| object.get("id"))
            .and_then(|value| {
                value
                    .as_str()
                    .map(ToString::to_string)
                    .or_else(|| value.as_i64().map(|item| item.to_string()))
            });
        let Some(uuid) = uuid else {
            continue;
        };
        let distance = object
            .get("distance")
            .or_else(|| object.get("score"))
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        hits.push(VectorHit { uuid, distance });
    }
    Ok(hits)
}

fn active_lite_viewer_uri(root: &Path, db: &Path) -> Result<Option<String>> {
    let path = project_path(root, ProjectPath::MilvusLiteServerState);
    if !path.exists() {
        return Ok(None);
    }
    let state: Value = serde_json::from_str(&fs::read_to_string(path)?)?;
    let pid = state.get("pid").and_then(Value::as_u64).unwrap_or(0) as u32;
    if !crate::service::pid_exists(pid) {
        return Ok(None);
    }
    let Some(data_dir) = state
        .get("data_dir")
        .and_then(Value::as_str)
        .map(PathBuf::from)
    else {
        return Ok(None);
    };
    if normalize_path(&data_dir) != normalize_path(db) {
        return Ok(None);
    }
    Ok(state
        .get("endpoint")
        .and_then(Value::as_str)
        .map(ToString::to_string))
}

fn normalize_path(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn remote_query_records(config: &RuntimeConfig, filter: &str) -> Result<Vec<MemoryRecord>> {
    let remote = &config.storage.milvus_remote;
    let response = milvus_rest_post(
        remote,
        "/v2/vectordb/entities/query",
        json!({
            "dbName": remote.database,
            "collectionName": active_collection_name(config),
            "filter": filter,
            "outputFields": RECORD_OUTPUT_FIELDS,
        }),
        false,
    )?;
    records_from_value(&response["data"])
}

fn remote_get_record(config: &RuntimeConfig, memory_id: &str) -> Result<Option<MemoryRecord>> {
    let remote = &config.storage.milvus_remote;
    let response = milvus_rest_post(
        remote,
        "/v2/vectordb/entities/get",
        json!({
            "dbName": remote.database,
            "collectionName": active_collection_name(config),
            "id": memory_id,
            "outputFields": RECORD_OUTPUT_FIELDS,
        }),
        false,
    )?;
    response
        .get("data")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .map(record_from_entity)
        .transpose()
}

fn remote_get_vector(config: &RuntimeConfig, memory_id: &str) -> Result<Option<Vec<f32>>> {
    let remote = &config.storage.milvus_remote;
    let response = milvus_rest_post(
        remote,
        "/v2/vectordb/entities/get",
        json!({
            "dbName": remote.database,
            "collectionName": active_collection_name(config),
            "id": memory_id,
            "outputFields": [VECTOR_FIELD],
        }),
        false,
    )?;
    Ok(response
        .get("data")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .and_then(vector_from_entity))
}
