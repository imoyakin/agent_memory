fn removable_storage_accessible(root: &Path, config: &UserConfig) -> bool {
    if !root.is_dir() {
        return false;
    }
    let memory = project_path(root, ProjectPath::MemoryDir);
    if !memory.is_dir() {
        return false;
    }
    let storage_path = resolve_under_root(root, &config.storage.qdrant.storage_path);
    if !storage_path.is_dir() {
        return false;
    }
    let heartbeat = memory.join(".service-heartbeat");
    OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&heartbeat)
        .and_then(|mut file| writeln!(file, "{}", now()))
        .is_ok()
}

fn removable_storage_should_stop(failures: usize) -> bool {
    failures >= 3
}

fn qdrant_pid_for_root(root: &Path) -> Result<Option<u32>> {
    let path = project_path(root, ProjectPath::QdrantServerState);
    if !path.exists() {
        return Ok(None);
    }
    let state: Value = serde_json::from_str(&fs::read_to_string(path)?)?;
    Ok(state
        .get("pid")
        .and_then(Value::as_u64)
        .map(|pid| pid as u32)
        .filter(|pid| *pid > 0))
}

fn terminate_owned_qdrant(pid: Option<u32>) {
    let Some(pid) = pid.filter(|pid| pid_exists(*pid)) else {
        return;
    };
    let _ = terminate_process_group(pid);
}

fn terminate_process_group(pid: u32) -> Result<()> {
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
