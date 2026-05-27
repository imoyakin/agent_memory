fn cmd_ui_start(root_arg: Option<PathBuf>, host: String, port: u16) -> Result<Value> {
    let root = runtime_root(root_arg.clone())?;
    let config = load_runtime_config(&root)?;
    ensure_backend(&root, &config)?;
    let server = qdrant_server_status(&root, &config.storage.qdrant)?;
    if server.get("dashboard_available").and_then(Value::as_bool) != Some(true) {
        bail!(
            "Qdrant API is reachable at {}, but the official /dashboard UI is not available. Install Qdrant Web UI static files with scripts/install-agent-memory.sh or set storage.qdrant.static_content_dir.",
            config.storage.qdrant.uri
        );
    }
    let gateway = cmd_gateway_start(15, 5, host.clone(), port, false)?;
    let gateway_endpoint = gateway
        .pointer("/gateway/endpoint")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("gateway did not return an endpoint"))?;
    let viewer_url = qdrant_viewer_url(gateway_endpoint, &root);
    register_ui_viewer(
        &root,
        &json!({
            "pid": server.get("pid").cloned().unwrap_or(Value::Null),
            "host": host,
            "port": port,
            "endpoint": viewer_url.clone(),
            "qdrant_endpoint": config.storage.qdrant.uri,
            "data_dir": server.get("storage_path").cloned().unwrap_or(Value::Null),
            "database_name": config.collection_name,
            "root_hash": crate::ipc::root_hash(&root),
        }),
    )?;
    let projects = gateway_projects()?;
    Ok(json!({
        "ok": true,
        "server": server,
        "gateway": gateway.get("gateway").cloned().unwrap_or(Value::Null),
        "projects": projects,
        "ui": {
            "viewer": "qdrant-dashboard",
            "address": viewer_url,
            "qdrant_endpoint": config.storage.qdrant.uri,
            "dashboard_path": "/dashboard"
        }
    }))
}

fn cmd_ui_status(root_arg: Option<PathBuf>) -> Result<Value> {
    let root = runtime_root(root_arg.clone())?;
    let config = load_runtime_config(&root)?;
    let gateway = gateway_status_value()?;
    Ok(json!({
        "ok": true,
        "server": qdrant_server_status(&root, &config.storage.qdrant)?,
        "gateway": gateway,
        "ui": {
            "viewer": "qdrant-dashboard",
            "address": gateway
                .get("endpoint")
                .and_then(Value::as_str)
                .map(|endpoint| qdrant_viewer_url(endpoint, &root)),
            "qdrant_endpoint": config.storage.qdrant.uri
        }
    }))
}

fn cmd_ui_stop(root_arg: Option<PathBuf>, timeout_seconds: u64) -> Result<Value> {
    let root = runtime_root(root_arg.clone())?;
    let config = load_runtime_config(&root)?;
    let stopped = cmd_gateway_stop(timeout_seconds)?;
    unregister_ui_viewer(&root)?;
    Ok(json!({
        "ok": true,
        "stopped": stopped.get("stopped").cloned().unwrap_or(Value::Null),
        "gateway_stop": stopped,
        "server": qdrant_server_status(&root, &config.storage.qdrant)?,
    }))
}
