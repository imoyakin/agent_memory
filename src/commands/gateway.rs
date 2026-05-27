fn cmd_gateway_start(
    lease_seconds: u64,
    heartbeat_seconds: u64,
    host: String,
    port: u16,
    foreground: bool,
) -> Result<Value> {
    let lease_seconds = lease_seconds.max(3);
    let heartbeat_seconds = heartbeat_seconds.max(1).min(lease_seconds);
    if foreground {
        return run_gateway_loop(lease_seconds, heartbeat_seconds, host, port);
    }
    let current = gateway_status_value()?;
    if current.get("active").and_then(Value::as_bool) == Some(true) {
        return Ok(
            json!({"ok": true, "already_running": true, "gateway": current, "projects": gateway_projects()?}),
        );
    }
    clear_stale_gateway_lock()?;
    let log_path = home_path(HomePath::UiGatewayLog);
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let stdout = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;
    let stderr = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;
    let mut command = Command::new(std::env::current_exe()?);
    command
        .arg("gateway")
        .arg("start")
        .arg("--foreground")
        .arg("--lease-seconds")
        .arg(lease_seconds.to_string())
        .arg("--heartbeat-seconds")
        .arg(heartbeat_seconds.to_string())
        .arg("--host")
        .arg(&host)
        .arg("--port")
        .arg(port.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    detach_daemon(&mut command);
    let mut child = command.spawn()?;
    let gateway = wait_for_gateway_start(&mut child, Duration::from_secs(5), &log_path)?;
    Ok(json!({"ok": true, "gateway": gateway, "projects": gateway_projects()?}))
}

fn cmd_gateway_status() -> Result<Value> {
    Ok(json!({"ok": true, "gateway": gateway_status_value()?, "projects": gateway_projects()?}))
}

fn cmd_gateway_projects() -> Result<Value> {
    Ok(json!({"ok": true, "gateway": gateway_status_value()?, "projects": gateway_projects()?}))
}

fn cmd_gateway_attach(name: String, url: String, token: String) -> Result<Value> {
    let name = sanitize_remote_alias(&name)?;
    let mut remotes = read_gateway_remotes()?;
    remotes.retain(|remote| remote.name != name);
    let timestamp = now();
    remotes.push(GatewayRemote {
        name,
        url: url.trim_end_matches('/').to_string(),
        token,
        created_at: timestamp.clone(),
        updated_at: timestamp,
    });
    write_gateway_remotes(&remotes)?;
    Ok(
        json!({"ok": true, "remotes": gateway_remote_statuses(gateway_status_value()?.get("endpoint").and_then(Value::as_str))?}),
    )
}

fn cmd_gateway_detach(name: String) -> Result<Value> {
    let name = sanitize_remote_alias(&name)?;
    let mut remotes = read_gateway_remotes()?;
    let before = remotes.len();
    remotes.retain(|remote| remote.name != name);
    write_gateway_remotes(&remotes)?;
    Ok(json!({
        "ok": true,
        "detached": before != remotes.len(),
        "remotes": gateway_remote_statuses(gateway_status_value()?.get("endpoint").and_then(Value::as_str))?
    }))
}

fn cmd_gateway_remotes() -> Result<Value> {
    Ok(json!({
        "ok": true,
        "remotes": gateway_remote_statuses(gateway_status_value()?.get("endpoint").and_then(Value::as_str))?
    }))
}

fn cmd_gateway_stop(timeout_seconds: u64) -> Result<Value> {
    let Some(mut state) = read_gateway_state()? else {
        return Ok(json!({"ok": true, "stopped": false, "gateway": gateway_status_value()?}));
    };
    if !gateway_state_active(&state) {
        mark_gateway_stopped(&mut state)?;
        return Ok(json!({"ok": true, "stopped": true, "gateway": gateway_status_value()?}));
    }
    state.stop_requested_at = Some(now());
    state.updated_at = now();
    write_gateway_state(&state)?;
    let deadline = Instant::now() + Duration::from_secs(timeout_seconds.max(1));
    while Instant::now() < deadline {
        let Some(mut current) = read_gateway_state()? else {
            return Ok(json!({"ok": true, "stopped": true, "gateway": gateway_status_value()?}));
        };
        if current.pid == 0 || !pid_exists(current.pid) {
            mark_gateway_stopped(&mut current)?;
            return Ok(json!({"ok": true, "stopped": true, "gateway": gateway_status_value()?}));
        }
        thread::sleep(Duration::from_millis(100));
    }
    bail!(
        "timed out waiting for UI gateway pid {} to stop; state at {}",
        state.pid,
        home_path(HomePath::UiGatewayState).display()
    )
}

fn run_gateway_loop(
    lease_seconds: u64,
    heartbeat_seconds: u64,
    host: String,
    port: u16,
) -> Result<Value> {
    clear_stale_gateway_lock()?;
    let lock_path = home_path(HomePath::UiGatewayLock);
    if let Some(parent) = lock_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut lock = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock_path)?;
    writeln!(lock, "{}", std::process::id())?;

    let started_at = now();
    let mut state = GatewayState {
        pid: std::process::id(),
        started_at: started_at.clone(),
        updated_at: started_at,
        lease_expires_at: lease_deadline(lease_seconds),
        lease_seconds,
        heartbeat_seconds,
        host: Some(host.clone()),
        port: Some(port),
        token: Some(Uuid::new_v4().to_string()),
        stopped_at: None,
        stop_requested_at: None,
    };
    write_gateway_state(&state)?;
    start_gateway_http(host, port)?;

    'gateway: loop {
        let heartbeat_deadline = Instant::now() + Duration::from_secs(heartbeat_seconds);
        while Instant::now() < heartbeat_deadline {
            thread::sleep(Duration::from_millis(200));
            let Some(current) = read_gateway_state()? else {
                break 'gateway;
            };
            if current.pid != std::process::id() || current.stop_requested_at.is_some() {
                break 'gateway;
            }
        }
        let Some(current) = read_gateway_state()? else {
            break;
        };
        if current.pid != std::process::id() || current.stop_requested_at.is_some() {
            break;
        }
        state.updated_at = now();
        state.lease_expires_at = lease_deadline(lease_seconds);
        write_gateway_state(&state)?;
    }

    if let Some(mut current) = read_gateway_state()? {
        if current.pid == std::process::id() {
            mark_gateway_stopped(&mut current)?;
        }
    }
    let _ = fs::remove_file(lock_path);
    Ok(json!({"ok": true, "gateway": gateway_status_value()?}))
}

fn wait_for_gateway_start(child: &mut Child, timeout: Duration, log_path: &Path) -> Result<Value> {
    let deadline = Instant::now() + timeout;
    loop {
        let status = gateway_status_value()?;
        if status.get("active").and_then(Value::as_bool) == Some(true) {
            return Ok(status);
        }
        if let Some(exit_status) = child.try_wait()? {
            bail!(
                "UI gateway exited before becoming active ({exit_status}); see {}",
                log_path.display()
            );
        }
        if Instant::now() >= deadline {
            bail!(
                "timed out waiting for UI gateway to start; see {}",
                log_path.display()
            );
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn start_gateway_http(host: String, port: u16) -> Result<()> {
    let listener = TcpListener::bind((host.as_str(), port))?;
    listener.set_nonblocking(true)?;
    thread::spawn(move || loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                let _ = stream.set_nonblocking(false);
                let _ = handle_gateway_http_stream(&mut stream);
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                if !read_gateway_state()
                    .ok()
                    .flatten()
                    .map(|state| {
                        state.pid == std::process::id() && state.stop_requested_at.is_none()
                    })
                    .unwrap_or(false)
                {
                    break;
                }
                thread::sleep(Duration::from_millis(100));
            }
            Err(_) => break,
        }
    });
    Ok(())
}
