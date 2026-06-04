pub(crate) fn registry_entries() -> Result<Vec<ProcessRegistryEntry>> {
    let path = home_path(HomePath::ProcessRegistry);
    if !path.exists() {
        return Ok(Vec::new());
    }
    Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
}

pub(crate) fn write_registry(entries: &[ProcessRegistryEntry]) -> Result<()> {
    let path = home_path(HomePath::ProcessRegistry);
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
    let path = project_path(root, ProjectPath::ServiceState);
    if !path.exists() {
        return Ok(None);
    }
    let state = serde_json::from_str(&fs::read_to_string(path)?)?;
    Ok(Some(state))
}

pub(crate) fn write_service_state(root: &Path, state: &ServiceState) -> Result<()> {
    fs::create_dir_all(project_path(root, ProjectPath::MemoryDir))?;
    fs::write(
        project_path(root, ProjectPath::ServiceState),
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

pub(crate) fn default_agent_pids() -> Vec<u32> {
    #[cfg(unix)]
    {
        ancestor_pids(std::process::id())
    }
    #[cfg(not(unix))]
    {
        Vec::new()
    }
}

#[cfg(unix)]
fn ancestor_pids(pid: u32) -> Vec<u32> {
    let mut result = Vec::new();
    let mut current = pid;
    for _ in 0..16 {
        let Some(parent) = process_parent_pid(current) else {
            break;
        };
        if parent <= 1 {
            break;
        }
        if !result.contains(&parent) {
            result.push(parent);
        }
        current = parent;
    }
    result
}

pub(crate) fn process_parent_pid(pid: u32) -> Option<u32> {
    #[cfg(unix)]
    {
        let output = Command::new("ps")
            .args(["-o", "ppid=", "-p", &pid.to_string()])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse::<u32>()
            .ok()
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        None
    }
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

fn service_lost_all_tracked_agents(previous: &[u32], live: &[u32]) -> bool {
    !previous.is_empty() && live.is_empty()
}

pub(crate) fn sleep_until_stop(stop: &AtomicBool, duration: Duration) {
    let mut slept = Duration::ZERO;
    while slept < duration && !stop.load(Ordering::SeqCst) {
        let step = (duration - slept).min(Duration::from_millis(200));
        thread::sleep(step);
        slept += step;
    }
}
