fn cmd_ps() -> Result<Value> {
    let gateway = gateway_status_value()?;
    let main = gateway_main_process(&gateway);
    let (memories, pruned) = memory_processes()?;
    let remotes = gateway_remote_statuses(gateway.get("endpoint").and_then(Value::as_str))?;
    Ok(ps_payload(main, memories, remotes, pruned))
}

fn ps_payload(
    main: Option<Value>,
    memories: Vec<Value>,
    remotes: Vec<Value>,
    pruned: usize,
) -> Value {
    json!({
        "main": main.unwrap_or(Value::Null),
        "memories": memories,
        "remotes": remotes,
        "pruned": pruned,
    })
}

fn gateway_main_process(gateway: &Value) -> Option<Value> {
    if gateway.get("active").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    Some(json!({
        "role": "main",
        "scope": "local",
        "workdir": Value::Null,
        "root": Value::Null,
        "status": "running",
        "viewer": gateway.get("endpoint").cloned().unwrap_or(Value::Null),
        "pid": gateway.get("pid").cloned().unwrap_or(Value::Null),
        "gateway": gateway,
    }))
}

fn memory_processes() -> Result<(Vec<Value>, usize)> {
    let mut entries = Vec::new();
    let mut pruned = 0usize;
    let endpoints = crate::ipc::discover_service_endpoints()?;
    if !endpoints.is_empty() || cfg!(unix) {
        for endpoint in endpoints {
            let Ok(response) = crate::ipc::request(&endpoint, json!({"method": "status"})) else {
                prune_endpoint(&endpoint);
                pruned += 1;
                continue;
            };
            let service = response.get("service").cloned().unwrap_or(Value::Null);
            if service.get("active").and_then(Value::as_bool) != Some(true) {
                prune_endpoint(&endpoint);
                pruned += 1;
                continue;
            }
            if let Some(row) = process_row_from_service(&endpoint, service, None) {
                entries.push(row);
            } else {
                prune_endpoint(&endpoint);
                pruned += 1;
            }
        }
    } else {
        let mut kept_registry = Vec::new();
        for entry in registry_entries()? {
            let Ok(response) = crate::ipc::request(&entry.ipc, json!({"method": "status"})) else {
                pruned += 1;
                continue;
            };
            let service = response.get("service").cloned().unwrap_or(Value::Null);
            if service.get("active").and_then(Value::as_bool) != Some(true) {
                pruned += 1;
                continue;
            }
            if let Some(row) = process_row_from_service(&entry.ipc, service, entry.memory_count) {
                kept_registry.push(entry);
                entries.push(row);
            } else {
                pruned += 1;
            }
        }
        if pruned > 0 {
            write_registry(&kept_registry)?;
        }
    }
    entries.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
    let rows: Vec<_> = entries.into_iter().map(|entry| entry.2).collect();
    Ok((rows, pruned))
}

fn process_row_from_service(
    endpoint: &crate::models::IpcEndpoint,
    service: Value,
    fallback_memory_count: Option<usize>,
) -> Option<(String, u32, Value)> {
    let state = service.get("state")?;
    let root = service
        .get("root")
        .cloned()
        .or_else(|| state.get("root").cloned())?;
    let root_text = root.as_str()?.to_string();
    if !Path::new(&root_text).exists() {
        return None;
    }
    let pid = service
        .get("service_pid")
        .and_then(Value::as_u64)
        .map(|value| value as u32)?;
    let workdir = state
        .get("workdir")
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .or_else(|| root.as_str().map(ToString::to_string))
        .unwrap_or_default();
    let mode = state
        .get("install_scope")
        .cloned()
        .unwrap_or_else(|| json!("project"));
    let memory_count = state
        .get("memory_count")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .or(fallback_memory_count);
    let viewer = local_viewer_url_for_root(Path::new(&root_text)).unwrap_or(Value::Null);
    let detail = json!({
        "role": "memory",
        "workdir": workdir,
        "root": root,
        "scope": mode,
        "status": "running",
        "viewer": viewer,
        "memory_count": memory_count,
        "pid": pid,
        "root_hash": crate::ipc::root_hash(Path::new(&root_text)),
        "ipc": endpoint,
        "service": service,
    });
    let workdir_text = detail["workdir"].as_str().unwrap_or_default().to_string();
    Some((workdir_text, pid, detail))
}

fn prune_endpoint(endpoint: &crate::models::IpcEndpoint) {
    if endpoint.name_type == "filesystem" {
        let _ = fs::remove_file(&endpoint.address);
    }
}

fn local_viewer_url_for_root(root: &Path) -> Option<Value> {
    let gateway = gateway_status_value().ok()?;
    if gateway.get("active").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    let config = load_runtime_config(root).ok()?;
    if config.storage.backend != BackendKind::Qdrant {
        return None;
    }
    let endpoint = gateway.get("endpoint").and_then(Value::as_str)?;
    Some(json!(qdrant_viewer_url(endpoint, root)))
}

fn qdrant_viewer_url(gateway_endpoint: &str, root: &Path) -> String {
    format!(
        "{}/view/{}/dashboard",
        gateway_endpoint.trim_end_matches('/'),
        crate::ipc::root_hash(root)
    )
}
