pub(crate) fn service_status(root: &Path) -> Result<Value> {
    let state = read_service_state(root)?;
    let service_pid = state.as_ref().map(|item| item.service_pid).unwrap_or(0);
    Ok(json!({
        "root": root,
        "state_path": project_path(root, ProjectPath::ServiceState),
        "active": pid_exists(service_pid),
        "service_pid": if service_pid == 0 { Value::Null } else { json!(service_pid) },
        "agent_pids": state.as_ref().map(|item| item.agent_pids.clone()).unwrap_or_default(),
        "state": state,
    }))
}

pub(crate) fn spawn_service_daemon(
    project_root: &Path,
    config_path: &Path,
    root: &Path,
    agent_pids: &[u32],
    pid_check_interval: Option<u64>,
    worker_interval: Option<f64>,
    retry_failed: bool,
) -> Result<SpawnedServiceDaemon> {
    fs::create_dir_all(project_path(root, ProjectPath::MemoryDir))?;
    let log_path = project_path(root, ProjectPath::ServiceLog);
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
        .arg("--root")
        .arg(project_root)
        .arg("service")
        .arg("start")
        .arg("--config")
        .arg(config_path)
        .arg("--foreground")
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    for pid in agent_pids.iter().copied().filter(|pid| *pid > 0) {
        command.arg("--agent-pid").arg(pid.to_string());
    }
    if let Some(interval) = pid_check_interval {
        command
            .arg("--pid-check-interval")
            .arg(interval.to_string());
    }
    if let Some(interval) = worker_interval {
        command.arg("--worker-interval").arg(interval.to_string());
    }
    if retry_failed {
        command.arg("--retry-failed");
    }
    detach_daemon(&mut command);

    let child = command
        .spawn()
        .with_context(|| "failed to spawn agent-memory service daemon")?;
    let pid = child.id();
    Ok(SpawnedServiceDaemon {
        child,
        pid,
        log_path,
    })
}

pub(crate) fn detach_daemon(command: &mut Command) {
    #[cfg(unix)]
    unsafe {
        command.pre_exec(|| {
            if setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

pub(crate) fn wait_for_service_start(
    root: &Path,
    child: &mut Child,
    timeout: Duration,
    log_path: &Path,
) -> Result<Value> {
    let deadline = Instant::now() + timeout;
    loop {
        let status = service_status(root)?;
        if status.get("active").and_then(Value::as_bool) == Some(true) {
            return Ok(status);
        }
        if let Some(exit_status) = child.try_wait()? {
            bail!(
                "agent-memory service exited before becoming active ({exit_status}); see {}",
                log_path.display()
            );
        }
        if Instant::now() >= deadline {
            bail!(
                "timed out waiting for agent-memory service to start; see {}",
                log_path.display()
            );
        }
        thread::sleep(Duration::from_millis(100));
    }
}

pub(crate) fn register_agent_pids(root: &Path, agent_pids: &[u32]) -> Result<Value> {
    let mut state = read_service_state(root)?
        .ok_or_else(|| anyhow!("agent-memory service is not running for {}", root.display()))?;
    if !pid_exists(state.service_pid) {
        bail!(
            "stale agent-memory service state at {}",
            project_path(root, ProjectPath::ServiceState).display()
        );
    }
    let mut merged: HashSet<u32> = state.agent_pids.into_iter().collect();
    merged.extend(agent_pids.iter().copied().filter(|pid| pid_exists(*pid)));
    state.agent_pids = sorted_pids(merged);
    state.updated_at = now();
    write_service_state(root, &state)?;
    Ok(json!({"ok": true, "service": service_status(root)?}))
}

pub(crate) fn request_service_stop(root: &Path, timeout: Duration) -> Result<Value> {
    let Some(mut state) = read_service_state(root)? else {
        return Ok(json!({"ok": true, "stopped": false, "service": service_status(root)?}));
    };
    if !pid_exists(state.service_pid) {
        state.service_pid = 0;
        state.agent_pids.clear();
        state.updated_at = now();
        if state.stopped_at.is_none() {
            state.stopped_at = Some(now());
        }
        write_service_state(root, &state)?;
        return Ok(json!({"ok": true, "stopped": true, "service": service_status(root)?}));
    }

    state.stop_requested_at = Some(now());
    state.updated_at = now();
    write_service_state(root, &state)?;

    let deadline = Instant::now() + timeout;
    loop {
        let status = service_status(root)?;
        if status.get("active").and_then(Value::as_bool) == Some(false) {
            return Ok(json!({"ok": true, "stopped": true, "service": status}));
        }
        if Instant::now() >= deadline {
            bail!(
                "timed out waiting for agent-memory service to stop; state at {}",
                project_path(root, ProjectPath::ServiceState).display()
            );
        }
        thread::sleep(Duration::from_millis(100));
    }
}
