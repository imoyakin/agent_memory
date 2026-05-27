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

fn milvus_lite_server_command(
    data_dir: &Path,
    host: &str,
    port: u16,
    max_workers: u16,
) -> Result<Command> {
    let mut command = Command::new("uv");
    command
        .arg("run")
        .arg("--project")
        .arg(skill_root()?)
        .arg("milvus-lite")
        .arg("server")
        .arg("--data-dir")
        .arg(data_dir)
        .arg("--host")
        .arg(host)
        .arg("--port")
        .arg(port.to_string())
        .arg("--max-workers")
        .arg(max_workers.max(1).to_string());
    Ok(command)
}

fn wait_for_milvus_lite_server_start(
    child: &mut Child,
    host: &str,
    port: u16,
    timeout: Duration,
    log_path: &Path,
) -> Result<()> {
    let deadline = Instant::now() + timeout;
    loop {
        if TcpStream::connect((attu_connect_host(host), port)).is_ok() {
            return Ok(());
        }
        if let Some(status) = child.try_wait()? {
            bail!(
                "Milvus Lite server exited before becoming reachable ({status}); see {}",
                log_path.display()
            );
        }
        if Instant::now() >= deadline {
            bail!(
                "timed out waiting for Milvus Lite server on {}:{}; see {}",
                attu_connect_host(host),
                port,
                log_path.display()
            );
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn milvus_lite_server_status_value(root: &Path) -> Result<Value> {
    let state = read_milvus_lite_server_state(root)?;
    let pid = state
        .as_ref()
        .and_then(|value| value.get("pid"))
        .and_then(Value::as_u64)
        .unwrap_or(0) as u32;
    let active = pid_exists(pid);
    Ok(json!({
        "root": root,
        "state_path": project_path(root, ProjectPath::MilvusLiteServerState),
        "active": active,
        "pid": if pid == 0 { Value::Null } else { json!(pid) },
        "endpoint": state.as_ref().and_then(|value| value.get("endpoint")).cloned().unwrap_or(Value::Null),
        "data_dir": state.as_ref().and_then(|value| value.get("data_dir")).cloned().unwrap_or(Value::Null),
        "database_name": state.as_ref().and_then(|value| value.get("database_name")).cloned().unwrap_or(Value::Null),
        "log_path": project_path(root, ProjectPath::MilvusLiteServerLog),
        "state": state,
    }))
}

fn stop_other_ui_viewers(
    root: &Path,
    host: &str,
    port: u16,
    timeout_seconds: u64,
) -> Result<Vec<Value>> {
    let mut closed = Vec::new();
    let mut kept = Vec::new();
    for entry in read_ui_viewer_registry()? {
        let entry_root = entry
            .get("root")
            .and_then(Value::as_str)
            .map(PathBuf::from)
            .unwrap_or_default();
        let entry_pid = entry.get("pid").and_then(Value::as_u64).unwrap_or(0) as u32;
        let entry_port = entry.get("port").and_then(Value::as_u64).unwrap_or(0) as u16;
        let entry_host = entry
            .get("host")
            .and_then(Value::as_str)
            .unwrap_or("127.0.0.1");
        let same_port =
            entry_port == port && attu_connect_host(entry_host) == attu_connect_host(host);
        if entry_root == root || !same_port || !pid_exists(entry_pid) {
            if pid_exists(entry_pid) {
                kept.push(entry);
            }
            continue;
        }
        terminate_process_group(entry_pid)?;
        let deadline = Instant::now() + Duration::from_secs(timeout_seconds.max(1));
        while pid_exists(entry_pid) {
            if Instant::now() >= deadline {
                bail!("timed out stopping existing UI viewer pid {entry_pid}");
            }
            thread::sleep(Duration::from_millis(100));
        }
        if let Some(mut state) = read_milvus_lite_server_state(&entry_root)? {
            state["pid"] = json!(0);
            state["updated_at"] = json!(now());
            state["stopped_at"] = json!(now());
            write_milvus_lite_server_state(&entry_root, &state)?;
        }
        closed.push(json!({
            "root": entry_root,
            "pid": entry_pid,
            "port": entry_port,
            "database_name": entry.get("database_name").cloned().unwrap_or(Value::Null),
        }));
    }
    write_ui_viewer_registry(&kept)?;
    Ok(closed)
}

fn register_ui_viewer(root: &Path, state: &Value) -> Result<()> {
    let mut entries = read_ui_viewer_registry()?;
    entries.retain(|entry| entry.get("root").and_then(Value::as_str).map(Path::new) != Some(root));
    entries.push(json!({
        "root": root,
        "pid": state.get("pid").cloned().unwrap_or(Value::Null),
        "host": state.get("host").cloned().unwrap_or(Value::Null),
        "port": state.get("port").cloned().unwrap_or(Value::Null),
        "endpoint": state.get("endpoint").cloned().unwrap_or(Value::Null),
        "qdrant_endpoint": state.get("qdrant_endpoint").cloned().unwrap_or(Value::Null),
        "data_dir": state.get("data_dir").cloned().unwrap_or(Value::Null),
        "database_name": state.get("database_name").cloned().unwrap_or(Value::Null),
        "root_hash": state.get("root_hash").cloned().unwrap_or_else(|| json!(crate::ipc::root_hash(root))),
        "updated_at": now(),
    }));
    write_ui_viewer_registry(&entries)
}

fn unregister_ui_viewer(root: &Path) -> Result<()> {
    let mut entries = read_ui_viewer_registry()?;
    entries.retain(|entry| entry.get("root").and_then(Value::as_str).map(Path::new) != Some(root));
    write_ui_viewer_registry(&entries)
}

fn read_ui_viewer_registry() -> Result<Vec<Value>> {
    let path = home_path(HomePath::UiViewerRegistry);
    if !path.exists() {
        return Ok(Vec::new());
    }
    Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
}

fn write_ui_viewer_registry(entries: &[Value]) -> Result<()> {
    let path = home_path(HomePath::UiViewerRegistry);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string_pretty(entries)? + "\n")?;
    Ok(())
}

fn milvus_lite_display_name(root: &Path, data_dir: &Path) -> String {
    let project = root
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("global");
    let db = data_dir
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("memory");
    format!("{project} ({db})")
}

fn read_milvus_lite_server_state(root: &Path) -> Result<Option<Value>> {
    let path = project_path(root, ProjectPath::MilvusLiteServerState);
    if !path.exists() {
        return Ok(None);
    }
    Ok(Some(serde_json::from_str(&fs::read_to_string(path)?)?))
}

fn write_milvus_lite_server_state(root: &Path, state: &Value) -> Result<()> {
    fs::create_dir_all(project_path(root, ProjectPath::MemoryDir))?;
    fs::write(
        project_path(root, ProjectPath::MilvusLiteServerState),
        serde_json::to_string_pretty(state)? + "\n",
    )?;
    Ok(())
}

fn attu_connect_host(host: &str) -> &str {
    match host {
        "0.0.0.0" => "127.0.0.1",
        "::" => "::1",
        value => value,
    }
}

fn terminate_process_group(pid: u32) -> Result<()> {
    let group_target = format!("-{pid}");
    let status = Command::new("kill")
        .arg("-TERM")
        .arg(&group_target)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    if status.map(|status| status.success()).unwrap_or(false) {
        return Ok(());
    }
    let status = Command::new("kill")
        .arg("-TERM")
        .arg(pid.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    if !status.success() {
        bail!("failed to terminate Milvus Lite server pid {pid}");
    }
    Ok(())
}
