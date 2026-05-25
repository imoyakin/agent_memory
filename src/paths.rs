use anyhow::Result;
use std::path::{Path, PathBuf};

pub(crate) fn runtime_config_path(root: &Path) -> PathBuf {
    memory_dir(root).join("config.json")
}

pub(crate) fn memory_dir(root: &Path) -> PathBuf {
    root.join(".memory")
}

pub(crate) fn service_state_path(root: &Path) -> PathBuf {
    memory_dir(root).join("service.json")
}

pub(crate) fn service_lock_path(root: &Path) -> PathBuf {
    memory_dir(root).join("service.lock")
}

pub(crate) fn service_socket_path(root: &Path) -> PathBuf {
    memory_dir(root).join("service.sock")
}

pub(crate) fn process_registry_path() -> PathBuf {
    home_root().join(".memory").join("processes.json")
}

pub(crate) fn ui_viewer_registry_path() -> PathBuf {
    home_root().join(".memory").join("ui-viewers.json")
}

pub(crate) fn milvus_lite_server_state_path(root: &Path) -> PathBuf {
    memory_dir(root).join("milvus-lite-server.json")
}

pub(crate) fn milvus_lite_server_log_path(root: &Path) -> PathBuf {
    memory_dir(root).join("milvus-lite-server.log")
}

pub(crate) fn resolve_under_root(root: &Path, raw: &str) -> PathBuf {
    let path = PathBuf::from(raw);
    if path.is_absolute() {
        path
    } else {
        root.join(path)
    }
}

pub(crate) fn skill_root() -> Result<PathBuf> {
    if let Ok(root) = std::env::var("AGENT_MEMORY_SKILL_ROOT") {
        return Ok(PathBuf::from(root));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            if parent.file_name().and_then(|name| name.to_str()) == Some("bin") {
                if let Some(root) = parent.parent() {
                    if root.join("SKILL.md").exists() {
                        return Ok(root.to_path_buf());
                    }
                }
            }
            if parent.join("SKILL.md").exists() {
                return Ok(parent.to_path_buf());
            }
        }
    }
    Ok(PathBuf::from(env!("CARGO_MANIFEST_DIR")))
}

pub(crate) fn skill_bin_path(name: &str) -> Result<PathBuf> {
    Ok(skill_root()?.join("bin").join(executable_name(name)))
}

pub(crate) fn executable_name(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

pub(crate) fn home_root() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

pub(crate) fn absolutize(path: PathBuf) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path)
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}
