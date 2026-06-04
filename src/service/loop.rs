pub(crate) fn run_service_loop(
    root: PathBuf,
    config_path: PathBuf,
    user_config: UserConfig,
    agent_pids: Vec<u32>,
    pid_check_interval: Option<u64>,
    worker_interval: Option<f64>,
    retry_failed: bool,
) -> Result<Value> {
    fs::create_dir_all(project_path(&root, ProjectPath::MemoryDir))?;
    let lock_path = project_path(&root, ProjectPath::ServiceLock);
    if lock_path.exists() {
        if service_status(&root)?
            .get("active")
            .and_then(Value::as_bool)
            == Some(true)
        {
            return register_agent_pids(&root, &agent_pids);
        }
        let _ = fs::remove_file(&lock_path);
    }
    let mut lock = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock_path)
        .with_context(|| format!("failed to acquire service lock {}", lock_path.display()))?;
    writeln!(lock, "{}", std::process::id())?;

    let pid_interval = pid_check_interval
        .unwrap_or(user_config.service.pid_check_interval_seconds)
        .max(1);
    let worker_interval = worker_interval
        .unwrap_or(user_config.worker.interval_seconds)
        .max(0.2);
    let stop = Arc::new(AtomicBool::new(false));
    let started_at = now();
    let state = ServiceState {
        service_pid: std::process::id(),
        agent_pids: sorted_pids(
            agent_pids
                .into_iter()
                .filter(|pid| pid_exists(*pid))
                .collect(),
        ),
        install_scope: user_config.install_scope.clone(),
        config_path: config_path.to_string_lossy().to_string(),
        root: root.to_string_lossy().to_string(),
        started_at: started_at.clone(),
        updated_at: started_at,
        stopped_at: None,
        stop_requested_at: None,
        last_worker_error: None,
        workdir: std::env::current_dir()
            .ok()
            .map(|path| path.to_string_lossy().to_string()),
        memory_count: read_records(&root).ok().map(|records| records.len()),
        ipc: Some(ipc::service_endpoint(&root)),
    };
    write_service_state(&root, &state)?;
    register_process(&root)?;
    println!(
        "agent-memory service started pid={} root={} config={}",
        std::process::id(),
        root.display(),
        config_path.display()
    );

    let ipc_stop = Arc::clone(&stop);
    let ipc_root = root.clone();
    let ipc_thread = thread::spawn(move || {
        if let Err(error) = run_ipc_loop(ipc_root, ipc_stop) {
            eprintln!("agent-memory IPC server stopped: {error}");
        }
    });

    let worker_stop = Arc::clone(&stop);
    let worker_root = root.clone();
    let worker_thread = thread::spawn(move || {
        while !worker_stop.load(Ordering::SeqCst) {
            if let Err(error) = cmd_worker(Some(worker_root.clone()), None, retry_failed) {
                let _ = update_service_worker_error(&worker_root, error.to_string());
            }
            sleep_until_stop(&worker_stop, Duration::from_secs_f64(worker_interval));
        }
    });

    let monitor_stop = Arc::clone(&stop);
    let monitor_root = root.clone();
    let monitor_config = user_config.clone();
    let owned_qdrant_pid = qdrant_pid_for_root(&root).ok().flatten();
    let monitor_qdrant_pid = owned_qdrant_pid;
    let monitor_thread = thread::spawn(move || {
        let pid_check_interval = Duration::from_secs(pid_interval);
        let mut last_pid_check = Instant::now() - pid_check_interval;
        let mut storage_failures = 0usize;
        while !monitor_stop.load(Ordering::SeqCst) {
            if !removable_storage_accessible(&monitor_root, &monitor_config) {
                storage_failures += 1;
                if removable_storage_should_stop(storage_failures) {
                    monitor_stop.store(true, Ordering::SeqCst);
                    terminate_owned_qdrant(monitor_qdrant_pid);
                    break;
                }
                sleep_until_stop(&monitor_stop, Duration::from_secs(1));
                continue;
            }
            storage_failures = 0;
            match read_service_state(&monitor_root) {
                Ok(Some(mut state)) => {
                    if state.stop_requested_at.is_some() {
                        monitor_stop.store(true, Ordering::SeqCst);
                        terminate_owned_qdrant(monitor_qdrant_pid);
                        break;
                    }
                    if last_pid_check.elapsed() >= pid_check_interval {
                        let previous_agent_pids = state.agent_pids.clone();
                        let live_agent_pids = sorted_pids(
                            state
                                .agent_pids
                                .into_iter()
                                .filter(|pid| pid_exists(*pid))
                                .collect(),
                        );
                        if service_lost_all_tracked_agents(&previous_agent_pids, &live_agent_pids)
                        {
                            monitor_stop.store(true, Ordering::SeqCst);
                            terminate_owned_qdrant(monitor_qdrant_pid);
                            break;
                        }
                        state.agent_pids = live_agent_pids;
                        last_pid_check = Instant::now();
                    }
                    state.memory_count = read_records(&monitor_root)
                        .ok()
                        .map(|records| records.len());
                    state.updated_at = now();
                    if write_service_state(&monitor_root, &state).is_err()
                        || register_process(&monitor_root).is_err()
                    {
                        storage_failures += 1;
                        if removable_storage_should_stop(storage_failures) {
                            monitor_stop.store(true, Ordering::SeqCst);
                            terminate_owned_qdrant(monitor_qdrant_pid);
                            break;
                        }
                    }
                }
                Ok(None) | Err(_) => {
                    storage_failures += 1;
                    if removable_storage_should_stop(storage_failures) {
                        monitor_stop.store(true, Ordering::SeqCst);
                        terminate_owned_qdrant(monitor_qdrant_pid);
                        break;
                    }
                }
            }
            sleep_until_stop(&monitor_stop, Duration::from_secs(1));
        }
    });

    while !stop.load(Ordering::SeqCst) {
        thread::sleep(Duration::from_millis(200));
    }
    let _ = ipc_thread.join();
    let _ = worker_thread.join();
    let _ = monitor_thread.join();
    terminate_owned_qdrant(owned_qdrant_pid);
    if let Ok(Some(mut state)) = read_service_state(&root) {
        state.service_pid = 0;
        state.agent_pids.clear();
        state.updated_at = now();
        state.stopped_at = Some(now());
        write_service_state(&root, &state)?;
    }
    let _ = unregister_process(&root);
    let endpoint = ipc::service_endpoint(&root);
    if endpoint.name_type == "filesystem" {
        let _ = fs::remove_file(endpoint.address);
    }
    let _ = fs::remove_file(&lock_path);
    let state_path = project_path(&root, ProjectPath::ServiceState);
    println!(
        "agent-memory service stopped pid={} root={}",
        std::process::id(),
        root.display()
    );
    Ok(json!({
        "ok": true,
        "service": "stopped",
        "root": root,
        "state_path": state_path
    }))
}

pub(crate) fn request_service_status(root: &Path) -> Result<Value> {
    if let Some(state) = read_service_state(root)? {
        if let Some(endpoint) = state.ipc {
            if let Ok(response) = ipc::request(&endpoint, json!({"method": "status"})) {
                return Ok(response);
            }
        }
    }
    Ok(json!({"ok": true, "service": service_status(root)?}))
}

pub(crate) fn request_service_worker(
    root: &Path,
    limit: Option<usize>,
    retry_failed: bool,
) -> Result<Value> {
    if let Some(state) = read_service_state(root)? {
        if let Some(endpoint) = state.ipc {
            if let Ok(response) = ipc::request(
                &endpoint,
                json!({"method": "worker.run_once", "limit": limit, "retry_failed": retry_failed}),
            ) {
                return Ok(response);
            }
        }
    }
    cmd_worker(Some(root.to_path_buf()), limit, retry_failed)
}

fn run_ipc_loop(root: PathBuf, stop: Arc<AtomicBool>) -> Result<()> {
    let endpoint = ipc::service_endpoint(&root);
    let listener = ipc::listen(&endpoint)?;
    while !stop.load(Ordering::SeqCst) {
        let Some(mut stream) = ipc::accept(&listener)? else {
            ipc::sleep_after_empty_accept();
            continue;
        };
        let response = match ipc::read_request(&mut stream) {
            Ok(request) => handle_ipc_request(&root, &stop, request),
            Err(error) => json!({"ok": false, "error": error.to_string()}),
        };
        let _ = ipc::respond(stream, response);
    }
    Ok(())
}

fn handle_ipc_request(root: &Path, stop: &Arc<AtomicBool>, request: Value) -> Value {
    match request.get("method").and_then(Value::as_str) {
        Some("status") => json!({"ok": true, "service": service_status(root).ok()}),
        Some("stop") => {
            stop.store(true, Ordering::SeqCst);
            json!({"ok": true, "stopping": true, "service": service_status(root).ok()})
        }
        Some("worker.run_once") => {
            let limit = request
                .get("limit")
                .and_then(Value::as_u64)
                .map(|value| value as usize);
            let retry_failed = request
                .get("retry_failed")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            match cmd_worker(Some(root.to_path_buf()), limit, retry_failed) {
                Ok(value) => value,
                Err(error) => json!({"ok": false, "error": error.to_string()}),
            }
        }
        Some(method) => json!({"ok": false, "error": format!("unknown IPC method: {method}")}),
        None => json!({"ok": false, "error": "missing IPC method"}),
    }
}
