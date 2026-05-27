use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Map, Value};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::config::{BackendKind, MilvusRemoteConfig, QdrantConfig, RuntimeConfig};
use crate::models::{MemoryRecord, VectorHit};
use crate::paths::{project_path, resolve_under_root, skill_bin_path, skill_root, ProjectPath};
use crate::records::record_value;
use crate::search::reliability_score;
use crate::{DEFAULT_DIM, PRIMARY_FIELD, SCHEMA_VERSION, VECTOR_FIELD};

const RECORD_OUTPUT_FIELDS: &[&str] = &[
    PRIMARY_FIELD,
    "content",
    "keys",
    "summary",
    "embedding_status",
    "embedding_error",
    "embedding_attempts",
    "memory_type",
    "scope",
    "root_path",
    "tags",
    "source_kind",
    "source_ref",
    "created_at",
    "updated_at",
    "last_accessed_at",
    "access_count",
    "conflict_count",
    "confidence",
    "verified_at",
    "stale_after_days",
    "embedding_provider",
    "embedding_model",
    "embedding_dim",
    "schema_version",
    "reliability",
];

include!("storage/backend.rs");
include!("storage/qdrant_server.rs");
include!("storage/qdrant_records.rs");
include!("storage/backend_records.rs");
include!("storage/milvus_remote.rs");
include!("storage/codec.rs");
include!("storage/tests.rs");
