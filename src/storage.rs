use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Map, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::config::{QdrantConfig, RuntimeConfig};
use crate::models::{MemoryRecord, VectorHit};
use crate::paths::{project_path, resolve_under_root, skill_root, ProjectPath};
use crate::{DEFAULT_DIM, PRIMARY_FIELD, SCHEMA_VERSION, VECTOR_FIELD};

include!("storage/backend.rs");
include!("storage/qdrant_server.rs");
include!("storage/qdrant_records.rs");
include!("storage/backend_records.rs");
include!("storage/codec.rs");
include!("storage/tests.rs");
