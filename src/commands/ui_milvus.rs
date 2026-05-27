fn cmd_milvus_lite_server(
    root_arg: Option<PathBuf>,
    host: String,
    port: u16,
    max_workers: u16,
    stop_service: bool,
    timeout_seconds: u64,
) -> Result<Value> {
    let root = runtime_root(root_arg)?;
    let config = load_runtime_config(&root)?;
    if config.storage.backend != BackendKind::MilvusLite {
        bail!(
            "milvus-lite-server only applies to milvus_lite storage; current backend is {}",
            config.storage.backend
        );
    }

    let current = milvus_lite_server_status_value(&root)?;
    if current.get("active").and_then(Value::as_bool) == Some(true) {
        return Ok(json!({"ok": true, "already_running": true, "server": current}));
    }

    let active_service = service_status(&root)?
        .get("active")
        .and_then(Value::as_bool)
        == Some(true);
    let service_stop = if active_service {
        if !stop_service {
            bail!(
                "agent-memory service is active for {}; stop it first or rerun with --stop-service because Milvus Lite allows one writer/server for the same data directory",
                root.display()
            );
        }
        Some(request_service_stop(
            &root,
            Duration::from_secs(timeout_seconds.max(1)),
        )?)
    } else {
        None
    };

    ensure_backend(&root, &config, false)?;
    fs::create_dir_all(project_path(&root, ProjectPath::MemoryDir))?;
    let data_dir = resolve_under_root(&root, &config.storage.milvus_lite.db_path);
    let log_path = project_path(&root, ProjectPath::MilvusLiteServerLog);
    let closed_viewers = stop_other_ui_viewers(&root, &host, port, timeout_seconds)?;
    if TcpStream::connect((attu_connect_host(&host), port)).is_ok() {
        bail!(
            "port {}:{} is already accepting connections; choose another --port or stop the existing Milvus server",
            attu_connect_host(&host),
            port
        );
    }
    let stdout = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;
    let stderr = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;

    let mut command = milvus_lite_server_command(&data_dir, &host, port, max_workers)?;
    command
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    detach_daemon(&mut command);

    let mut child = command.spawn()?;
    let pid = child.id();
    let endpoint = format!("http://{}:{port}", attu_connect_host(&host));
    let started_at = now();
    let state = json!({
        "pid": pid,
        "host": host,
        "port": port,
        "endpoint": endpoint,
        "data_dir": data_dir,
        "database_name": milvus_lite_display_name(&root, &data_dir),
        "log_path": log_path,
        "started_at": started_at,
        "updated_at": started_at,
    });
    write_milvus_lite_server_state(&root, &state)?;
    register_ui_viewer(&root, &state)?;
    wait_for_milvus_lite_server_start(
        &mut child,
        &host,
        port,
        Duration::from_secs(timeout_seconds.max(1)),
        &log_path,
    )?;

    Ok(json!({
        "ok": true,
        "server": milvus_lite_server_status_value(&root)?,
        "service_stop": service_stop,
        "closed_viewers": closed_viewers,
        "attu": {
            "address": endpoint,
            "token": null,
            "recommended": true,
            "project_url": "https://github.com/zilliztech/attu"
        }
    }))
}
