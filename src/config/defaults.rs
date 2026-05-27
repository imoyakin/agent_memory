pub(crate) fn schema_version() -> u32 {
    SCHEMA_VERSION
}

pub(crate) fn default_install_scope() -> String {
    "project".to_string()
}

pub(crate) fn default_memory_root() -> String {
    ".".to_string()
}

pub(crate) fn default_allowed_memory_types() -> Vec<String> {
    [
        "code",
        "decision",
        "domain",
        "failure",
        "preference",
        "project",
        "research",
    ]
    .into_iter()
    .map(ToString::to_string)
    .collect()
}

pub(crate) fn default_collection_name() -> String {
    DEFAULT_COLLECTION.to_string()
}

pub(crate) fn default_provider() -> String {
    DEFAULT_PROVIDER.to_string()
}

pub(crate) fn default_model() -> String {
    DEFAULT_MODEL.to_string()
}

pub(crate) fn default_dim() -> usize {
    DEFAULT_DIM
}

pub(crate) fn default_endpoint_option() -> Option<String> {
    Some(DEFAULT_ENDPOINT.to_string())
}

pub(crate) fn max_content_bytes() -> usize {
    MAX_CONTENT_BYTES
}

pub(crate) fn default_limit() -> usize {
    1
}

pub(crate) fn default_true() -> bool {
    true
}

pub(crate) fn default_associative_limit() -> usize {
    3
}

pub(crate) fn default_associative_min_score() -> f64 {
    0.35
}

pub(crate) fn default_associative_strategy() -> String {
    "keys_then_embedding".to_string()
}

pub(crate) fn default_worker_mode() -> String {
    "manual".to_string()
}

pub(crate) fn default_worker_interval() -> f64 {
    5.0
}

pub(crate) fn default_service_mode() -> String {
    "resident".to_string()
}

pub(crate) fn default_pid_interval() -> u64 {
    60
}

pub(crate) fn default_qdrant_uri() -> String {
    "http://127.0.0.1:6333".to_string()
}

pub(crate) fn default_qdrant_storage_path() -> String {
    ".memory/qdrant/default".to_string()
}

pub(crate) fn default_qdrant_binary() -> String {
    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(bin_dir) = current_exe.parent() {
            let sibling = bin_dir.join("qdrant");
            if sibling.is_file() {
                return sibling.to_string_lossy().to_string();
            }
        }
    }
    "qdrant".to_string()
}
