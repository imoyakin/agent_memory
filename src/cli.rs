use clap::{ArgAction, Parser, Subcommand};
use std::path::PathBuf;

use crate::config::BackendKind;

#[derive(Parser)]
#[command(name = "agent-memory")]
pub(crate) struct Cli {
    #[arg(long)]
    pub(crate) root: Option<PathBuf>,
    #[arg(long, global = true, help = "Emit structured JSON for agent/tool use")]
    pub(crate) agent: bool,
    #[command(subcommand)]
    pub(crate) command: Commands,
}

#[derive(Subcommand)]
pub(crate) enum Commands {
    #[command(about = "Initialize runtime state from memory.yaml")]
    Init {
        #[arg(long)]
        config: Option<PathBuf>,
        #[arg(long)]
        provider: Option<String>,
        #[arg(long)]
        model: Option<String>,
        #[arg(long)]
        dim: Option<usize>,
        #[arg(long)]
        endpoint: Option<String>,
        #[arg(long)]
        collection: Option<String>,
        #[arg(long, value_enum)]
        backend: Option<BackendKind>,
        #[arg(long)]
        remote_uri: Option<String>,
        #[arg(long)]
        remote_token: Option<String>,
        #[arg(long)]
        verify_remote: bool,
        #[arg(long)]
        force: bool,
        #[arg(long = "no-update-agents", action = ArgAction::SetFalse, default_value_t = true)]
        update_agents: bool,
        #[arg(long)]
        start_service: bool,
    },
    #[command(about = "Create memory.yaml and optionally initialize runtime state")]
    Setup {
        #[arg(long)]
        target: Option<PathBuf>,
        #[arg(long, default_value = ".agents/agent_memory/memory.yaml")]
        config: PathBuf,
        #[arg(long, default_value = "project")]
        install_scope: String,
        #[arg(long, value_enum, default_value_t = BackendKind::MilvusLite)]
        backend: BackendKind,
        #[arg(long)]
        remote_uri: Option<String>,
        #[arg(long)]
        remote_token: Option<String>,
        #[arg(long)]
        verify_remote: bool,
        #[arg(long)]
        force_template: bool,
        #[arg(long = "no-update-agents", action = ArgAction::SetFalse, default_value_t = true)]
        update_agents: bool,
        #[arg(long)]
        init: bool,
        #[arg(long)]
        start_service: bool,
    },
    #[command(about = "List running agent-memory processes")]
    Ps,
    #[command(hide = true)]
    SetupConfig {
        #[arg(long, default_value = ".agents/agent_memory/memory.yaml")]
        output: PathBuf,
        #[arg(long, default_value = "project")]
        install_scope: String,
        #[arg(long, value_enum, default_value_t = BackendKind::MilvusLite)]
        backend: BackendKind,
        #[arg(long)]
        remote_uri: Option<String>,
        #[arg(long)]
        update_agents: bool,
        #[arg(long)]
        force: bool,
    },
    #[command(about = "Inspect and operate memory records")]
    Memory {
        #[command(subcommand)]
        command: MemoryCommands,
    },
    #[command(about = "Inspect and control the resident service")]
    Service {
        #[command(subcommand)]
        command: ServiceCommands,
    },
}

#[derive(Subcommand)]
pub(crate) enum MemoryCommands {
    #[command(about = "Show the active memory configuration")]
    Discover,
    #[command(about = "Add a durable memory record")]
    Add {
        #[arg(long)]
        content: String,
        #[arg(long = "type")]
        memory_type: String,
        #[arg(long)]
        source_kind: String,
        #[arg(long)]
        source_ref: String,
        #[arg(long)]
        confidence: f64,
        #[arg(long, default_value = "project")]
        scope: String,
        #[arg(long, default_value = "")]
        tags: String,
        #[arg(long, default_value = "")]
        keys: String,
    },
    #[command(about = "Search advisory memory")]
    Search {
        query: String,
        #[arg(long)]
        limit: Option<usize>,
        #[arg(long = "type")]
        memory_type: Option<String>,
        #[arg(long)]
        scope: Option<String>,
        #[arg(long, default_value = "")]
        tags: String,
    },
    #[command(about = "List memory records")]
    List {
        #[arg(long = "type")]
        memory_type: Option<String>,
        #[arg(long)]
        scope: Option<String>,
        #[arg(long, default_value = "")]
        tags: String,
    },
    #[command(about = "Get one memory record by id")]
    Get { memory_id: String },
    #[command(about = "Delete one memory record by id")]
    Delete { memory_id: String },
    #[command(about = "Delete all local memory state for this root")]
    Clear {
        #[arg(long)]
        yes: bool,
    },
    #[command(about = "Inspect memory and embedding health")]
    Audit,
    #[command(about = "Migrate records to another Milvus backend")]
    Migrate {
        #[arg(long, value_enum)]
        to_backend: BackendKind,
        #[arg(long)]
        remote_uri: Option<String>,
        #[arg(long)]
        remote_token: Option<String>,
        #[arg(long)]
        new_instance: bool,
        #[arg(long)]
        verify_remote: bool,
    },
    #[command(about = "Export records and runtime config")]
    Dump {
        #[arg(long)]
        output: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
pub(crate) enum ServiceCommands {
    #[command(about = "Start the resident memory service")]
    Start {
        #[arg(long)]
        config: Option<PathBuf>,
        #[arg(long = "agent-pid")]
        agent_pids: Vec<u32>,
        #[arg(long)]
        pid_check_interval: Option<u64>,
        #[arg(long)]
        worker_interval: Option<f64>,
        #[arg(long)]
        retry_failed: bool,
        #[arg(long, hide = true)]
        foreground: bool,
    },
    #[command(about = "Show resident service status")]
    Status,
    #[command(about = "Stop the resident service")]
    Stop {
        #[arg(long, default_value_t = 5)]
        timeout_seconds: u64,
    },
    #[command(hide = true)]
    Register {
        #[arg(long = "agent-pid", required = true)]
        agent_pids: Vec<u32>,
    },
    #[command(about = "Process pending embeddings")]
    Worker {
        #[arg(long)]
        once: bool,
        #[arg(long)]
        limit: Option<usize>,
        #[arg(long)]
        retry_failed: bool,
    },
    #[command(about = "Start, inspect, and stop local UI inspection helpers")]
    Ui {
        #[command(subcommand)]
        command: UiCommands,
    },
}

#[derive(Subcommand)]
pub(crate) enum UiCommands {
    #[command(about = "Expose local Milvus Lite on port 19530 for Attu")]
    Start {
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long, default_value_t = 19530)]
        port: u16,
        #[arg(long, default_value_t = 10)]
        max_workers: u16,
        #[arg(long)]
        stop_service: bool,
        #[arg(long, default_value_t = 5)]
        timeout_seconds: u64,
    },
    #[command(about = "Show the local UI inspection endpoint status")]
    Status,
    #[command(about = "Stop the local Milvus Lite UI inspection endpoint")]
    Stop {
        #[arg(long, default_value_t = 5)]
        timeout_seconds: u64,
    },
}
