mod cli;
mod commands;
mod config;
mod discovery;
mod embedding;
mod ipc;
mod models;
mod paths;
mod records;
mod search;
mod service;
mod storage;
mod util;
mod worker;

pub use commands::run;

pub(crate) const SCHEMA_VERSION: u32 = 2;
pub(crate) const DEFAULT_PROVIDER: &str = "ollama";
pub(crate) const DEFAULT_MODEL: &str = "qwen3-embedding:8b";
pub(crate) const DEFAULT_DIM: usize = 4096;
pub(crate) const DEFAULT_ENDPOINT: &str = "http://localhost:11434";
pub(crate) const DEFAULT_COLLECTION: &str = "agent_memory";
pub(crate) const MAX_CONTENT_BYTES: usize = 4096;
pub(crate) const PRIMARY_FIELD: &str = "uuid";
pub(crate) const VECTOR_FIELD: &str = "dense_vector";
pub(crate) const AGENTS_MARKER_START: &str = "<!-- agent-memory:config:start -->";
pub(crate) const AGENTS_MARKER_END: &str = "<!-- agent-memory:config:end -->";
