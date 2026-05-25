use anyhow::{Context, Result};
use interprocess::local_socket::{
    prelude::*, GenericFilePath, GenericNamespaced, Listener, ListenerNonblockingMode,
    ListenerOptions, Name, Stream,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::thread;
use std::time::Duration;

use crate::models::IpcEndpoint;
use crate::paths::service_socket_path;

pub(crate) fn service_endpoint(root: &Path) -> IpcEndpoint {
    if GenericNamespaced::is_supported() {
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
    let name = endpoint_name(endpoint)?;
    ListenerOptions::new()
        .name(name)
        .nonblocking(ListenerNonblockingMode::Accept)
        .try_overwrite(true)
        .max_spin_time(Duration::from_millis(200))
        .create_sync()
        .with_context(|| format!("failed to listen on IPC socket {}", endpoint.address))
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

fn root_hash(root: &Path) -> String {
    let mut hasher = Sha256::new();
    hasher.update(root.to_string_lossy().as_bytes());
    let digest = hasher.finalize();
    digest[..16]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn namespaced_transport() -> &'static str {
    if cfg!(windows) {
        "named_pipe"
    } else {
        "local_socket"
    }
}
