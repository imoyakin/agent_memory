fn gateway_projects() -> Result<Vec<Value>> {
    gateway_projects_with_options(false)
}

fn gateway_projects_with_options(local_only: bool) -> Result<Vec<Value>> {
    let viewers = read_ui_viewer_registry()?;
    let (processes, _) = memory_processes()?;
    let gateway = gateway_status_value()?;
    let gateway_endpoint = gateway.get("endpoint").and_then(Value::as_str);
    let mut entries = Vec::new();
    let mut seen_roots = HashSet::new();
    for process in processes {
        let root = process
            .get("root")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let viewer = active_viewer_for_root(&viewers, &root);
        let viewer_active = viewer
            .as_ref()
            .and_then(|item| item.get("pid"))
            .and_then(Value::as_u64)
            .map(|pid| pid_exists(pid as u32))
            .unwrap_or(false);
        entries.push(json!({
            "workdir": process.get("workdir").cloned().unwrap_or(Value::Null),
            "root": process.get("root").cloned().unwrap_or(Value::Null),
            "scope": process.get("scope").cloned().unwrap_or(Value::Null),
            "status": "running",
            "memory_count": process.get("memory_count").cloned().unwrap_or(Value::Null),
            "pid": process.get("pid").cloned().unwrap_or(Value::Null),
            "root_hash": process.get("root_hash").cloned().unwrap_or(Value::Null),
            "ipc": process.get("ipc").cloned().unwrap_or(Value::Null),
            "service": process.get("service").cloned().unwrap_or(Value::Null),
            "viewer_active": viewer_active,
            "viewer_endpoint": local_project_viewer_endpoint(
                Path::new(&root),
                gateway_endpoint,
                viewer.as_ref(),
            ),
            "qdrant_endpoint": qdrant_endpoint_for_root(Path::new(&root)).unwrap_or(Value::Null),
            "viewer_database": viewer.as_ref().and_then(|item| item.get("database_name")).cloned().unwrap_or(Value::Null),
        }));
        seen_roots.insert(root);
    }
    for viewer in viewers {
        let root = viewer
            .get("root")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if root.is_empty() || seen_roots.contains(&root) {
            continue;
        }
        let viewer_pid = viewer.get("pid").and_then(Value::as_u64).unwrap_or(0) as u32;
        if !pid_exists(viewer_pid) {
            continue;
        }
        entries.push(json!({
            "workdir": root.clone(),
            "root": root.clone(),
            "scope": "project",
            "status": "viewer",
            "memory_count": Value::Null,
            "pid": Value::Null,
            "ipc": Value::Null,
            "service": Value::Null,
            "root_hash": crate::ipc::root_hash(Path::new(&root)),
            "viewer_active": true,
            "viewer_endpoint": local_project_viewer_endpoint(
                Path::new(&root),
                gateway_endpoint,
                Some(&viewer),
            ),
            "qdrant_endpoint": qdrant_endpoint_for_root(Path::new(&root)).unwrap_or(Value::Null),
            "viewer_database": viewer.get("database_name").cloned().unwrap_or(Value::Null),
        }));
    }
    if !local_only {
        for remote in read_gateway_remotes()? {
            if let Ok(projects) = remote_gateway_projects(&remote, true) {
                entries.extend(rewrite_remote_projects(
                    &remote.name,
                    &projects,
                    gateway_endpoint,
                ));
            }
        }
    }
    entries.sort_by(|left, right| value_text(&left["workdir"]).cmp(&value_text(&right["workdir"])));
    Ok(entries)
}

fn local_project_viewer_endpoint(
    root: &Path,
    gateway_endpoint: Option<&str>,
    fallback_viewer: Option<&Value>,
) -> Value {
    if qdrant_endpoint_for_root(root).is_some() {
        if let Some(endpoint) = gateway_endpoint {
            return json!(qdrant_viewer_url(endpoint, root));
        }
    }
    fallback_viewer
        .and_then(|item| item.get("endpoint"))
        .cloned()
        .unwrap_or(Value::Null)
}

fn qdrant_endpoint_for_root(root: &Path) -> Option<Value> {
    let config = load_runtime_config(root).ok()?;
    Some(json!(config.storage.qdrant.uri))
}

fn active_viewer_for_root(viewers: &[Value], root: &str) -> Option<Value> {
    viewers
        .iter()
        .find(|viewer| viewer.get("root").and_then(Value::as_str) == Some(root))
        .cloned()
}

fn gateway_status_value() -> Result<Value> {
    let state = read_gateway_state()?;
    let active = state.as_ref().map(gateway_state_active).unwrap_or(false);
    let endpoint = state.as_ref().and_then(|item| {
        item.host
            .as_ref()
            .zip(item.port)
            .map(|(host, port)| format!("http://{host}:{port}"))
    });
    let endpoint = if active { endpoint } else { None };
    let token = if active {
        state.as_ref().and_then(|item| item.token.clone())
    } else {
        None
    };
    Ok(json!({
        "active": active,
        "pid": state.as_ref().map(|item| item.pid).filter(|pid| *pid > 0),
        "endpoint": endpoint.clone(),
        "host": state.as_ref().and_then(|item| item.host.clone()),
        "port": state.as_ref().and_then(|item| item.port),
        "token": token.clone(),
        "attach": gateway_attach_info(endpoint.as_deref(), token.as_deref()),
        "state_path": home_path(HomePath::UiGatewayState),
        "lock_path": home_path(HomePath::UiGatewayLock),
        "log_path": home_path(HomePath::UiGatewayLog),
        "started_at": state.as_ref().map(|item| item.started_at.clone()),
        "updated_at": state.as_ref().map(|item| item.updated_at.clone()),
        "lease_expires_at": state.as_ref().map(|item| item.lease_expires_at.clone()),
        "lease_seconds": state.as_ref().map(|item| item.lease_seconds),
        "heartbeat_seconds": state.as_ref().map(|item| item.heartbeat_seconds),
        "state": state,
    }))
}

fn gateway_state_active(state: &GatewayState) -> bool {
    state.pid > 0
        && pid_exists(state.pid)
        && state.stop_requested_at.is_none()
        && parse_time(&state.lease_expires_at)
            .map(|expires| expires > Utc::now())
            .unwrap_or(false)
}

fn gateway_attach_info(endpoint: Option<&str>, token: Option<&str>) -> Value {
    let Some(endpoint) = endpoint else {
        return Value::Null;
    };
    let parsed = reqwest::Url::parse(endpoint).ok();
    let port = parsed
        .as_ref()
        .and_then(|url| url.port_or_known_default())
        .unwrap_or(19531);
    let cwd = std::env::current_dir().ok();
    let scope = cwd
        .as_ref()
        .and_then(|path| discover(Some(path)).ok().flatten())
        .map(|discovered| {
            load_user_config(&discovered.config_path)
                .map(|config| config.install_scope)
                .unwrap_or_else(|_| "project".to_string())
        });
    json!({
        "name": local_host_name(),
        "url": endpoint,
        "token": token,
        "ssh_forward_hint": format!("ssh -L {port}:127.0.0.1:{port} user@host"),
        "workdir": cwd,
        "scope": scope,
    })
}

fn local_host_name() -> String {
    std::env::var("HOSTNAME")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            Command::new("hostname")
                .output()
                .ok()
                .filter(|output| output.status.success())
                .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
                .filter(|value| !value.is_empty())
        })
        .unwrap_or_else(|| "local".to_string())
}

fn clear_stale_gateway_lock() -> Result<()> {
    if let Some(state) = read_gateway_state()? {
        if gateway_state_active(&state) {
            return Ok(());
        }
    }
    let lock_path = home_path(HomePath::UiGatewayLock);
    if lock_path.exists() {
        let _ = fs::remove_file(lock_path);
    }
    Ok(())
}

fn mark_gateway_stopped(state: &mut GatewayState) -> Result<()> {
    state.pid = 0;
    state.updated_at = now();
    state.stopped_at = Some(now());
    write_gateway_state(state)?;
    let _ = fs::remove_file(home_path(HomePath::UiGatewayLock));
    Ok(())
}

fn read_gateway_state() -> Result<Option<GatewayState>> {
    let path = home_path(HomePath::UiGatewayState);
    if !path.exists() {
        return Ok(None);
    }
    Ok(Some(serde_json::from_str(&fs::read_to_string(path)?)?))
}

fn write_gateway_state(state: &GatewayState) -> Result<()> {
    let path = home_path(HomePath::UiGatewayState);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string_pretty(state)? + "\n")?;
    Ok(())
}

fn lease_deadline(lease_seconds: u64) -> String {
    (Utc::now() + ChronoDuration::seconds(lease_seconds as i64)).to_rfc3339()
}

fn parse_time(raw: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|value| value.with_timezone(&Utc))
}
