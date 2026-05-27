use anyhow::Result;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Copy)]
pub(crate) enum ProjectPath {
    MemoryDir,
    RuntimeConfig,
    ServiceState,
    ServiceLock,
    ServiceLog,
    QdrantServerState,
    QdrantServerLog,
}

impl ProjectPath {
    fn segments(self) -> &'static [&'static str] {
        match self {
            Self::MemoryDir => &[".memory"],
            Self::RuntimeConfig => &[".memory", "config.json"],
            Self::ServiceState => &[".memory", "service.json"],
            Self::ServiceLock => &[".memory", "service.lock"],
            Self::ServiceLog => &[".memory", "service.log"],
            Self::QdrantServerState => &[".memory", "qdrant-server.json"],
            Self::QdrantServerLog => &[".memory", "qdrant-server.log"],
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum HomePath {
    ProcessRegistry,
    UiViewerRegistry,
    UiGatewayState,
    UiGatewayLock,
    UiGatewayLog,
    GatewayRemotes,
}

impl HomePath {
    fn segments(self) -> &'static [&'static str] {
        match self {
            Self::ProcessRegistry => &[".memory", "processes.json"],
            Self::UiViewerRegistry => &[".memory", "ui-viewers.json"],
            Self::UiGatewayState => &[".memory", "ui-gateway.json"],
            Self::UiGatewayLock => &[".memory", "ui-gateway.lock"],
            Self::UiGatewayLog => &[".memory", "ui-gateway.log"],
            Self::GatewayRemotes => &[".memory", "gateway-remotes.json"],
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum RuntimePath {
    SocketDir,
}

impl RuntimePath {
    fn segments(self) -> &'static [&'static str] {
        match self {
            Self::SocketDir => &["sockets"],
        }
    }
}

pub(crate) fn project_path(root: &Path, path: ProjectPath) -> PathBuf {
    join_segments(root.to_path_buf(), path.segments())
}

pub(crate) fn home_path(path: HomePath) -> PathBuf {
    join_segments(home_root(), path.segments())
}

pub(crate) fn runtime_path(path: RuntimePath) -> PathBuf {
    join_segments(agent_memory_runtime_dir(), path.segments())
}

pub(crate) fn service_socket_path(root: &Path) -> PathBuf {
    runtime_path(RuntimePath::SocketDir).join(format!("{}.sock", crate::ipc::root_hash(root)))
}

fn join_segments(mut path: PathBuf, segments: &[&str]) -> PathBuf {
    for segment in segments {
        path.push(segment);
    }
    path
}

fn agent_memory_runtime_dir() -> PathBuf {
    if let Ok(path) = std::env::var("AGENT_MEMORY_RUNTIME_DIR") {
        return PathBuf::from(path);
    }
    if cfg!(target_os = "linux") {
        if let Ok(path) = std::env::var("XDG_RUNTIME_DIR") {
            return PathBuf::from(path).join("agent-memory");
        }
    }
    if cfg!(unix) {
        return PathBuf::from(format!("/tmp/agent-memory-{}", user_id()));
    }
    std::env::temp_dir().join(format!("agent-memory-{}", user_id()))
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

pub(crate) fn home_root() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

fn user_id() -> String {
    if let Ok(uid) = std::env::var("UID") {
        if !uid.trim().is_empty() {
            return uid;
        }
    }
    Command::new("id")
        .arg("-u")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|uid| !uid.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

pub(crate) fn absolutize(path: PathBuf) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path)
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_catalog_builds_project_home_and_runtime_paths() {
        let root = Path::new("/tmp/project");

        assert_eq!(
            project_path(root, ProjectPath::RuntimeConfig),
            PathBuf::from("/tmp/project/.memory/config.json")
        );
        assert_eq!(
            home_path(HomePath::UiGatewayLog),
            home_root().join(".memory").join("ui-gateway.log")
        );
        assert!(runtime_path(RuntimePath::SocketDir).ends_with("sockets"));
    }
}
