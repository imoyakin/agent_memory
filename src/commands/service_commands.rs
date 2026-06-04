fn cmd_serve(
    root_arg: Option<PathBuf>,
    config_arg: Option<PathBuf>,
    agent_pids: Vec<u32>,
    pid_check_interval: Option<u64>,
    worker_interval: Option<f64>,
    retry_failed: bool,
    foreground: bool,
) -> Result<Value> {
    let (project_root, config_path, user_config) =
        active_user_config(root_arg.clone(), config_arg)?;
    let runtime_root = resolve_memory_root(&user_config, &config_path, &project_root)?;
    let requested_agent_pids = if agent_pids.is_empty() {
        default_agent_pids()
    } else {
        agent_pids
    };
    let live_agent_pids: Vec<u32> = requested_agent_pids
        .into_iter()
        .filter(|pid| *pid > 0 && pid_exists(*pid))
        .collect();
    let root = runtime_root;
    if service_status(&root)?
        .get("active")
        .and_then(Value::as_bool)
        == Some(true)
    {
        return register_agent_pids(&root, &live_agent_pids);
    }
    if !foreground {
        let mut daemon = spawn_service_daemon(
            &project_root,
            &config_path,
            &root,
            &live_agent_pids,
            pid_check_interval,
            worker_interval,
            retry_failed,
        )?;
        let service = wait_for_service_start(
            &root,
            &mut daemon.child,
            Duration::from_secs(5),
            &daemon.log_path,
        )?;
        return Ok(json!({
            "ok": true,
            "daemonized": true,
            "service_pid": daemon.pid,
            "log_path": daemon.log_path,
            "service": service
        }));
    }
    crate::storage::enable_qdrant_supervisor_context();
    cmd_init(InitOptions {
        root_arg: Some(project_root.clone()),
        config_path_arg: Some(config_path.clone()),
        provider: None,
        model: None,
        dim: None,
        endpoint: None,
        collection: None,
        force: false,
        update_agents: true,
        start_service: false,
    })?;
    run_service_loop(
        root,
        config_path,
        user_config,
        live_agent_pids,
        pid_check_interval,
        worker_interval,
        retry_failed,
    )
}

fn cmd_service_status(root_arg: Option<PathBuf>) -> Result<Value> {
    let root = runtime_root(root_arg)?;
    request_service_status(&root)
}

fn cmd_service_stop(root_arg: Option<PathBuf>, timeout_seconds: u64) -> Result<Value> {
    let root = runtime_root(root_arg)?;
    if let Ok(Some(state)) = crate::service::read_service_state(&root) {
        if let Some(endpoint) = state.ipc {
            if let Ok(response) = crate::ipc::request(&endpoint, json!({"method": "stop"})) {
                let deadline = Instant::now() + Duration::from_secs(timeout_seconds.max(1));
                while Instant::now() < deadline {
                    if request_service_status(&root)
                        .ok()
                        .and_then(|value| value.pointer("/service/active").and_then(Value::as_bool))
                        == Some(false)
                    {
                        return Ok(json!({"ok": true, "stopped": true, "ipc": response}));
                    }
                    thread::sleep(Duration::from_millis(100));
                }
            }
        }
    }
    request_service_stop(&root, Duration::from_secs(timeout_seconds.max(1)))
}

fn cmd_service_worker(
    root_arg: Option<PathBuf>,
    limit: Option<usize>,
    retry_failed: bool,
) -> Result<Value> {
    let root = runtime_root(root_arg)?;
    request_service_worker(&root, limit, retry_failed)
}

fn cmd_service_register(root_arg: Option<PathBuf>, agent_pids: Vec<u32>) -> Result<Value> {
    let root = runtime_root(root_arg)?;
    register_agent_pids(&root, &agent_pids)
}
