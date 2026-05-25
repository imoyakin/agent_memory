use anyhow::{bail, Result};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use crate::discovery::discover;
use crate::paths::{memory_dir, runtime_config_path};
use crate::util::{new_uuid_string, uuid_hex};
use crate::{
    DEFAULT_COLLECTION, DEFAULT_DIM, DEFAULT_ENDPOINT, DEFAULT_MODEL, DEFAULT_PROVIDER,
    MAX_CONTENT_BYTES, SCHEMA_VERSION,
};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct UserConfig {
    #[serde(default = "schema_version")]
    pub(crate) schema_version: u32,
    #[serde(default = "default_install_scope")]
    pub(crate) install_scope: String,
    #[serde(default = "default_memory_root")]
    pub(crate) memory_root: String,
    #[serde(default = "default_allowed_memory_types")]
    pub(crate) allowed_memory_types: Vec<String>,
    #[serde(default = "default_collection_name")]
    pub(crate) collection_name: String,
    #[serde(default)]
    pub(crate) embedding: EmbeddingConfig,
    #[serde(default)]
    pub(crate) limits: LimitsConfig,
    #[serde(default)]
    pub(crate) retrieval: RetrievalConfig,
    #[serde(default)]
    pub(crate) worker: WorkerConfig,
    #[serde(default)]
    pub(crate) service: ServiceConfig,
    #[serde(default)]
    pub(crate) storage: StorageConfig,
}

impl Default for UserConfig {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            install_scope: default_install_scope(),
            memory_root: default_memory_root(),
            allowed_memory_types: default_allowed_memory_types(),
            collection_name: default_collection_name(),
            embedding: EmbeddingConfig::default(),
            limits: LimitsConfig::default(),
            retrieval: RetrievalConfig::default(),
            worker: WorkerConfig::default(),
            service: ServiceConfig::default(),
            storage: StorageConfig::default(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct EmbeddingConfig {
    #[serde(default = "default_provider")]
    pub(crate) provider: String,
    #[serde(default = "default_model")]
    pub(crate) model: String,
    #[serde(default = "default_dim")]
    pub(crate) dim: usize,
    #[serde(default = "default_endpoint_option")]
    pub(crate) endpoint: Option<String>,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            provider: default_provider(),
            model: default_model(),
            dim: default_dim(),
            endpoint: default_endpoint_option(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct LimitsConfig {
    #[serde(default = "max_content_bytes")]
    pub(crate) max_content_bytes: usize,
}

impl Default for LimitsConfig {
    fn default() -> Self {
        Self {
            max_content_bytes: MAX_CONTENT_BYTES,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct RetrievalConfig {
    #[serde(default = "default_limit")]
    pub(crate) default_limit: usize,
    #[serde(default = "default_true")]
    pub(crate) advisory_memory: bool,
    #[serde(default)]
    pub(crate) associative: AssociativeRetrievalConfig,
}

impl Default for RetrievalConfig {
    fn default() -> Self {
        Self {
            default_limit: 1,
            advisory_memory: true,
            associative: AssociativeRetrievalConfig::default(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct AssociativeRetrievalConfig {
    #[serde(default)]
    pub(crate) enabled: bool,
    #[serde(default = "default_associative_limit")]
    pub(crate) limit: usize,
    #[serde(default = "default_associative_min_score")]
    pub(crate) min_score: f64,
    #[serde(default = "default_associative_strategy")]
    pub(crate) strategy: String,
}

impl Default for AssociativeRetrievalConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            limit: 3,
            min_score: 0.35,
            strategy: "keys_then_embedding".to_string(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct WorkerConfig {
    #[serde(default = "default_worker_mode")]
    pub(crate) mode: String,
    #[serde(default = "default_worker_interval")]
    pub(crate) interval_seconds: f64,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            mode: "manual".to_string(),
            interval_seconds: 5.0,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct ServiceConfig {
    #[serde(default = "default_service_mode")]
    pub(crate) mode: String,
    #[serde(default = "default_pid_interval")]
    pub(crate) pid_check_interval_seconds: u64,
}

impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            mode: "resident".to_string(),
            pid_check_interval_seconds: 60,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BackendKind {
    MilvusLite,
    MilvusRemote,
}

impl std::fmt::Display for BackendKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BackendKind::MilvusLite => write!(f, "milvus_lite"),
            BackendKind::MilvusRemote => write!(f, "milvus_remote"),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct StorageConfig {
    #[serde(default = "new_uuid_string")]
    pub(crate) instance_uuid: String,
    #[serde(default = "default_backend")]
    pub(crate) backend: BackendKind,
    #[serde(default)]
    pub(crate) milvus_lite: MilvusLiteConfig,
    #[serde(default)]
    pub(crate) milvus_remote: MilvusRemoteConfig,
}

impl Default for StorageConfig {
    fn default() -> Self {
        let instance_uuid = new_uuid_string();
        Self {
            milvus_lite: MilvusLiteConfig::for_uuid(&instance_uuid),
            milvus_remote: MilvusRemoteConfig::for_uuid(&instance_uuid, None),
            instance_uuid,
            backend: BackendKind::MilvusLite,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct MilvusLiteConfig {
    #[serde(default = "default_lite_db_path")]
    pub(crate) db_path: String,
    #[serde(default = "default_bridge")]
    pub(crate) bridge: String,
}

impl MilvusLiteConfig {
    fn for_uuid(instance_uuid: &str) -> Self {
        Self {
            db_path: format!(".memory/milvus/{}.db", uuid_hex(instance_uuid)),
            bridge: "python_process".to_string(),
        }
    }
}

impl Default for MilvusLiteConfig {
    fn default() -> Self {
        Self {
            db_path: default_lite_db_path(),
            bridge: default_bridge(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct MilvusRemoteConfig {
    #[serde(default)]
    pub(crate) uri: Option<String>,
    #[serde(default = "default_remote_database")]
    pub(crate) database: String,
    #[serde(default = "default_collection_name")]
    pub(crate) collection: String,
    #[serde(default)]
    pub(crate) token: Option<String>,
}

impl MilvusRemoteConfig {
    fn for_uuid(instance_uuid: &str, uri: Option<String>) -> Self {
        Self {
            uri,
            database: format!("agent_memory_{}", uuid_hex(instance_uuid)),
            collection: DEFAULT_COLLECTION.to_string(),
            token: None,
        }
    }
}

impl Default for MilvusRemoteConfig {
    fn default() -> Self {
        Self::for_uuid(&new_uuid_string(), None)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct RuntimeConfig {
    pub(crate) schema_version: u32,
    pub(crate) root_path: String,
    pub(crate) collection_name: String,
    pub(crate) embedding_provider: String,
    pub(crate) embedding_model: String,
    pub(crate) embedding_dim: usize,
    pub(crate) embedding_endpoint: Option<String>,
    pub(crate) created_at: String,
    pub(crate) install_scope: String,
    pub(crate) allowed_memory_types: Vec<String>,
    pub(crate) retrieval: RetrievalConfig,
    pub(crate) storage: StorageConfig,
}

pub(crate) fn write_user_config_template(
    path: &Path,
    install_scope: &str,
    backend: BackendKind,
    remote_uri: Option<String>,
    force: bool,
) -> Result<()> {
    if path.exists() && !force {
        bail!("config file already exists: {}", path.display());
    }
    let mut config = UserConfig {
        install_scope: install_scope.to_string(),
        ..UserConfig::default()
    };
    if install_scope == "global" {
        config.memory_root = "~".to_string();
        config.allowed_memory_types = vec!["environment".to_string(), "preference".to_string()];
    }
    config.storage = storage_for_backend(backend, remote_uri, None, true);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_yaml::to_string(&config)?)?;
    Ok(())
}

pub(crate) fn load_user_config(path: &Path) -> Result<UserConfig> {
    if !matches!(
        path.extension().and_then(|value| value.to_str()),
        Some("yaml" | "yml")
    ) {
        bail!("user config must be memory.yaml, got: {}", path.display());
    }
    let mut config: UserConfig = serde_yaml::from_str(&fs::read_to_string(path)?)?;
    normalize_storage(&mut config.storage);
    Ok(config)
}

pub(crate) fn persist_user_config_if_exists(path: &Path, config: &UserConfig) -> Result<()> {
    if path.exists() {
        fs::write(path, serde_yaml::to_string(config)?)?;
    }
    Ok(())
}

pub(crate) fn persist_storage_to_user_config(root: &Path, storage: &StorageConfig) -> Result<()> {
    let Some(discovered) = discover(Some(root))? else {
        return Ok(());
    };
    let mut config = load_user_config(&discovered.config_path)?;
    config.storage = storage.clone();
    fs::write(&discovered.config_path, serde_yaml::to_string(&config)?)?;
    Ok(())
}

pub(crate) fn storage_for_backend(
    backend: BackendKind,
    remote_uri: Option<String>,
    remote_token: Option<String>,
    _new_instance: bool,
) -> StorageConfig {
    let instance_uuid = new_uuid_string();
    let mut storage = StorageConfig {
        instance_uuid: instance_uuid.clone(),
        backend,
        milvus_lite: MilvusLiteConfig::for_uuid(&instance_uuid),
        milvus_remote: MilvusRemoteConfig::for_uuid(&instance_uuid, remote_uri),
    };
    storage.milvus_remote.token = remote_token;
    storage
}

pub(crate) fn normalize_storage(storage: &mut StorageConfig) {
    if storage.instance_uuid.trim().is_empty() {
        storage.instance_uuid = new_uuid_string();
    }
    if storage.milvus_lite.db_path.trim().is_empty() {
        storage.milvus_lite = MilvusLiteConfig::for_uuid(&storage.instance_uuid);
    }
    if storage.milvus_remote.database.trim().is_empty() {
        storage.milvus_remote.database =
            format!("agent_memory_{}", uuid_hex(&storage.instance_uuid));
    }
    if storage.milvus_remote.collection.trim().is_empty() {
        storage.milvus_remote.collection = DEFAULT_COLLECTION.to_string();
    }
}

pub(crate) fn normalize_storage_for_project(storage: &mut StorageConfig, project_root: &Path) {
    normalize_storage(storage);
    let slug = project_slug(project_root);
    let uuid = uuid_hex(&storage.instance_uuid);
    let default_lite = format!(".memory/milvus/{uuid}.db");
    if storage.milvus_lite.db_path == default_lite || storage.milvus_lite.db_path.trim().is_empty()
    {
        storage.milvus_lite.db_path = format!(".memory/milvus/{slug}-{uuid}.db");
    }
    let default_remote = format!("agent_memory_{uuid}");
    if storage.milvus_remote.database == default_remote
        || storage.milvus_remote.database.trim().is_empty()
    {
        storage.milvus_remote.database = format!("agent_memory_{slug}_{uuid}");
    }
}

fn project_slug(project_root: &Path) -> String {
    let raw = project_root
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("global");
    let mut slug = String::new();
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
        } else if !slug.ends_with('_') {
            slug.push('_');
        }
    }
    let slug = slug.trim_matches('_');
    if slug.is_empty() {
        "global".to_string()
    } else {
        slug.to_string()
    }
}

pub(crate) fn load_runtime_config(root: &Path) -> Result<RuntimeConfig> {
    let mut config: RuntimeConfig =
        serde_json::from_str(&fs::read_to_string(runtime_config_path(root))?)?;
    normalize_storage(&mut config.storage);
    Ok(config)
}

pub(crate) fn write_runtime_config(root: &Path, config: &RuntimeConfig) -> Result<()> {
    fs::create_dir_all(memory_dir(root))?;
    fs::write(
        runtime_config_path(root),
        serde_json::to_string_pretty(config)? + "\n",
    )?;
    Ok(())
}

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

pub(crate) fn default_backend() -> BackendKind {
    BackendKind::MilvusLite
}

pub(crate) fn default_lite_db_path() -> String {
    ".memory/milvus/default.db".to_string()
}

pub(crate) fn default_bridge() -> String {
    "python_process".to_string()
}

pub(crate) fn default_remote_database() -> String {
    format!("agent_memory_{}", uuid_hex(&new_uuid_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_uuid_backed_storage() {
        let config = UserConfig::default();
        assert_eq!(config.storage.backend, BackendKind::MilvusLite);
        assert!(config
            .storage
            .milvus_lite
            .db_path
            .contains(&uuid_hex(&config.storage.instance_uuid)));
        assert!(config
            .storage
            .milvus_remote
            .database
            .contains(&uuid_hex(&config.storage.instance_uuid)));
    }

    #[test]
    fn global_template_uses_home_runtime_memory_dir() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("memory.yaml");
        write_user_config_template(&path, "global", BackendKind::MilvusLite, None, false).unwrap();

        let config = load_user_config(&path).unwrap();
        assert_eq!(config.install_scope, "global");
        assert_eq!(config.memory_root, "~");
        assert_eq!(
            config.allowed_memory_types,
            vec!["environment".to_string(), "preference".to_string()]
        );
    }

    #[test]
    fn project_storage_names_include_project_slug() {
        let mut storage = StorageConfig::default();
        normalize_storage_for_project(&mut storage, Path::new("/tmp/My Project"));
        assert!(storage.milvus_lite.db_path.contains("my_project-"));
        assert!(storage.milvus_remote.database.contains("my_project_"));
    }
}
