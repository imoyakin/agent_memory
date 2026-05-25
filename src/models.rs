use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct MemoryRecord {
    #[serde(default)]
    pub(crate) uuid: String,
    pub(crate) content: String,
    #[serde(default)]
    pub(crate) keys: Vec<String>,
    #[serde(default)]
    pub(crate) summary: String,
    #[serde(default = "pending_status")]
    pub(crate) embedding_status: String,
    #[serde(default)]
    pub(crate) embedding_error: Option<String>,
    #[serde(default)]
    pub(crate) embedding_attempts: u64,
    pub(crate) memory_type: String,
    pub(crate) scope: String,
    pub(crate) root_path: String,
    #[serde(default)]
    pub(crate) tags: Vec<String>,
    pub(crate) source_kind: String,
    #[serde(default)]
    pub(crate) source_ref: String,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
    #[serde(default)]
    pub(crate) last_accessed_at: Option<String>,
    #[serde(default)]
    pub(crate) access_count: u64,
    #[serde(default)]
    pub(crate) conflict_count: u64,
    pub(crate) confidence: f64,
    #[serde(default)]
    pub(crate) verified_at: Option<String>,
    #[serde(default)]
    pub(crate) stale_after_days: Option<u64>,
    pub(crate) embedding_provider: String,
    pub(crate) embedding_model: String,
    pub(crate) embedding_dim: usize,
    pub(crate) schema_version: u32,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct SearchResult {
    pub(crate) uuid: String,
    pub(crate) memory_id: String,
    pub(crate) summary: String,
    pub(crate) content: String,
    pub(crate) keys: Vec<String>,
    pub(crate) matched_keys: Vec<String>,
    pub(crate) memory_type: String,
    pub(crate) scope: String,
    pub(crate) tags: Vec<String>,
    pub(crate) source_kind: String,
    pub(crate) source_ref: String,
    pub(crate) updated_at: String,
    pub(crate) confidence: f64,
    pub(crate) reliability: f64,
    pub(crate) relevance: f64,
    pub(crate) score: f64,
    pub(crate) embedding_status: String,
    pub(crate) match_reasons: Vec<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct VectorHit {
    pub(crate) uuid: String,
    pub(crate) distance: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct IpcEndpoint {
    pub(crate) kind: String,
    pub(crate) transport: String,
    pub(crate) address: String,
    pub(crate) name_type: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct ProcessRegistryEntry {
    pub(crate) pid: u32,
    pub(crate) root: String,
    pub(crate) workdir: String,
    pub(crate) mode: String,
    pub(crate) status: String,
    #[serde(default)]
    pub(crate) memory_count: Option<usize>,
    pub(crate) updated_at: String,
    pub(crate) ipc: IpcEndpoint,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct ServiceState {
    pub(crate) service_pid: u32,
    pub(crate) agent_pids: Vec<u32>,
    pub(crate) install_scope: String,
    pub(crate) config_path: String,
    pub(crate) root: String,
    pub(crate) started_at: String,
    pub(crate) updated_at: String,
    #[serde(default)]
    pub(crate) stopped_at: Option<String>,
    #[serde(default)]
    pub(crate) stop_requested_at: Option<String>,
    #[serde(default)]
    pub(crate) last_worker_error: Option<String>,
    #[serde(default)]
    pub(crate) workdir: Option<String>,
    #[serde(default)]
    pub(crate) memory_count: Option<usize>,
    #[serde(default)]
    pub(crate) ipc: Option<IpcEndpoint>,
}

pub(crate) fn pending_status() -> String {
    "pending".to_string()
}
