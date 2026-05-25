use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Map, Value};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::config::{BackendKind, MilvusRemoteConfig, RuntimeConfig};
use crate::models::{MemoryRecord, VectorHit};
use crate::paths::{resolve_under_root, skill_bin_path, skill_root};
use crate::records::record_value;
use crate::search::reliability_score;
use crate::{DEFAULT_DIM, PRIMARY_FIELD, SCHEMA_VERSION, VECTOR_FIELD};

const RECORD_OUTPUT_FIELDS: &[&str] = &[
    PRIMARY_FIELD,
    "content",
    "keys",
    "summary",
    "embedding_status",
    "embedding_error",
    "embedding_attempts",
    "memory_type",
    "scope",
    "root_path",
    "tags",
    "source_kind",
    "source_ref",
    "created_at",
    "updated_at",
    "last_accessed_at",
    "access_count",
    "conflict_count",
    "confidence",
    "verified_at",
    "stale_after_days",
    "embedding_provider",
    "embedding_model",
    "embedding_dim",
    "schema_version",
    "reliability",
];

pub(crate) fn ensure_backend(
    root: &Path,
    config: &RuntimeConfig,
    verify_remote: bool,
) -> Result<()> {
    match config.storage.backend {
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

pub(crate) fn upsert_record_to_backend(
    root: &Path,
    config: &RuntimeConfig,
    record: &MemoryRecord,
    vector: Option<&[f32]>,
) -> Result<()> {
    ensure_backend(root, config, false)?;
    match config.storage.backend {
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
    ensure_backend(root, config, false)?;
    match config.storage.backend {
        BackendKind::MilvusLite => {
            let db = resolve_under_root(root, &config.storage.milvus_lite.db_path);
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
    ensure_backend(root, config, false)?;
    match config.storage.backend {
        BackendKind::MilvusLite => {
            let db = resolve_under_root(root, &config.storage.milvus_lite.db_path);
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
        BackendKind::MilvusLite => {
            let db = resolve_under_root(root, &config.storage.milvus_lite.db_path);
            let payload = json!({"vector": vector});
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

fn entity_for_record(record: &MemoryRecord, vector: &[f32]) -> Result<Map<String, Value>> {
    let mut entity = Map::new();
    entity.insert(PRIMARY_FIELD.to_string(), json!(record.uuid.clone()));
    entity.insert("content".to_string(), json!(record.content.clone()));
    entity.insert(
        "keys".to_string(),
        json!(serde_json::to_string(&record.keys)?),
    );
    entity.insert("summary".to_string(), json!(record.summary.clone()));
    entity.insert(
        "embedding_status".to_string(),
        json!(record.embedding_status.clone()),
    );
    entity.insert(
        "embedding_error".to_string(),
        json!(record.embedding_error.clone().unwrap_or_default()),
    );
    entity.insert(
        "embedding_attempts".to_string(),
        json!(record.embedding_attempts as i64),
    );
    entity.insert("memory_type".to_string(), json!(record.memory_type.clone()));
    entity.insert("scope".to_string(), json!(record.scope.clone()));
    entity.insert("root_path".to_string(), json!(record.root_path.clone()));
    entity.insert(
        "tags".to_string(),
        json!(serde_json::to_string(&record.tags)?),
    );
    entity.insert("source_kind".to_string(), json!(record.source_kind.clone()));
    entity.insert("source_ref".to_string(), json!(record.source_ref.clone()));
    entity.insert("created_at".to_string(), json!(record.created_at.clone()));
    entity.insert("updated_at".to_string(), json!(record.updated_at.clone()));
    entity.insert(
        "last_accessed_at".to_string(),
        json!(record.last_accessed_at.clone().unwrap_or_default()),
    );
    entity.insert(
        "access_count".to_string(),
        json!(record.access_count as i64),
    );
    entity.insert(
        "conflict_count".to_string(),
        json!(record.conflict_count as i64),
    );
    entity.insert("confidence".to_string(), json!(record.confidence));
    entity.insert(
        "verified_at".to_string(),
        json!(record.verified_at.clone().unwrap_or_default()),
    );
    entity.insert(
        "stale_after_days".to_string(),
        json!(record
            .stale_after_days
            .map(|value| value as i64)
            .unwrap_or(-1)),
    );
    entity.insert(
        "embedding_provider".to_string(),
        json!(record.embedding_provider.clone()),
    );
    entity.insert(
        "embedding_model".to_string(),
        json!(record.embedding_model.clone()),
    );
    entity.insert(
        "embedding_dim".to_string(),
        json!(record.embedding_dim as i64),
    );
    entity.insert(
        "schema_version".to_string(),
        json!(record.schema_version as i64),
    );
    entity.insert(VECTOR_FIELD.to_string(), json!(vector));
    Ok(entity)
}

fn records_from_value(value: &Value) -> Result<Vec<MemoryRecord>> {
    let Some(items) = value.as_array() else {
        return Ok(Vec::new());
    };
    items.iter().map(record_from_entity).collect()
}

fn record_from_entity(value: &Value) -> Result<MemoryRecord> {
    let updated_at = string_field(value, "updated_at").unwrap_or_default();
    Ok(MemoryRecord {
        uuid: string_field(value, PRIMARY_FIELD)
            .or_else(|| string_field(value, "memory_id"))
            .unwrap_or_default(),
        content: string_field(value, "content").unwrap_or_default(),
        keys: string_list_field(value, "keys"),
        summary: string_field(value, "summary").unwrap_or_default(),
        embedding_status: string_field(value, "embedding_status")
            .unwrap_or_else(|| "pending".to_string()),
        embedding_error: option_string_field(value, "embedding_error"),
        embedding_attempts: u64_field(value, "embedding_attempts").unwrap_or_default(),
        memory_type: string_field(value, "memory_type").unwrap_or_else(|| "project".to_string()),
        scope: string_field(value, "scope").unwrap_or_else(|| "project".to_string()),
        root_path: string_field(value, "root_path").unwrap_or_default(),
        tags: string_list_field(value, "tags"),
        source_kind: string_field(value, "source_kind")
            .unwrap_or_else(|| "agent_inferred".to_string()),
        source_ref: string_field(value, "source_ref").unwrap_or_default(),
        created_at: string_field(value, "created_at").unwrap_or_else(|| updated_at.clone()),
        updated_at,
        last_accessed_at: option_string_field(value, "last_accessed_at"),
        access_count: u64_field(value, "access_count").unwrap_or_default(),
        conflict_count: u64_field(value, "conflict_count").unwrap_or_default(),
        confidence: f64_field(value, "confidence").unwrap_or_default(),
        verified_at: option_string_field(value, "verified_at"),
        stale_after_days: option_u64_field(value, "stale_after_days"),
        embedding_provider: string_field(value, "embedding_provider").unwrap_or_default(),
        embedding_model: string_field(value, "embedding_model").unwrap_or_default(),
        embedding_dim: u64_field(value, "embedding_dim")
            .map(|value| value as usize)
            .unwrap_or(DEFAULT_DIM),
        schema_version: u64_field(value, "schema_version")
            .map(|value| value as u32)
            .unwrap_or(SCHEMA_VERSION),
    })
}

fn string_field(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(|item| {
        item.as_str()
            .map(ToString::to_string)
            .or_else(|| item.as_i64().map(|number| number.to_string()))
            .or_else(|| item.as_u64().map(|number| number.to_string()))
            .or_else(|| item.as_f64().map(|number| number.to_string()))
    })
}

fn option_string_field(value: &Value, key: &str) -> Option<String> {
    string_field(value, key).filter(|item| !item.is_empty())
}

fn string_list_field(value: &Value, key: &str) -> Vec<String> {
    let Some(item) = value.get(key) else {
        return Vec::new();
    };
    if let Some(items) = item.as_array() {
        return items
            .iter()
            .filter_map(|item| item.as_str().map(ToString::to_string))
            .collect();
    }
    let Some(text) = item.as_str() else {
        return Vec::new();
    };
    if let Ok(items) = serde_json::from_str::<Vec<String>>(text) {
        return items;
    }
    text.split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn u64_field(value: &Value, key: &str) -> Option<u64> {
    value.get(key).and_then(|item| {
        item.as_u64().or_else(|| {
            item.as_i64()
                .and_then(|number| u64::try_from(number).ok())
                .or_else(|| item.as_str().and_then(|text| text.parse().ok()))
        })
    })
}

fn option_u64_field(value: &Value, key: &str) -> Option<u64> {
    value.get(key).and_then(|item| {
        if item.as_i64() == Some(-1) {
            None
        } else {
            u64_field(value, key)
        }
    })
}

fn f64_field(value: &Value, key: &str) -> Option<f64> {
    value.get(key).and_then(|item| {
        item.as_f64()
            .or_else(|| item.as_str().and_then(|text| text.parse().ok()))
    })
}

fn vector_from_entity(value: &Value) -> Option<Vec<f32>> {
    value.get(VECTOR_FIELD).and_then(|item| {
        item.as_array().map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_f64().map(|number| number as f32))
                .collect()
        })
    })
}

fn zero_vector(dim: usize) -> Vec<f32> {
    vec![0.0; dim.max(1)]
}

fn escape_milvus_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}
