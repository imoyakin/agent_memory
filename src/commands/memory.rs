#[allow(clippy::too_many_arguments)]
fn cmd_add(
    root_arg: Option<PathBuf>,
    content: String,
    memory_type: String,
    source_kind: String,
    source_ref: String,
    confidence: f64,
    scope: String,
    tags: String,
    keys: String,
) -> Result<Value> {
    validate_content(&content)?;
    if !(0.0..=1.0).contains(&confidence) {
        bail!("confidence must be between 0.0 and 1.0");
    }
    let root = runtime_root(root_arg)?;
    let config = load_runtime_config(&root)?;
    if !config.allowed_memory_types.contains(&memory_type) {
        bail!("memory_type '{}' is disabled by this config", memory_type);
    }
    if config.install_scope == "global" && scope != "global" {
        bail!("global agent-memory requires --scope global");
    }
    let tags = parse_csv(&tags);
    let mut keys = parse_csv(&keys);
    if keys.is_empty() {
        keys = derive_keys(&content, &source_ref, &tags);
    }
    let timestamp = now();
    let record = MemoryRecord {
        uuid: Uuid::new_v4().to_string(),
        content,
        keys,
        summary: summarize(&source_ref, &timestamp, &memory_type), // overwritten below
        embedding_status: "pending".to_string(),
        embedding_error: None,
        embedding_attempts: 0,
        memory_type,
        scope,
        root_path: root.to_string_lossy().to_string(),
        tags,
        source_kind,
        source_ref,
        created_at: timestamp.clone(),
        updated_at: timestamp.clone(),
        last_accessed_at: None,
        access_count: 0,
        conflict_count: 0,
        confidence,
        verified_at: None,
        stale_after_days: None,
        embedding_provider: config.embedding_provider.clone(),
        embedding_model: config.embedding_model.clone(),
        embedding_dim: config.embedding_dim,
        schema_version: SCHEMA_VERSION,
    }
    .with_summary();
    ensure_backend(&root, &config)?;
    upsert_record(&root, &record, None)?;
    Ok(json!({"ok": true, "record": record_value(&record)}))
}

fn cmd_dump(root_arg: Option<PathBuf>, output: Option<PathBuf>) -> Result<Value> {
    let root = runtime_root(root_arg)?;
    let payload = json!({
        "config": load_runtime_config(&root).ok(),
        "records": read_records(&root)?.iter().map(record_value).collect::<Vec<_>>(),
    });
    let path = output.unwrap_or_else(|| {
        project_path(&root, ProjectPath::MemoryDir)
            .join("dumps")
            .join(format!("memory-{}.json", safe_timestamp()))
    });
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, serde_json::to_string_pretty(&payload)? + "\n")?;
    Ok(json!({"ok": true, "dump": path}))
}

fn cmd_search(
    root_arg: Option<PathBuf>,
    query: String,
    limit: Option<usize>,
    memory_type: Option<String>,
    scope: Option<String>,
    tags: String,
) -> Result<Value> {
    let root = runtime_root(root_arg)?;
    let config = load_runtime_config(&root)?;
    let required_tags = parse_csv(&tags);
    let records = filtered_records(
        &root,
        memory_type.as_deref(),
        scope.as_deref(),
        &required_tags,
    )?;
    let direct_limit = limit.unwrap_or(config.retrieval.default_limit).max(1);
    let vector_limit = direct_limit
        + if config.retrieval.associative.enabled {
            config.retrieval.associative.limit
        } else {
            0
        }
        + 8;
    let mut warnings = Vec::new();
    let mut direct = exact_results(&records, &query);
    if records
        .iter()
        .any(|record| record.embedding_status == "embedded")
    {
        match vector_results(&root, &config, &records, &query, vector_limit) {
            Ok(vector_matches) => merge_search_results(&mut direct, vector_matches),
            Err(error) => warnings.push(format!("vector search unavailable: {error}")),
        }
    }
    direct.truncate(direct_limit);
    let mut results = direct.clone();
    if config.retrieval.associative.enabled && config.retrieval.associative.limit > 0 {
        let selected: HashSet<_> = direct.iter().map(|item| item.uuid.clone()).collect();
        let seed = direct
            .iter()
            .flat_map(|item| item.matched_keys.iter().chain(item.keys.iter()))
            .take(16)
            .cloned()
            .collect::<Vec<_>>()
            .join(" ");
        if !seed.is_empty() {
            let candidates: Vec<_> = records
                .iter()
                .filter(|record| !selected.contains(&record.uuid))
                .cloned()
                .collect();
            let mut associative = exact_results(&candidates, &seed);
            for result in &mut associative {
                result.match_reasons.retain(|reason| reason == "pending");
                result
                    .match_reasons
                    .insert(0, "associative_keys".to_string());
                result.relevance = round4(result.relevance * 0.92);
                result.score = combined_score(result.relevance, result.reliability);
            }
            associative.retain(|item| item.score >= config.retrieval.associative.min_score);
            associative.truncate(config.retrieval.associative.limit);
            results.extend(associative);
        }
    }
    mark_accessed(
        &root,
        results.iter().map(|item| item.uuid.clone()).collect(),
    )?;
    Ok(json!({"ok": true, "query": query, "warnings": warnings, "results": results}))
}

fn cmd_list(
    root_arg: Option<PathBuf>,
    memory_type: Option<String>,
    scope: Option<String>,
    tags: String,
) -> Result<Value> {
    let root = runtime_root(root_arg)?;
    let tags = parse_csv(&tags);
    let records = filtered_records(&root, memory_type.as_deref(), scope.as_deref(), &tags)?;
    Ok(json!({"ok": true, "records": records.iter().map(record_value).collect::<Vec<_>>()}))
}

fn cmd_audit(root_arg: Option<PathBuf>) -> Result<Value> {
    let root = runtime_root(root_arg)?;
    let records = read_records(&root)?;
    let mut records_by_status: HashMap<String, usize> = HashMap::new();
    let mut records_by_type: HashMap<String, usize> = HashMap::new();
    let mut embedding_queue: HashMap<String, usize> = HashMap::new();
    for record in &records {
        *records_by_status
            .entry(record.embedding_status.clone())
            .or_default() += 1;
        *records_by_type
            .entry(record.memory_type.clone())
            .or_default() += 1;
        if matches!(
            record.embedding_status.as_str(),
            "pending" | "embedding" | "failed"
        ) {
            *embedding_queue
                .entry(record.embedding_status.clone())
                .or_default() += 1;
        }
    }
    let config = load_runtime_config(&root)?;
    Ok(json!({
        "ok": true,
        "root": root,
        "storage": config.storage,
        "records": records.len(),
        "records_by_status": records_by_status,
        "records_by_type": records_by_type,
        "embedding_queue": embedding_queue
    }))
}
