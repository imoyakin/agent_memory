pub(crate) fn ensure_qdrant_server(root: &Path, config: &QdrantConfig) -> Result<()> {
    let storage_path = resolve_under_root(root, &config.storage_path);
    let static_content_dir = qdrant_static_content_dir(config).filter(|path| path.is_dir());
    if qdrant_get(config, "/collections").is_ok() {
        if let Some(state) = read_qdrant_server_state(root)? {
            let pid = state.get("pid").and_then(Value::as_u64).unwrap_or(0) as u32;
            let alive = crate::service::pid_exists(pid);
            let supervised = alive && qdrant_process_is_supervised(root, pid);
            if alive && !supervised {
                if !qdrant_start_allowed() {
                    bail!(
                        "Qdrant at {} is not parented by an agent-memory service for {}",
                        config.uri,
                        root.display()
                    );
                }
                terminate_qdrant_process(pid)?;
                wait_for_qdrant_stop(config)?;
                wait_for_qdrant_process_exit(pid)?;
            } else if alive && qdrant_state_requires_restart(&storage_path, config, &state) {
                if !qdrant_start_allowed() {
                    bail!(
                        "Qdrant at {} needs restart for {}, but this command is not the service supervisor",
                        config.uri,
                        root.display()
                    );
                }
                terminate_qdrant_process(pid)?;
                wait_for_qdrant_stop(config)?;
                wait_for_qdrant_process_exit(pid)?;
            } else if supervised || qdrant_dashboard_available(config) || static_content_dir.is_none()
            {
                return Ok(());
            } else {
                if !qdrant_start_allowed() {
                    return Ok(());
                }
                terminate_qdrant_process(pid)?;
                wait_for_qdrant_stop(config)?;
                wait_for_qdrant_process_exit(pid)?;
            }
        } else {
            bail!(
                "Qdrant at {} is reachable but is not owned by this agent-memory root; stop it or configure a different storage.qdrant.uri",
                config.uri
            );
        }
    }
    if let Some(state) = read_qdrant_server_state(root)? {
        let pid = state.get("pid").and_then(Value::as_u64).unwrap_or(0) as u32;
        if crate::service::pid_exists(pid) {
            bail!(
                "Qdrant pid {pid} is recorded but {} is not reachable",
                config.uri
            );
        }
    }
    if !qdrant_start_allowed() {
        bail!(
            "Qdrant is not running under an agent-memory service for {}; run `agent-memory init --start-service` or `agent-memory service start`",
            root.display()
        );
    }

    fs::create_dir_all(&storage_path)?;
    let log_path = project_path(root, ProjectPath::QdrantServerLog);
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let stdout = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;
    let stderr = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;
    let (host, port) = qdrant_host_port(&config.uri)?;
    let grpc_port = port
        .checked_add(1)
        .ok_or_else(|| anyhow!("Qdrant uri port must leave room for a gRPC port: {port}"))?;
    let mut command = Command::new(&config.binary);
    command
        .env("QDRANT__SERVICE__HOST", &host)
        .env("QDRANT__SERVICE__HTTP_PORT", port.to_string())
        .env("QDRANT__SERVICE__GRPC_PORT", grpc_port.to_string())
        .env("QDRANT__SERVICE__ENABLE_STATIC_CONTENT", "true")
        .env("QDRANT__STORAGE__STORAGE_PATH", &storage_path)
        .current_dir(&storage_path)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    if let Some(static_content_dir) = &static_content_dir {
        command.env("QDRANT__SERVICE__STATIC_CONTENT_DIR", static_content_dir);
    }
    let mut child = command
        .spawn()
        .with_context(|| format!("failed to start Qdrant binary '{}'", config.binary))?;
    let started_at = crate::util::now();
    let state = json!({
        "pid": child.id(),
        "parent_pid": std::process::id(),
        "uri": config.uri,
        "endpoint": config.uri,
        "storage_path": storage_path,
        "binary": config.binary,
        "static_content_dir": static_content_dir,
        "log_path": log_path,
        "started_at": started_at,
        "updated_at": started_at,
    });
    write_qdrant_server_state(root, &state)?;
    wait_for_qdrant_start(&mut child, config, &log_path)?;
    Ok(())
}

pub(crate) fn enable_qdrant_supervisor_context() {
    std::env::set_var("AGENT_MEMORY_QDRANT_SUPERVISOR", "service");
}

pub(crate) fn qdrant_server_status(root: &Path, config: &QdrantConfig) -> Result<Value> {
    let reachable = qdrant_get(config, "/collections").is_ok();
    let static_content_dir = qdrant_static_content_dir(config);
    let static_content_dir_available = static_content_dir
        .as_ref()
        .map(|path| path.is_dir())
        .unwrap_or(false);
    let state = read_qdrant_server_state(root)?;
    let pid = state
        .as_ref()
        .and_then(|value| value.get("pid"))
        .and_then(Value::as_u64)
        .unwrap_or(0) as u32;
    Ok(json!({
        "root": root,
        "active": reachable,
        "pid": if pid == 0 { Value::Null } else { json!(pid) },
        "uri": config.uri,
        "endpoint": config.uri,
        "dashboard_available": qdrant_dashboard_available(config),
        "static_content_dir": static_content_dir,
        "static_content_dir_available": static_content_dir_available,
        "storage_path": resolve_under_root(root, &config.storage_path),
        "state_path": project_path(root, ProjectPath::QdrantServerState),
        "log_path": project_path(root, ProjectPath::QdrantServerLog),
        "state": state,
    }))
}

fn qdrant_state_requires_restart(
    expected_storage_path: &Path,
    config: &QdrantConfig,
    state: &Value,
) -> bool {
    let storage_matches = state
        .get("storage_path")
        .and_then(Value::as_str)
        .map(Path::new)
        .map(|path| path == expected_storage_path)
        .unwrap_or(false);
    let binary_matches = state
        .get("binary")
        .and_then(Value::as_str)
        .map(|binary| binary == config.binary)
        .unwrap_or(false);
    !(storage_matches && binary_matches)
}

fn qdrant_process_is_supervised(root: &Path, qdrant_pid: u32) -> bool {
    let Some(parent_pid) = crate::service::process_parent_pid(qdrant_pid) else {
        return false;
    };
    if parent_pid == std::process::id() {
        return true;
    }
    crate::service::read_service_state(root)
        .ok()
        .flatten()
        .filter(|state| state.service_pid == parent_pid)
        .map(|state| crate::service::pid_exists(state.service_pid))
        .unwrap_or(false)
}

fn qdrant_start_allowed() -> bool {
    qdrant_start_allowed_from_env(std::env::var("AGENT_MEMORY_QDRANT_SUPERVISOR").ok().as_deref())
}

fn qdrant_start_allowed_from_env(value: Option<&str>) -> bool {
    value == Some("service")
}

fn wait_for_qdrant_start(
    child: &mut std::process::Child,
    config: &QdrantConfig,
    log_path: &Path,
) -> Result<()> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        if qdrant_get(config, "/collections").is_ok() {
            return Ok(());
        }
        if let Some(status) = child.try_wait()? {
            bail!(
                "Qdrant exited before becoming reachable ({status}); see {}",
                log_path.display()
            );
        }
        if std::time::Instant::now() >= deadline {
            bail!(
                "timed out waiting for Qdrant at {}; see {}",
                config.uri,
                log_path.display()
            );
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

fn wait_for_qdrant_stop(config: &QdrantConfig) -> Result<()> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while qdrant_get(config, "/collections").is_ok() {
        if std::time::Instant::now() >= deadline {
            bail!("timed out waiting for Qdrant at {} to stop", config.uri);
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    Ok(())
}

fn wait_for_qdrant_process_exit(pid: u32) -> Result<()> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while crate::service::pid_exists(pid) {
        if std::time::Instant::now() >= deadline {
            bail!("timed out waiting for Qdrant pid {pid} to exit");
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    Ok(())
}

pub(crate) fn qdrant_dashboard_available(config: &QdrantConfig) -> bool {
    let url = format!("{}/dashboard", config.uri.trim_end_matches('/'));
    reqwest::blocking::Client::new()
        .get(url)
        .send()
        .map(|response| response.status().is_success())
        .unwrap_or(false)
}

pub(crate) fn qdrant_static_content_dir(config: &QdrantConfig) -> Option<PathBuf> {
    if let Some(path) = config
        .static_content_dir
        .as_deref()
        .map(str::trim)
        .filter(|path| !path.is_empty())
    {
        return Some(PathBuf::from(path));
    }
    let binary = Path::new(&config.binary);
    if let Some(parent) = binary
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        let sibling = parent.join("qdrant-static");
        if sibling.is_dir() {
            return Some(sibling);
        }
        let legacy_sibling = parent.join("static");
        if legacy_sibling.is_dir() {
            return Some(legacy_sibling);
        }
    }
    let skill_static = skill_root().ok()?.join("bin").join("qdrant-static");
    Some(skill_static)
}

fn terminate_qdrant_process(pid: u32) -> Result<()> {
    #[cfg(unix)]
    {
        let group_target = format!("-{pid}");
        if Command::new("kill")
            .arg("-TERM")
            .arg(&group_target)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
        {
            return Ok(());
        }
        let status = Command::new("kill")
            .arg("-TERM")
            .arg(pid.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
        if !status.success() {
            bail!("failed to terminate Qdrant pid {pid}");
        }
    }
    #[cfg(windows)]
    {
        let status = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
        if !status.success() {
            bail!("failed to terminate Qdrant pid {pid}");
        }
    }
    Ok(())
}

fn ensure_qdrant_collection(config: &RuntimeConfig) -> Result<()> {
    let collection_path = format!("/collections/{}", active_collection_name(config));
    if qdrant_get(&config.storage.qdrant, &collection_path).is_ok() {
        return Ok(());
    }
    let payload = json!({
        "vectors": {
            "size": config.embedding_dim,
            "distance": "Cosine"
        }
    });
    if let Err(error) = qdrant_put(&config.storage.qdrant, &collection_path, payload) {
        if !error.to_string().contains("already exists") {
            return Err(error);
        }
    }
    Ok(())
}
