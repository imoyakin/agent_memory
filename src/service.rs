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
use crate::paths::{home_path, project_path, resolve_under_root, HomePath, ProjectPath};
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

include!("service/lifecycle.rs");
include!("service/loop.rs");
include!("service/registry.rs");
include!("service/removable.rs");
include!("service/tests.rs");
