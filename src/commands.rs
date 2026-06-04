use anyhow::{bail, Result};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use clap::Parser;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use uuid::Uuid;

use crate::cli::{
    AgentsHookCommands, Cli, Commands, GatewayCommands, MemoryCommands, ServiceCommands, UiCommands,
};
use crate::config::{
    load_runtime_config, load_user_config, logical_database_name, normalize_storage_for_runtime,
    persist_user_config_if_exists, write_runtime_config, write_user_config_template, RuntimeConfig,
    UserConfig,
};
use crate::discovery::{
    active_user_config, discover, discover_root, infer_project_root_from_config_path,
    remove_agents_config_pointer, resolve_memory_root, runtime_root, write_agents_config_pointer,
};
use crate::models::{GatewayState, MemoryRecord};
use crate::paths::{absolutize, home_path, project_path, HomePath, ProjectPath};
use crate::records::{
    delete_record, derive_keys, filtered_records, get_record, mark_accessed, read_records,
    record_value, summarize, upsert_record, validate_content,
};
use crate::search::{combined_score, exact_results, merge_search_results, round4, vector_results};
use crate::service::{
    default_agent_pids, detach_daemon, pid_exists, register_agent_pids, registry_entries,
    request_service_status, request_service_stop, request_service_worker, run_service_loop,
    service_status, spawn_service_daemon, wait_for_service_start, write_registry,
};
use crate::storage::{ensure_backend, qdrant_server_status};
use crate::util::{now, parse_csv, safe_timestamp};
use crate::{DEFAULT_COLLECTION, SCHEMA_VERSION};

pub fn run() -> Result<()> {
    let cli = Cli::parse();
    let agent_output = cli.agent;
    let result = match cli.command {
        Commands::Init {
            config,
            provider,
            model,
            dim,
            endpoint,
            collection,
            force,
            update_agents,
            start_service,
        } => cmd_init(InitOptions {
            root_arg: cli.root,
            config_path_arg: config,
            provider,
            model,
            dim,
            endpoint,
            collection,
            force,
            update_agents,
            start_service,
        })?,
        Commands::Setup {
            target,
            config,
            install_scope,
            force_template,
            update_agents,
            init,
            start_service,
        } => cmd_setup(SetupOptions {
            root_arg: cli.root.or(target),
            output: config,
            install_scope,
            force_template,
            update_agents,
            run_init: init,
            start_service,
        })?,
        Commands::Ps => cmd_ps()?,
        Commands::Gateway { command } => match command {
            GatewayCommands::Start {
                lease_seconds,
                heartbeat_seconds,
                host,
                port,
                foreground,
            } => cmd_gateway_start(lease_seconds, heartbeat_seconds, host, port, foreground)?,
            GatewayCommands::Status => cmd_gateway_status()?,
            GatewayCommands::Stop { timeout_seconds } => cmd_gateway_stop(timeout_seconds)?,
            GatewayCommands::Projects => cmd_gateway_projects()?,
            GatewayCommands::Attach { name, url, token } => cmd_gateway_attach(name, url, token)?,
            GatewayCommands::Detach { name } => cmd_gateway_detach(name)?,
            GatewayCommands::Remotes => cmd_gateway_remotes()?,
        },
        Commands::SetupConfig {
            output,
            install_scope,
            update_agents,
            force,
        } => cmd_setup_config(cli.root, output, install_scope, update_agents, force)?,
        Commands::AgentsHook { command } => match command {
            AgentsHookCommands::Install {
                config,
                install_scope,
            } => cmd_agents_hook_install(cli.root, config, install_scope)?,
            AgentsHookCommands::Remove => cmd_agents_hook_remove(cli.root)?,
        },
        Commands::Memory { command } => match command {
            MemoryCommands::Discover => cmd_discover(cli.root)?,
            MemoryCommands::Add {
                content,
                memory_type,
                source_kind,
                source_ref,
                confidence,
                scope,
                tags,
                keys,
            } => cmd_add(
                cli.root,
                content,
                memory_type,
                source_kind,
                source_ref,
                confidence,
                scope,
                tags,
                keys,
            )?,
            MemoryCommands::Search {
                query,
                limit,
                memory_type,
                scope,
                tags,
            } => cmd_search(cli.root, query, limit, memory_type, scope, tags)?,
            MemoryCommands::List {
                memory_type,
                scope,
                tags,
            } => cmd_list(cli.root, memory_type, scope, tags)?,
            MemoryCommands::Get { memory_id } => {
                let root = runtime_root(cli.root)?;
                let record = get_record(&root, &memory_id)?;
                json!({"ok": true, "record": record_value(&record)})
            }
            MemoryCommands::Delete { memory_id } => {
                let root = runtime_root(cli.root)?;
                let deleted = delete_record(&root, &memory_id)?;
                json!({"ok": true, "deleted": deleted, "memory_id": memory_id})
            }
            MemoryCommands::Clear { yes } => {
                if !yes {
                    bail!("clear requires --yes");
                }
                let root = runtime_root(cli.root)?;
                let dir = project_path(&root, ProjectPath::MemoryDir);
                if dir.exists() {
                    fs::remove_dir_all(&dir)?;
                }
                json!({"ok": true, "cleared": dir})
            }
            MemoryCommands::Audit => cmd_audit(cli.root)?,
            MemoryCommands::Dump { output } => cmd_dump(cli.root, output)?,
        },
        Commands::Service { command } => match command {
            ServiceCommands::Start {
                config,
                agent_pids,
                pid_check_interval,
                worker_interval,
                retry_failed,
                foreground,
            } => cmd_serve(
                cli.root,
                config,
                agent_pids,
                pid_check_interval,
                worker_interval,
                retry_failed,
                foreground,
            )?,
            ServiceCommands::Status => cmd_service_status(cli.root)?,
            ServiceCommands::Stop { timeout_seconds } => {
                cmd_service_stop(cli.root, timeout_seconds)?
            }
            ServiceCommands::Register { agent_pids } => cmd_service_register(cli.root, agent_pids)?,
            ServiceCommands::Worker {
                once: _,
                limit,
                retry_failed,
            } => cmd_service_worker(cli.root, limit, retry_failed)?,
            ServiceCommands::Ui { command } => match command {
                UiCommands::Start { host, port } => cmd_ui_start(cli.root, host, port)?,
                UiCommands::Status => cmd_ui_status(cli.root)?,
                UiCommands::Stop { timeout_seconds } => cmd_ui_stop(cli.root, timeout_seconds)?,
            },
        },
    };
    if agent_output {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        println!("{}", render_human(&result));
    }
    Ok(())
}

include!("commands/render_status.rs");
include!("commands/render_records.rs");
include!("commands/render_tables.rs");
include!("commands/processes.rs");
include!("commands/remotes.rs");
include!("commands/gateway.rs");
include!("commands/http.rs");
include!("commands/proxy.rs");
include!("commands/ui_registry.rs");
include!("commands/projects.rs");
include!("commands/setup.rs");
include!("commands/agents_hook.rs");
include!("commands/memory.rs");
include!("commands/service_commands.rs");
include!("commands/ui.rs");
include!("commands/tests.rs");
