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
    Qdrant,
    MilvusLite,
    MilvusRemote,
}

impl std::fmt::Display for BackendKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BackendKind::Qdrant => write!(f, "qdrant"),
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
    pub(crate) qdrant: QdrantConfig,
    #[serde(default)]
    pub(crate) milvus_lite: MilvusLiteConfig,
    #[serde(default)]
    pub(crate) milvus_remote: MilvusRemoteConfig,
}

impl Default for StorageConfig {
    fn default() -> Self {
        let instance_uuid = new_uuid_string();
        Self {
            qdrant: QdrantConfig::for_uuid(&instance_uuid),
            milvus_lite: MilvusLiteConfig::for_uuid(&instance_uuid),
            milvus_remote: MilvusRemoteConfig::for_uuid(&instance_uuid, None),
            instance_uuid,
            backend: BackendKind::Qdrant,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct QdrantConfig {
    #[serde(default = "default_qdrant_uri")]
    pub(crate) uri: String,
    #[serde(default = "default_qdrant_storage_path")]
    pub(crate) storage_path: String,
    #[serde(default = "default_qdrant_binary")]
    pub(crate) binary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) static_content_dir: Option<String>,
}

impl QdrantConfig {
    fn for_uuid(instance_uuid: &str) -> Self {
        Self {
            uri: default_qdrant_uri(),
            storage_path: format!(".memory/qdrant/{}", uuid_hex(instance_uuid)),
            binary: default_qdrant_binary(),
            static_content_dir: None,
        }
    }
}

impl Default for QdrantConfig {
    fn default() -> Self {
        Self {
            uri: default_qdrant_uri(),
            storage_path: default_qdrant_storage_path(),
            binary: default_qdrant_binary(),
            static_content_dir: None,
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
            collection: "memories".to_string(),
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
