use anyhow::{bail, Context, Result};
use interprocess::local_socket::{
    prelude::*, GenericFilePath, GenericNamespaced, Listener, ListenerNonblockingMode,
    ListenerOptions, Name, Stream,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{BufRead, BufReader, Write};
#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::Duration;

use crate::models::IpcEndpoint;
use crate::paths::{runtime_path, service_socket_path, RuntimePath};

pub(crate) fn service_endpoint(root: &Path) -> IpcEndpoint {
    if cfg!(unix) {
        IpcEndpoint {
            kind: "socket".to_string(),
            transport: "unix".to_string(),
            address: service_socket_path(root).to_string_lossy().to_string(),
            name_type: "filesystem".to_string(),
        }
    } else if GenericNamespaced::is_supported() {
        let hash = root_hash(root);
        IpcEndpoint {
            kind: "socket".to_string(),
            transport: namespaced_transport().to_string(),
            address: format!("agent-memory-{hash}"),
            name_type: "namespaced".to_string(),
        }
    } else {
        IpcEndpoint {
            kind: "socket".to_string(),
            transport: "unix".to_string(),
            address: service_socket_path(root).to_string_lossy().to_string(),
            name_type: "filesystem".to_string(),
        }
    }
}

pub(crate) fn listen(endpoint: &IpcEndpoint) -> Result<Listener> {
    prepare_endpoint(endpoint)?;
    let name = endpoint_name(endpoint)?;
    ListenerOptions::new()
        .name(name)
        .nonblocking(ListenerNonblockingMode::Accept)
        .try_overwrite(true)
        .max_spin_time(Duration::from_millis(200))
        .create_sync()
        .with_context(|| format!("failed to listen on IPC socket {}", endpoint.address))
}

pub(crate) fn discover_service_endpoints() -> Result<Vec<IpcEndpoint>> {
    if !cfg!(unix) {
        return Ok(Vec::new());
    }
    let dir = runtime_path(RuntimePath::SocketDir);
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut endpoints = Vec::new();
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension().and_then(|value| value.to_str()) != Some("sock") {
            continue;
        }
        if !is_socket_file(&path) {
            continue;
        }
        endpoints.push(IpcEndpoint {
            kind: "socket".to_string(),
            transport: "unix".to_string(),
            address: path.to_string_lossy().to_string(),
            name_type: "filesystem".to_string(),
        });
    }
    endpoints.sort_by(|left, right| left.address.cmp(&right.address));
    Ok(endpoints)
}

pub(crate) fn accept(listener: &Listener) -> Result<Option<Stream>> {
    match listener.accept() {
        Ok(stream) => Ok(Some(stream)),
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
        Err(error) => Err(error).context("failed to accept IPC connection"),
    }
}

pub(crate) fn request(endpoint: &IpcEndpoint, request: Value) -> Result<Value> {
    let name = endpoint_name(endpoint)?;
    let mut stream = BufReader::new(
        Stream::connect(name.borrow())
            .with_context(|| format!("failed to connect IPC socket {}", endpoint.address))?,
    );
    let payload = serde_json::to_vec(&request)?;
    stream.get_mut().write_all(&payload)?;
    stream.get_mut().write_all(b"\n")?;
    stream.get_mut().flush()?;

    let mut line = String::new();
    stream.read_line(&mut line)?;
    Ok(serde_json::from_str(line.trim())?)
}

pub(crate) fn respond(mut stream: Stream, response: Value) -> Result<()> {
    stream.write_all(serde_json::to_string(&response)?.as_bytes())?;
    stream.write_all(b"\n")?;
    stream.flush()?;
    Ok(())
}

pub(crate) fn read_request(stream: &mut Stream) -> Result<Value> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    Ok(serde_json::from_str(line.trim())?)
}

pub(crate) fn sleep_after_empty_accept() {
    thread::sleep(Duration::from_millis(100));
}

fn endpoint_name(endpoint: &IpcEndpoint) -> Result<Name<'_>> {
    if endpoint.name_type == "filesystem" {
        endpoint
            .address
            .as_str()
            .to_fs_name::<GenericFilePath>()
            .context("invalid filesystem IPC socket name")
    } else {
        endpoint
            .address
            .as_str()
            .to_ns_name::<GenericNamespaced>()
            .context("invalid namespaced IPC socket name")
    }
}

pub(crate) fn root_hash(root: &Path) -> String {
    let mut hasher = Sha256::new();
    hasher.update(root.to_string_lossy().as_bytes());
    let digest = hasher.finalize();
    digest[..16]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn prepare_endpoint(endpoint: &IpcEndpoint) -> Result<()> {
    if endpoint.name_type != "filesystem" {
        return Ok(());
    }
    let path = Path::new(&endpoint.address);
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    fs::create_dir_all(parent)?;
    secure_socket_dir(parent)?;
    Ok(())
}

fn secure_socket_dir(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        let metadata = fs::metadata(path)?;
        if !metadata.is_dir() {
            bail!("IPC socket parent is not a directory: {}", path.display());
        }
        if let Some(uid) = current_uid() {
            if metadata.uid() != uid {
                bail!(
                    "IPC socket parent is owned by uid {}, expected {}: {}",
                    metadata.uid(),
                    uid,
                    path.display()
                );
            }
        }
        let mut permissions = metadata.permissions();
        if permissions.mode() & 0o777 != 0o700 {
            permissions.set_mode(0o700);
            fs::set_permissions(path, permissions)?;
        }
    }
    Ok(())
}

#[cfg(unix)]
fn current_uid() -> Option<u32> {
    if let Ok(uid) = std::env::var("UID") {
        if let Ok(uid) = uid.parse() {
            return Some(uid);
        }
    }
    Command::new("id")
        .arg("-u")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .and_then(|uid| uid.trim().parse().ok())
}

fn is_socket_file(path: &PathBuf) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileTypeExt;
        fs::symlink_metadata(path)
            .map(|metadata| metadata.file_type().is_socket())
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        false
    }
}

fn namespaced_transport() -> &'static str {
    if cfg!(windows) {
        "named_pipe"
    } else {
        "local_socket"
    }
}
