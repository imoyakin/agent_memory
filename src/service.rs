use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread;
use std::time::{Duration, Instant};

use crate::config::UserConfig;
use crate::ipc;
use crate::models::{ProcessRegistryEntry, ServiceState};
use crate::paths::{memory_dir, process_registry_path, service_lock_path, service_state_path};
use crate::records::read_records;
use crate::util::now;
use crate::worker::cmd_worker;

#[cfg(unix)]
extern "C" {
    fn setsid() -> i32;
}

pub(crate) struct SpawnedServiceDaemon {
    pub(crate) child: Child,
    pub(crate) pid: u32,
    pub(crate) log_path: PathBuf,
}

pub(crate) fn service_status(root: &Path) -> Result<Value> {
    let state = read_service_state(root)?;
    let service_pid = state.as_ref().map(|item| item.service_pid).unwrap_or(0);
    Ok(json!({
        "root": root,
        "state_path": service_state_path(root),
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
    fs::create_dir_all(memory_dir(root))?;
    let log_path = memory_dir(root).join("service.log");
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
            service_state_path(root).display()
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
                service_state_path(root).display()
            );
        }
        thread::sleep(Duration::from_millis(100));
    }
}

pub(crate) fn run_service_loop(
    root: PathBuf,
    config_path: PathBuf,
    user_config: UserConfig,
    agent_pids: Vec<u32>,
    pid_check_interval: Option<u64>,
    worker_interval: Option<f64>,
    retry_failed: bool,
) -> Result<Value> {
    fs::create_dir_all(memory_dir(&root))?;
    let lock_path = service_lock_path(&root);
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
        install_scope: user_config.install_scope,
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
    let monitor_thread = thread::spawn(move || {
        let pid_check_interval = Duration::from_secs(pid_interval);
        let mut last_pid_check = Instant::now() - pid_check_interval;
        while !monitor_stop.load(Ordering::SeqCst) {
            match read_service_state(&monitor_root) {
                Ok(Some(mut state)) => {
                    if state.stop_requested_at.is_some() {
                        monitor_stop.store(true, Ordering::SeqCst);
                        break;
                    }
                    if last_pid_check.elapsed() >= pid_check_interval {
                        state.agent_pids = sorted_pids(
                            state
                                .agent_pids
                                .into_iter()
                                .filter(|pid| pid_exists(*pid))
                                .collect(),
                        );
                        last_pid_check = Instant::now();
                    }
                    state.memory_count = read_records(&monitor_root)
                        .ok()
                        .map(|records| records.len());
                    state.updated_at = now();
                    let _ = write_service_state(&monitor_root, &state);
                    let _ = register_process(&monitor_root);
                }
                Ok(None) | Err(_) => {
                    monitor_stop.store(true, Ordering::SeqCst);
                    break;
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
    if let Ok(Some(mut state)) = read_service_state(&root) {
        state.service_pid = 0;
        state.agent_pids.clear();
        state.updated_at = now();
        state.stopped_at = Some(now());
        write_service_state(&root, &state)?;
    }
    let _ = unregister_process(&root);
    let _ = fs::remove_file(&lock_path);
    let state_path = service_state_path(&root);
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

pub(crate) fn registry_entries() -> Result<Vec<ProcessRegistryEntry>> {
    let path = process_registry_path();
    if !path.exists() {
        return Ok(Vec::new());
    }
    Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
}

fn write_registry(entries: &[ProcessRegistryEntry]) -> Result<()> {
    let path = process_registry_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string_pretty(entries)? + "\n")?;
    Ok(())
}

fn register_process(root: &Path) -> Result<()> {
    let Some(state) = read_service_state(root)? else {
        return Ok(());
    };
    let Some(ipc) = state.ipc.clone() else {
        return Ok(());
    };
    let mut entries = registry_entries().unwrap_or_default();
    let root_text = root.to_string_lossy().to_string();
    entries.retain(|entry| entry.root != root_text);
    entries.push(ProcessRegistryEntry {
        pid: state.service_pid,
        root: root_text,
        workdir: state.workdir.clone().unwrap_or_else(|| state.root.clone()),
        mode: state.install_scope.clone(),
        status: if state.stop_requested_at.is_some() {
            "stopping".to_string()
        } else {
            "running".to_string()
        },
        memory_count: state.memory_count,
        updated_at: now(),
        ipc,
    });
    write_registry(&entries)
}

fn unregister_process(root: &Path) -> Result<()> {
    let mut entries = registry_entries().unwrap_or_default();
    let root_text = root.to_string_lossy().to_string();
    entries.retain(|entry| entry.root != root_text);
    write_registry(&entries)
}

pub(crate) fn read_service_state(root: &Path) -> Result<Option<ServiceState>> {
    let path = service_state_path(root);
    if !path.exists() {
        return Ok(None);
    }
    let state = serde_json::from_str(&fs::read_to_string(path)?)?;
    Ok(Some(state))
}

pub(crate) fn write_service_state(root: &Path, state: &ServiceState) -> Result<()> {
    fs::create_dir_all(memory_dir(root))?;
    fs::write(
        service_state_path(root),
        serde_json::to_string_pretty(state)? + "\n",
    )?;
    Ok(())
}

pub(crate) fn update_service_worker_error(root: &Path, error: String) -> Result<()> {
    if let Some(mut state) = read_service_state(root)? {
        state.last_worker_error = Some(error.chars().take(500).collect());
        state.updated_at = now();
        write_service_state(root, &state)?;
    }
    Ok(())
}

pub(crate) fn pid_exists(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    if pid == std::process::id() {
        return true;
    }
    pid_exists_platform(pid)
}

#[cfg(unix)]
fn pid_exists_platform(pid: u32) -> bool {
    Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[cfg(windows)]
fn pid_exists_platform(pid: u32) -> bool {
    let filter = format!("PID eq {pid}");
    Command::new("tasklist")
        .args(["/FI", &filter, "/FO", "CSV", "/NH"])
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                Some(String::from_utf8_lossy(&output.stdout).contains(&format!("\"{pid}\"")))
            } else {
                None
            }
        })
        .unwrap_or(false)
}

#[cfg(not(any(unix, windows)))]
fn pid_exists_platform(_pid: u32) -> bool {
    false
}

pub(crate) fn sorted_pids(pids: HashSet<u32>) -> Vec<u32> {
    let mut pids: Vec<_> = pids.into_iter().filter(|pid| *pid > 0).collect();
    pids.sort_unstable();
    pids
}

pub(crate) fn sleep_until_stop(stop: &AtomicBool, duration: Duration) {
    let mut slept = Duration::ZERO;
    while slept < duration && !stop.load(Ordering::SeqCst) {
        let step = (duration - slept).min(Duration::from_millis(200));
        thread::sleep(step);
        slept += step;
    }
}
