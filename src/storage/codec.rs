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
