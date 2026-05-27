fn qdrant_upsert_record(
    root: &Path,
    config: &RuntimeConfig,
    record: &MemoryRecord,
    vector: Option<&[f32]>,
) -> Result<()> {
    ensure_qdrant_server(root, &config.storage.qdrant)?;
    let vector = match vector {
        Some(vector) => vector.to_vec(),
        None => qdrant_get_vector(root, config, &record.uuid)?
            .unwrap_or_else(|| zero_vector(config.embedding_dim)),
    };
    qdrant_put(
        &config.storage.qdrant,
        &format!(
            "/collections/{}/points?wait=true",
            active_collection_name(config)
        ),
        json!({
            "points": [{
                "id": record.uuid,
                "vector": vector,
                "payload": entity_for_record(record, &vector)?
            }]
        }),
    )?;
    Ok(())
}

fn qdrant_read_records(root: &Path, config: &RuntimeConfig) -> Result<Vec<MemoryRecord>> {
    ensure_qdrant_server(root, &config.storage.qdrant)?;
    let mut records = Vec::new();
    let mut offset = Value::Null;
    loop {
        let mut payload = json!({
            "limit": 256,
            "with_payload": true,
            "with_vector": false
        });
        if !offset.is_null() {
            payload["offset"] = offset.clone();
        }
        let response = qdrant_post(
            &config.storage.qdrant,
            &format!(
                "/collections/{}/points/scroll",
                active_collection_name(config)
            ),
            payload,
        )?;
        let result = &response["result"];
        if let Some(points) = result.get("points").and_then(Value::as_array) {
            for point in points {
                if let Some(payload) = point.get("payload") {
                    records.push(record_from_entity(payload)?);
                }
            }
        }
        offset = result
            .get("next_page_offset")
            .cloned()
            .unwrap_or(Value::Null);
        if offset.is_null() {
            break;
        }
    }
    Ok(records)
}

fn qdrant_get_record(
    root: &Path,
    config: &RuntimeConfig,
    memory_id: &str,
) -> Result<Option<MemoryRecord>> {
    ensure_qdrant_server(root, &config.storage.qdrant)?;
    let response = qdrant_post(
        &config.storage.qdrant,
        &format!("/collections/{}/points", active_collection_name(config)),
        json!({"ids": [memory_id], "with_payload": true, "with_vector": false}),
    )?;
    response
        .get("result")
        .and_then(Value::as_array)
        .and_then(|points| points.first())
        .and_then(|point| point.get("payload"))
        .filter(|payload| !payload.is_null())
        .map(record_from_entity)
        .transpose()
}

fn qdrant_delete_record(root: &Path, config: &RuntimeConfig, memory_id: &str) -> Result<()> {
    ensure_qdrant_server(root, &config.storage.qdrant)?;
    qdrant_post(
        &config.storage.qdrant,
        &format!(
            "/collections/{}/points/delete?wait=true",
            active_collection_name(config)
        ),
        json!({"points": [memory_id]}),
    )?;
    Ok(())
}

fn qdrant_get_vector(
    root: &Path,
    config: &RuntimeConfig,
    memory_id: &str,
) -> Result<Option<Vec<f32>>> {
    ensure_qdrant_server(root, &config.storage.qdrant)?;
    let response = qdrant_post(
        &config.storage.qdrant,
        &format!("/collections/{}/points", active_collection_name(config)),
        json!({"ids": [memory_id], "with_payload": false, "with_vector": true}),
    )?;
    Ok(response
        .get("result")
        .and_then(Value::as_array)
        .and_then(|points| points.first())
        .and_then(|point| point.get("vector"))
        .and_then(vector_from_entity))
}

fn qdrant_search_vectors(
    root: &Path,
    config: &RuntimeConfig,
    vector: &[f32],
    limit: usize,
) -> Result<Vec<VectorHit>> {
    ensure_qdrant_server(root, &config.storage.qdrant)?;
    let response = qdrant_post(
        &config.storage.qdrant,
        &format!(
            "/collections/{}/points/query",
            active_collection_name(config)
        ),
        json!({
            "query": vector,
            "limit": limit,
            "with_payload": false,
            "with_vector": false,
            "filter": {
                "must": [{
                    "key": "embedding_status",
                    "match": {"value": "embedded"}
                }]
            }
        }),
    )?;
    let points = response
        .get("result")
        .and_then(|result| result.get("points"))
        .cloned()
        .unwrap_or(Value::Null);
    parse_vector_hits(&points)
}

fn qdrant_get(config: &QdrantConfig, path: &str) -> Result<Value> {
    qdrant_request(config, reqwest::Method::GET, path, None)
}

fn qdrant_put(config: &QdrantConfig, path: &str, payload: Value) -> Result<Value> {
    qdrant_request(config, reqwest::Method::PUT, path, Some(payload))
}

fn qdrant_post(config: &QdrantConfig, path: &str, payload: Value) -> Result<Value> {
    qdrant_request(config, reqwest::Method::POST, path, Some(payload))
}

fn qdrant_request(
    config: &QdrantConfig,
    method: reqwest::Method,
    path: &str,
    payload: Option<Value>,
) -> Result<Value> {
    let client = reqwest::blocking::Client::new();
    let url = format!("{}{}", config.uri.trim_end_matches('/'), path);
    let mut request = client.request(method, &url);
    if let Some(payload) = payload {
        request = request.json(&payload);
    }
    let response = request
        .send()
        .with_context(|| format!("failed to call Qdrant API {path}"))?;
    let status = response.status();
    let body_text = response.text().unwrap_or_default();
    if !status.is_success() {
        bail!("Qdrant API {path} failed with HTTP {status}: {body_text}");
    }
    let body: Value = serde_json::from_str(&body_text).unwrap_or_else(|_| json!({}));
    if body.get("status").and_then(Value::as_str) == Some("error") {
        bail!("Qdrant API {path} failed: {body_text}");
    }
    Ok(body)
}

fn read_qdrant_server_state(root: &Path) -> Result<Option<Value>> {
    let path = project_path(root, ProjectPath::QdrantServerState);
    if !path.exists() {
        return Ok(None);
    }
    Ok(Some(serde_json::from_str(&fs::read_to_string(path)?)?))
}

fn write_qdrant_server_state(root: &Path, state: &Value) -> Result<()> {
    let path = project_path(root, ProjectPath::QdrantServerState);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string_pretty(state)? + "\n")?;
    Ok(())
}

fn qdrant_host_port(uri: &str) -> Result<(String, u16)> {
    let parsed = reqwest::Url::parse(uri).with_context(|| format!("invalid Qdrant uri: {uri}"))?;
    let host = parsed
        .host_str()
        .ok_or_else(|| anyhow!("Qdrant uri must include a host: {uri}"))?
        .to_string();
    let port = parsed.port_or_known_default().unwrap_or(6333);
    Ok((host, port))
}

pub(crate) fn active_collection_name(config: &RuntimeConfig) -> String {
    config.collection_name.clone()
}

pub(crate) fn search_vector_backend(
    root: &Path,
    config: &RuntimeConfig,
    vector: &[f32],
    limit: usize,
) -> Result<Vec<VectorHit>> {
    qdrant_search_vectors(root, config, vector, limit)
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
