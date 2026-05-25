use anyhow::{bail, Result};
use clap::Parser;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use uuid::Uuid;

use crate::cli::{Cli, Commands, MemoryCommands, ServiceCommands, UiCommands};
use crate::config::{
    load_runtime_config, load_user_config, normalize_storage, normalize_storage_for_project,
    persist_storage_to_user_config, persist_user_config_if_exists, storage_for_backend,
    write_runtime_config, write_user_config_template, BackendKind, RuntimeConfig, UserConfig,
};
use crate::discovery::{
    active_user_config, discover, discover_root, infer_project_root_from_config_path,
    resolve_memory_root, runtime_root, write_agents_config_pointer,
};
use crate::models::MemoryRecord;
use crate::paths::{
    absolutize, memory_dir, milvus_lite_server_log_path, milvus_lite_server_state_path,
    resolve_under_root, runtime_config_path, skill_root, ui_viewer_registry_path,
};
use crate::records::{
    delete_record, derive_keys, filtered_records, get_record, mark_accessed, read_records,
    record_value, summarize, upsert_record, validate_content,
};
use crate::search::{combined_score, exact_results, merge_search_results, round4, vector_results};
use crate::service::{
    detach_daemon, pid_exists, register_agent_pids, registry_entries, request_service_status,
    request_service_stop, request_service_worker, run_service_loop, service_status,
    spawn_service_daemon, wait_for_service_start,
};
use crate::storage::{ensure_backend, read_records_from_backend, upsert_record_to_backend};
use crate::util::{now, parse_csv, safe_timestamp};
use crate::worker::cmd_worker;
use crate::SCHEMA_VERSION;

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
            backend,
            remote_uri,
            remote_token,
            verify_remote,
            force,
            update_agents,
            start_service,
        } => cmd_init(
            cli.root,
            config,
            provider,
            model,
            dim,
            endpoint,
            collection,
            backend,
            remote_uri,
            remote_token,
            verify_remote,
            force,
            update_agents,
            start_service,
        )?,
        Commands::Setup {
            target,
            config,
            install_scope,
            backend,
            remote_uri,
            remote_token,
            verify_remote,
            force_template,
            update_agents,
            init,
            start_service,
        } => cmd_setup(
            cli.root.or(target),
            config,
            install_scope,
            backend,
            remote_uri,
            remote_token,
            verify_remote,
            force_template,
            update_agents,
            init,
            start_service,
        )?,
        Commands::Ps => cmd_ps()?,
        Commands::SetupConfig {
            output,
            install_scope,
            backend,
            remote_uri,
            update_agents,
            force,
        } => cmd_setup_config(
            cli.root,
            output,
            install_scope,
            backend,
            remote_uri,
            update_agents,
            force,
        )?,
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
                let dir = memory_dir(&root);
                if dir.exists() {
                    fs::remove_dir_all(&dir)?;
                }
                json!({"ok": true, "cleared": dir})
            }
            MemoryCommands::Audit => cmd_audit(cli.root)?,
            MemoryCommands::Migrate {
                to_backend,
                remote_uri,
                remote_token,
                new_instance,
                verify_remote,
            } => cmd_migrate(
                cli.root,
                to_backend,
                remote_uri,
                remote_token,
                new_instance,
                verify_remote,
            )?,
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
                UiCommands::Start {
                    host,
                    port,
                    max_workers,
                    stop_service,
                    timeout_seconds,
                } => cmd_ui_start(
                    cli.root,
                    host,
                    port,
                    max_workers,
                    stop_service,
                    timeout_seconds,
                )?,
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

fn render_human(value: &Value) -> String {
    if let Some(processes) = value.get("processes").and_then(Value::as_array) {
        return render_processes(processes);
    }
    if let Some(results) = value.get("results").and_then(Value::as_array) {
        return render_search_results(value, results);
    }
    if let Some(records) = value.get("records").and_then(Value::as_array) {
        return render_records("Memory Records", records);
    }
    if let Some(record) = value.get("record").and_then(Value::as_object) {
        return render_record("Memory Record", &Value::Object(record.clone()));
    }
    if let Some(service) = value.get("service") {
        return render_service(service);
    }
    if let Some(worker) = value.get("worker") {
        return render_worker(worker);
    }
    if value.get("records_by_status").is_some() || value.get("embedding_queue").is_some() {
        return render_audit(value);
    }
    if let Some(discovery) = value.get("discovery") {
        return render_discovery(discovery);
    }
    if let Some(config) = value.get("config") {
        return render_config_result(value, config);
    }
    if value.get("server").is_some() || value.get("attu").is_some() {
        return render_ui(value);
    }
    if value.get("dump").is_some() {
        return format!("Dump written\n\n{}", key_value_table(value));
    }
    if value.get("records_migrated").is_some() {
        return format!("Migration complete\n\n{}", key_value_table(value));
    }
    if value.get("stopped").is_some() {
        return render_stop_result(value);
    }
    if value.get("deleted").is_some() {
        return render_delete_result(value);
    }
    if value.get("cleared").is_some() {
        return render_clear_result(value);
    }
    key_value_table(value)
}

fn render_processes(processes: &[Value]) -> String {
    let mut rows = Vec::new();
    for process in processes {
        rows.push(vec![
            value_text(&process["workdir"]),
            value_text(&process["root"]),
            value_text(&process["mode"]),
            value_text(&process["status"]),
            value_text(&process["memory_count"]),
            value_text(&process["pid"]),
        ]);
    }
    let table = markdown_table(
        &["Workdir", "Root", "Mode", "Status", "Memories", "PID"],
        &rows,
    );
    if rows.is_empty() {
        "No running agent-memory services found.".to_string()
    } else {
        format!("Agent Memory Processes\n\n{table}")
    }
}

fn render_search_results(value: &Value, results: &[Value]) -> String {
    let mut rows = Vec::new();
    for item in results {
        rows.push(vec![
            value_text(&item["memory_id"]),
            value_text(&item["memory_type"]),
            value_text(&item["scope"]),
            value_text(&item["score"]),
            value_text(&item["embedding_status"]),
            value_text(&item["summary"]),
        ]);
    }
    let query = value_text(&value["query"]);
    let mut output = if rows.is_empty() {
        format!("No memory matched: {query}")
    } else {
        format!(
            "Search Results\n\nQuery: {query}\n\n{}",
            markdown_table(
                &["ID", "Type", "Scope", "Score", "Embedding", "Summary"],
                &rows
            )
        )
    };
    if let Some(warnings) = value.get("warnings").and_then(Value::as_array) {
        if !warnings.is_empty() {
            output.push_str("\n\nWarnings\n");
            for warning in warnings {
                output.push_str(&format!("- {}\n", value_text(warning)));
            }
        }
    }
    output
}

fn render_records(title: &str, records: &[Value]) -> String {
    let mut rows = Vec::new();
    for record in records {
        rows.push(vec![
            value_text(&record["memory_id"]),
            value_text(&record["memory_type"]),
            value_text(&record["scope"]),
            value_text(&record["embedding_status"]),
            value_text(&record["updated_at"]),
            value_text(&record["summary"]),
        ]);
    }
    if rows.is_empty() {
        format!("{title}\n\nNo records.")
    } else {
        format!(
            "{title}\n\n{}",
            markdown_table(
                &["ID", "Type", "Scope", "Embedding", "Updated", "Summary"],
                &rows
            )
        )
    }
}

fn render_record(title: &str, record: &Value) -> String {
    let mut output = format!("{title}\n\n");
    output.push_str(&key_value_rows(&[
        ("ID", value_text(&record["memory_id"])),
        ("Type", value_text(&record["memory_type"])),
        ("Scope", value_text(&record["scope"])),
        ("Embedding", value_text(&record["embedding_status"])),
        ("Confidence", value_text(&record["confidence"])),
        ("Updated", value_text(&record["updated_at"])),
        ("Source", value_text(&record["source_ref"])),
        ("Summary", value_text(&record["summary"])),
    ]));
    if let Some(content) = record.get("content").and_then(Value::as_str) {
        output.push_str("\n\nContent\n\n");
        output.push_str(content);
    }
    output
}

fn render_service(service: &Value) -> String {
    let state = service.get("state").unwrap_or(&Value::Null);
    let rows = vec![
        ("Active", value_text(&service["active"])),
        ("PID", value_text(&service["service_pid"])),
        ("Root", value_text(&service["root"])),
        ("Mode", value_text(&state["install_scope"])),
        ("Workdir", value_text(&state["workdir"])),
        ("Memory Count", value_text(&state["memory_count"])),
        ("Updated", value_text(&state["updated_at"])),
        (
            "IPC",
            value_text(
                &state
                    .pointer("/ipc/address")
                    .cloned()
                    .unwrap_or(Value::Null),
            ),
        ),
        ("Last Worker Error", value_text(&state["last_worker_error"])),
    ];
    format!("Service Status\n\n{}", key_value_rows(&rows))
}

fn render_worker(worker: &Value) -> String {
    let output = format!(
        "Worker Run\n\n{}",
        key_value_rows(&[
            ("Processed", value_text(&worker["processed"])),
            ("Embedded", value_text(&worker["embedded"])),
            ("Failed", value_text(&worker["failed"])),
        ])
    );
    if let Some(failures) = worker.get("failures").and_then(Value::as_array) {
        if failures.is_empty() {
            output
        } else {
            format!(
                "{output}\n\nFailures\n\n{}",
                key_value_table(&json!({ "failures": failures }))
            )
        }
    } else {
        output
    }
}

fn render_audit(value: &Value) -> String {
    let mut output = format!(
        "Memory Audit\n\n{}",
        key_value_rows(&[
            ("Root", value_text(&value["root"])),
            ("Records", value_text(&value["records"])),
        ])
    );
    output.push_str("\n\nBy Embedding Status\n\n");
    output.push_str(&object_count_table(value.get("records_by_status")));
    output.push_str("\n\nBy Memory Type\n\n");
    output.push_str(&object_count_table(value.get("records_by_type")));
    output.push_str("\n\nEmbedding Queue\n\n");
    output.push_str(&object_count_table(value.get("embedding_queue")));
    output
}

fn render_discovery(discovery: &Value) -> String {
    if discovery.get("found").and_then(Value::as_bool) == Some(false) {
        return format!("No memory config found.\n\n{}", key_value_table(discovery));
    }
    format!(
        "Active Memory Config\n\n{}",
        key_value_rows(&[
            ("Project Root", value_text(&discovery["project_root"])),
            ("Runtime Root", value_text(&discovery["runtime_root"])),
            ("Config", value_text(&discovery["config_path"])),
            ("Source", value_text(&discovery["source"])),
            ("Install Scope", value_text(&discovery["install_scope"])),
            (
                "Backend",
                value_text(
                    &discovery
                        .pointer("/storage/backend")
                        .cloned()
                        .unwrap_or(Value::Null)
                )
            ),
        ])
    )
}

fn render_config_result(value: &Value, config: &Value) -> String {
    format!(
        "Initialized Memory Runtime\n\n{}",
        key_value_rows(&[
            ("Project Root", value_text(&value["project_root"])),
            ("Root", value_text(&value["root"])),
            ("Config Path", value_text(&value["config_path"])),
            ("AGENTS.md", value_text(&value["agents_file"])),
            ("Install Scope", value_text(&config["install_scope"])),
            ("Collection", value_text(&config["collection_name"])),
            (
                "Embedding",
                format!(
                    "{} / {} / {}",
                    value_text(&config["embedding_provider"]),
                    value_text(&config["embedding_model"]),
                    value_text(&config["embedding_dim"])
                )
            ),
            (
                "Backend",
                value_text(
                    &config
                        .pointer("/storage/backend")
                        .cloned()
                        .unwrap_or(Value::Null)
                )
            ),
        ])
    )
}

fn render_ui(value: &Value) -> String {
    let server = value.get("server").unwrap_or(&Value::Null);
    format!(
        "UI Inspection Helper\n\n{}",
        key_value_rows(&[
            ("Active", value_text(&server["active"])),
            ("PID", value_text(&server["pid"])),
            ("Endpoint", value_text(&server["endpoint"])),
            ("Database", value_text(&server["database_name"])),
            ("Data Dir", value_text(&server["data_dir"])),
            ("Log", value_text(&server["log_path"])),
            (
                "Attu",
                value_text(
                    &value
                        .pointer("/attu/address")
                        .cloned()
                        .unwrap_or(Value::Null)
                )
            ),
        ])
    )
}

fn render_stop_result(value: &Value) -> String {
    let service = value.pointer("/ipc/service").unwrap_or(&Value::Null);
    format!(
        "Service Stop\n\n{}",
        key_value_rows(&[
            ("Stopped", value_text(&value["stopped"])),
            ("Root", value_text(&service["root"])),
        ])
    )
}

fn render_delete_result(value: &Value) -> String {
    format!(
        "Memory Delete\n\n{}",
        key_value_rows(&[
            ("Deleted", value_text(&value["deleted"])),
            ("ID", value_text(&value["memory_id"])),
        ])
    )
}

fn render_clear_result(value: &Value) -> String {
    format!(
        "Memory State Cleared\n\n{}",
        key_value_rows(&[("Path", value_text(&value["cleared"]))])
    )
}

fn object_count_table(value: Option<&Value>) -> String {
    let mut rows = Vec::new();
    if let Some(object) = value.and_then(Value::as_object) {
        for (key, count) in object {
            rows.push(vec![key.clone(), value_text(count)]);
        }
    }
    if rows.is_empty() {
        "No entries.".to_string()
    } else {
        markdown_table(&["Name", "Count"], &rows)
    }
}

fn key_value_table(value: &Value) -> String {
    if let Some(object) = value.as_object() {
        let rows: Vec<_> = object
            .iter()
            .map(|(key, value)| (key.as_str(), value_text(value)))
            .collect();
        key_value_rows(&rows)
    } else {
        value_text(value)
    }
}

fn key_value_rows(rows: &[(&str, String)]) -> String {
    let table_rows: Vec<Vec<String>> = rows
        .iter()
        .filter(|(_, value)| !value.is_empty())
        .map(|(key, value)| vec![(*key).to_string(), value.clone()])
        .collect();
    markdown_table(&["Field", "Value"], &table_rows)
}

fn markdown_table(headers: &[&str], rows: &[Vec<String>]) -> String {
    let mut widths: Vec<usize> = headers.iter().map(|header| display_len(header)).collect();
    for row in rows {
        for (index, cell) in row.iter().enumerate() {
            if let Some(width) = widths.get_mut(index) {
                *width = (*width).max(display_len(&truncate_cell(cell)));
            }
        }
    }
    let mut output = String::new();
    output.push('|');
    for (index, header) in headers.iter().enumerate() {
        output.push(' ');
        output.push_str(&pad_cell(header, widths[index]));
        output.push_str(" |");
    }
    output.push('\n');
    output.push('|');
    for width in &widths {
        output.push(' ');
        output.push_str(&"-".repeat(*width));
        output.push_str(" |");
    }
    output.push('\n');
    for row in rows {
        output.push('|');
        for (index, width) in widths.iter().enumerate().take(headers.len()) {
            let cell = row.get(index).map(String::as_str).unwrap_or("");
            output.push(' ');
            output.push_str(&pad_cell(&truncate_cell(cell), *width));
            output.push_str(" |");
        }
        output.push('\n');
    }
    output.trim_end().to_string()
}

fn pad_cell(value: &str, width: usize) -> String {
    let mut output = value.to_string();
    while display_len(&output) < width {
        output.push(' ');
    }
    output
}

fn truncate_cell(value: &str) -> String {
    const MAX: usize = 80;
    if display_len(value) <= MAX {
        value.to_string()
    } else {
        let mut output: String = value.chars().take(MAX.saturating_sub(3)).collect();
        output.push_str("...");
        output
    }
}

fn display_len(value: &str) -> usize {
    value.chars().count()
}

fn value_text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::String(value) => value.clone(),
        Value::Array(items) => items.iter().map(value_text).collect::<Vec<_>>().join(", "),
        Value::Object(_) => serde_json::to_string(value).unwrap_or_default(),
    }
}

fn cmd_ps() -> Result<Value> {
    let mut entries = Vec::new();
    for entry in registry_entries()? {
        let root = PathBuf::from(&entry.root);
        let response = crate::ipc::request(&entry.ipc, json!({"method": "status"}));
        let (status, service) = if let Ok(response) = response {
            (
                "running",
                response.get("service").cloned().unwrap_or(Value::Null),
            )
        } else {
            ("stale", Value::Null)
        };
        let memory_count = service
            .get("state")
            .and_then(|state| state.get("memory_count"))
            .and_then(Value::as_u64)
            .map(|value| value as usize)
            .or(entry.memory_count);
        let detail = json!({
            "workdir": entry.workdir,
            "root": root,
            "mode": entry.mode,
            "status": status,
            "memory_count": memory_count,
            "pid": entry.pid,
            "ipc": entry.ipc,
            "service": service,
        });
        let workdir_text = detail["workdir"].as_str().unwrap_or_default().to_string();
        let list_row = json!([
            workdir_text,
            detail["root"],
            detail["mode"],
            detail["status"],
            detail["memory_count"],
            detail["pid"]
        ]);
        entries.push((workdir_text, entry.pid, detail, list_row));
    }
    entries.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
    let count = entries.len();
    let rows: Vec<_> = entries.iter().map(|entry| entry.2.clone()).collect();
    let list: Vec<_> = entries.into_iter().map(|entry| entry.3).collect();
    Ok(json!({
        "ok": true,
        "count": count,
        "columns": ["workdir", "root", "mode", "status", "memory_count", "pid"],
        "list": list,
        "processes": rows,
    }))
}

#[allow(clippy::too_many_arguments)]
fn cmd_init(
    root_arg: Option<PathBuf>,
    config_path_arg: Option<PathBuf>,
    provider: Option<String>,
    model: Option<String>,
    dim: Option<usize>,
    endpoint: Option<String>,
    collection: Option<String>,
    backend: Option<BackendKind>,
    remote_uri: Option<String>,
    remote_token: Option<String>,
    verify_remote: bool,
    force: bool,
    update_agents: bool,
    start_service: bool,
) -> Result<Value> {
    let (project_root, root, config_path, mut user_config) = if let Some(path) = config_path_arg {
        let config_path = absolutize(path)?;
        let project_root = root_arg
            .map(absolutize)
            .transpose()?
            .unwrap_or_else(|| infer_project_root_from_config_path(&config_path));
        let user_config = load_user_config(&config_path)?;
        (
            project_root.clone(),
            resolve_memory_root(&user_config, &config_path, &project_root)?,
            config_path,
            user_config,
        )
    } else if let Some(discovered) = discover(root_arg.as_deref())? {
        let user_config = load_user_config(&discovered.config_path)?;
        (
            discovered.project_root.clone(),
            resolve_memory_root(
                &user_config,
                &discovered.config_path,
                &discovered.project_root,
            )?,
            discovered.config_path,
            user_config,
        )
    } else {
        let root = discover_root(root_arg.as_deref())?;
        let mut user_config = UserConfig::default();
        if let Some(backend) = backend {
            user_config.storage =
                storage_for_backend(backend, remote_uri.clone(), remote_token.clone(), true);
        }
        (
            root.clone(),
            root.clone(),
            root.join("memory.yaml"),
            user_config,
        )
    };

    if let Some(provider) = provider {
        user_config.embedding.provider = provider;
    }
    if let Some(model) = model {
        user_config.embedding.model = model;
    }
    if let Some(dim) = dim {
        user_config.embedding.dim = dim;
    }
    if endpoint.is_some() {
        user_config.embedding.endpoint = endpoint;
    }
    if let Some(collection) = collection {
        user_config.collection_name = collection.clone();
        user_config.storage.milvus_remote.collection = collection;
    }
    if backend.is_some() || remote_uri.is_some() || remote_token.is_some() {
        user_config.storage = storage_for_backend(
            backend.unwrap_or(user_config.storage.backend),
            remote_uri.or(user_config.storage.milvus_remote.uri.clone()),
            remote_token.or(user_config.storage.milvus_remote.token.clone()),
            false,
        );
    }
    normalize_storage(&mut user_config.storage);
    let default_lite_path = user_config.storage.milvus_lite.db_path.clone();
    if !resolve_under_root(&root, &default_lite_path).exists() {
        normalize_storage_for_project(&mut user_config.storage, &project_root);
    }
    if config_path.exists() {
        persist_user_config_if_exists(&config_path, &user_config)?;
    } else {
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&config_path, serde_yaml::to_string(&user_config)?)?;
    }

    let runtime_path = runtime_config_path(&root);
    if runtime_path.exists() && !force {
        let mut runtime = load_runtime_config(&root)?;
        runtime.schema_version = SCHEMA_VERSION;
        runtime.retrieval = user_config.retrieval.clone();
        runtime.storage = user_config.storage.clone();
        runtime.allowed_memory_types = user_config.allowed_memory_types.clone();
        write_runtime_config(&root, &runtime)?;
        ensure_backend(&root, &runtime, verify_remote)?;
        let agents_file = if update_agents {
            Some(write_agents_config_pointer(&project_root, &config_path)?)
        } else {
            None
        };
        let mut result = json!({
            "ok": true,
            "root": root,
            "project_root": project_root,
            "config": runtime,
            "config_path": config_path,
            "agents_file": agents_file,
        });
        if start_service {
            result["service_start"] = cmd_serve(
                Some(project_root.clone()),
                Some(config_path.clone()),
                Vec::new(),
                None,
                None,
                false,
                false,
            )?;
        }
        return Ok(result);
    }

    let runtime = RuntimeConfig {
        schema_version: SCHEMA_VERSION,
        root_path: root.to_string_lossy().to_string(),
        collection_name: user_config.collection_name.clone(),
        embedding_provider: user_config.embedding.provider.clone(),
        embedding_model: user_config.embedding.model.clone(),
        embedding_dim: user_config.embedding.dim,
        embedding_endpoint: user_config.embedding.endpoint.clone(),
        created_at: now(),
        install_scope: user_config.install_scope.clone(),
        allowed_memory_types: user_config.allowed_memory_types.clone(),
        retrieval: user_config.retrieval.clone(),
        storage: user_config.storage.clone(),
    };
    write_runtime_config(&root, &runtime)?;
    ensure_backend(&root, &runtime, verify_remote)?;
    let agents_file = if update_agents {
        Some(write_agents_config_pointer(&project_root, &config_path)?)
    } else {
        None
    };
    let mut result = json!({
        "ok": true,
        "root": root,
        "project_root": project_root,
        "config": runtime,
        "config_path": config_path,
        "agents_file": agents_file,
    });
    if start_service {
        result["service_start"] = cmd_serve(
            Some(project_root.clone()),
            Some(config_path.clone()),
            Vec::new(),
            None,
            None,
            false,
            false,
        )?;
    }
    Ok(result)
}

#[allow(clippy::too_many_arguments)]
fn cmd_setup(
    root_arg: Option<PathBuf>,
    output: PathBuf,
    install_scope: String,
    backend: BackendKind,
    remote_uri: Option<String>,
    remote_token: Option<String>,
    verify_remote: bool,
    force_template: bool,
    update_agents: bool,
    run_init: bool,
    start_service: bool,
) -> Result<Value> {
    let root = discover_root(root_arg.as_deref())?;
    let output = if output.is_absolute() {
        output
    } else {
        root.join(output)
    };
    let sync = run_uv_sync()?;
    let created_config = if output.exists() && !force_template {
        false
    } else {
        write_user_config_template(
            &output,
            &install_scope,
            backend,
            remote_uri.clone(),
            force_template,
        )?;
        let mut user_config = load_user_config(&output)?;
        normalize_storage_for_project(&mut user_config.storage, &root);
        fs::write(&output, serde_yaml::to_string(&user_config)?)?;
        true
    };
    let agents_file = if update_agents {
        Some(write_agents_config_pointer(&root, &output)?)
    } else {
        None
    };
    let init_result = if run_init || start_service {
        Some(cmd_init(
            Some(root.clone()),
            Some(output.clone()),
            None,
            None,
            None,
            None,
            None,
            Some(backend),
            remote_uri,
            remote_token,
            verify_remote,
            false,
            update_agents,
            start_service,
        )?)
    } else {
        None
    };
    let has_init_result = init_result.is_some();
    Ok(json!({
        "ok": true,
        "root": root,
        "config_path": output,
        "created_config": created_config,
        "agents_file": agents_file,
        "uv_sync": sync,
        "init": init_result,
        "next": if has_init_result { Value::Null } else { json!("agent-memory init") }
    }))
}

fn run_uv_sync() -> Result<Value> {
    let status = Command::new("uv")
        .arg("sync")
        .arg("--project")
        .arg(skill_root()?)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    match status {
        Ok(status) if status.success() => Ok(json!({"ok": true})),
        Ok(status) => bail!("uv sync failed with status {status}"),
        Err(error) => bail!("failed to run uv sync: {error}"),
    }
}

fn cmd_setup_config(
    root_arg: Option<PathBuf>,
    output: PathBuf,
    install_scope: String,
    backend: BackendKind,
    remote_uri: Option<String>,
    update_agents: bool,
    force: bool,
) -> Result<Value> {
    let root = discover_root(root_arg.as_deref())?;
    let output = if output.is_absolute() {
        output
    } else {
        root.join(output)
    };
    write_user_config_template(&output, &install_scope, backend, remote_uri, force)?;
    let mut user_config = load_user_config(&output)?;
    normalize_storage_for_project(&mut user_config.storage, &root);
    fs::write(&output, serde_yaml::to_string(&user_config)?)?;
    let agents_file = if update_agents {
        Some(write_agents_config_pointer(&root, &output)?)
    } else {
        None
    };
    Ok(json!({
        "ok": true,
        "root": root,
        "config_path": output,
        "agents_file": agents_file,
        "next": format!("agent-memory --root {} init --config {}", root.display(), output.display())
    }))
}

fn cmd_discover(root_arg: Option<PathBuf>) -> Result<Value> {
    if let Some(discovered) = discover(root_arg.as_deref())? {
        let user_config = load_user_config(&discovered.config_path)?;
        let runtime_root = resolve_memory_root(
            &user_config,
            &discovered.config_path,
            &discovered.project_root,
        )?;
        Ok(json!({
            "ok": true,
            "discovery": {
                "found": true,
                "project_root": discovered.project_root,
                "config_path": discovered.config_path,
                "source": discovered.source,
                "runtime_root": runtime_root,
                "install_scope": user_config.install_scope,
                "storage": user_config.storage,
            }
        }))
    } else {
        let root = discover_root(root_arg.as_deref())?;
        Ok(json!({
            "ok": true,
            "discovery": {
                "found": false,
                "project_root": root,
                "guidance": "No memory.yaml or .memory/ was found. Run `agent-memory setup`, edit memory.yaml, then run `agent-memory setup --init`."
            }
        }))
    }
}

#[allow(clippy::too_many_arguments)]
fn cmd_add(
    root_arg: Option<PathBuf>,
    content: String,
    memory_type: String,
    source_kind: String,
    source_ref: String,
    confidence: f64,
    scope: String,
    tags: String,
    keys: String,
) -> Result<Value> {
    validate_content(&content)?;
    if !(0.0..=1.0).contains(&confidence) {
        bail!("confidence must be between 0.0 and 1.0");
    }
    let root = runtime_root(root_arg)?;
    let config = load_runtime_config(&root)?;
    if !config.allowed_memory_types.contains(&memory_type) {
        bail!("memory_type '{}' is disabled by this config", memory_type);
    }
    if config.install_scope == "global" && scope != "global" {
        bail!("global agent-memory requires --scope global");
    }
    let tags = parse_csv(&tags);
    let mut keys = parse_csv(&keys);
    if keys.is_empty() {
        keys = derive_keys(&content, &source_ref, &tags);
    }
    let timestamp = now();
    let record = MemoryRecord {
        uuid: Uuid::new_v4().to_string(),
        content,
        keys,
        summary: summarize(&source_ref, &timestamp, &memory_type), // overwritten below
        embedding_status: "pending".to_string(),
        embedding_error: None,
        embedding_attempts: 0,
        memory_type,
        scope,
        root_path: root.to_string_lossy().to_string(),
        tags,
        source_kind,
        source_ref,
        created_at: timestamp.clone(),
        updated_at: timestamp.clone(),
        last_accessed_at: None,
        access_count: 0,
        conflict_count: 0,
        confidence,
        verified_at: None,
        stale_after_days: None,
        embedding_provider: config.embedding_provider.clone(),
        embedding_model: config.embedding_model.clone(),
        embedding_dim: config.embedding_dim,
        schema_version: SCHEMA_VERSION,
    }
    .with_summary();
    ensure_backend(&root, &config, false)?;
    upsert_record(&root, &record, None)?;
    Ok(json!({"ok": true, "record": record_value(&record)}))
}

fn cmd_search(
    root_arg: Option<PathBuf>,
    query: String,
    limit: Option<usize>,
    memory_type: Option<String>,
    scope: Option<String>,
    tags: String,
) -> Result<Value> {
    let root = runtime_root(root_arg)?;
    let config = load_runtime_config(&root)?;
    let required_tags = parse_csv(&tags);
    let records = filtered_records(
        &root,
        memory_type.as_deref(),
        scope.as_deref(),
        &required_tags,
    )?;
    let direct_limit = limit.unwrap_or(config.retrieval.default_limit).max(1);
    let vector_limit = direct_limit
        + if config.retrieval.associative.enabled {
            config.retrieval.associative.limit
        } else {
            0
        }
        + 8;
    let mut warnings = Vec::new();
    let mut direct = exact_results(&records, &query);
    if records
        .iter()
        .any(|record| record.embedding_status == "embedded")
    {
        match vector_results(&root, &config, &records, &query, vector_limit) {
            Ok(vector_matches) => merge_search_results(&mut direct, vector_matches),
            Err(error) => warnings.push(format!("vector search unavailable: {error}")),
        }
    }
    direct.truncate(direct_limit);
    let mut results = direct.clone();
    if config.retrieval.associative.enabled && config.retrieval.associative.limit > 0 {
        let selected: HashSet<_> = direct.iter().map(|item| item.uuid.clone()).collect();
        let seed = direct
            .iter()
            .flat_map(|item| item.matched_keys.iter().chain(item.keys.iter()))
            .take(16)
            .cloned()
            .collect::<Vec<_>>()
            .join(" ");
        if !seed.is_empty() {
            let candidates: Vec<_> = records
                .iter()
                .filter(|record| !selected.contains(&record.uuid))
                .cloned()
                .collect();
            let mut associative = exact_results(&candidates, &seed);
            for result in &mut associative {
                result.match_reasons.retain(|reason| reason == "pending");
                result
                    .match_reasons
                    .insert(0, "associative_keys".to_string());
                result.relevance = round4(result.relevance * 0.92);
                result.score = combined_score(result.relevance, result.reliability);
            }
            associative.retain(|item| item.score >= config.retrieval.associative.min_score);
            associative.truncate(config.retrieval.associative.limit);
            results.extend(associative);
        }
    }
    mark_accessed(
        &root,
        results.iter().map(|item| item.uuid.clone()).collect(),
    )?;
    Ok(json!({"ok": true, "query": query, "warnings": warnings, "results": results}))
}

fn cmd_list(
    root_arg: Option<PathBuf>,
    memory_type: Option<String>,
    scope: Option<String>,
    tags: String,
) -> Result<Value> {
    let root = runtime_root(root_arg)?;
    let tags = parse_csv(&tags);
    let records = filtered_records(&root, memory_type.as_deref(), scope.as_deref(), &tags)?;
    Ok(json!({"ok": true, "records": records.iter().map(record_value).collect::<Vec<_>>()}))
}

fn cmd_audit(root_arg: Option<PathBuf>) -> Result<Value> {
    let root = runtime_root(root_arg)?;
    let records = read_records(&root)?;
    let mut records_by_status: HashMap<String, usize> = HashMap::new();
    let mut records_by_type: HashMap<String, usize> = HashMap::new();
    let mut embedding_queue: HashMap<String, usize> = HashMap::new();
    for record in &records {
        *records_by_status
            .entry(record.embedding_status.clone())
            .or_default() += 1;
        *records_by_type
            .entry(record.memory_type.clone())
            .or_default() += 1;
        if matches!(
            record.embedding_status.as_str(),
            "pending" | "embedding" | "failed"
        ) {
            *embedding_queue
                .entry(record.embedding_status.clone())
                .or_default() += 1;
        }
    }
    let config = load_runtime_config(&root)?;
    Ok(json!({
        "ok": true,
        "root": root,
        "storage": config.storage,
        "records": records.len(),
        "records_by_status": records_by_status,
        "records_by_type": records_by_type,
        "embedding_queue": embedding_queue
    }))
}

fn cmd_serve(
    root_arg: Option<PathBuf>,
    config_arg: Option<PathBuf>,
    agent_pids: Vec<u32>,
    pid_check_interval: Option<u64>,
    worker_interval: Option<f64>,
    retry_failed: bool,
    foreground: bool,
) -> Result<Value> {
    let (project_root, config_path, user_config) =
        active_user_config(root_arg.clone(), config_arg)?;
    let runtime_root = resolve_memory_root(&user_config, &config_path, &project_root)?;
    let live_agent_pids: Vec<u32> = agent_pids
        .into_iter()
        .filter(|pid| *pid > 0 && pid_exists(*pid))
        .collect();
    cmd_init(
        Some(project_root.clone()),
        Some(config_path.clone()),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        false,
        false,
        true,
        false,
    )?;
    let root = runtime_root;
    if service_status(&root)?
        .get("active")
        .and_then(Value::as_bool)
        == Some(true)
    {
        return register_agent_pids(&root, &live_agent_pids);
    }
    if !foreground {
        let mut daemon = spawn_service_daemon(
            &project_root,
            &config_path,
            &root,
            &live_agent_pids,
            pid_check_interval,
            worker_interval,
            retry_failed,
        )?;
        let service = wait_for_service_start(
            &root,
            &mut daemon.child,
            Duration::from_secs(5),
            &daemon.log_path,
        )?;
        return Ok(json!({
            "ok": true,
            "daemonized": true,
            "service_pid": daemon.pid,
            "log_path": daemon.log_path,
            "service": service
        }));
    }
    run_service_loop(
        root,
        config_path,
        user_config,
        live_agent_pids,
        pid_check_interval,
        worker_interval,
        retry_failed,
    )
}

fn cmd_service_status(root_arg: Option<PathBuf>) -> Result<Value> {
    let root = runtime_root(root_arg)?;
    request_service_status(&root)
}

fn cmd_service_stop(root_arg: Option<PathBuf>, timeout_seconds: u64) -> Result<Value> {
    let root = runtime_root(root_arg)?;
    if let Ok(Some(state)) = crate::service::read_service_state(&root) {
        if let Some(endpoint) = state.ipc {
            if let Ok(response) = crate::ipc::request(&endpoint, json!({"method": "stop"})) {
                let deadline = Instant::now() + Duration::from_secs(timeout_seconds.max(1));
                while Instant::now() < deadline {
                    if request_service_status(&root)
                        .ok()
                        .and_then(|value| value.pointer("/service/active").and_then(Value::as_bool))
                        == Some(false)
                    {
                        return Ok(json!({"ok": true, "stopped": true, "ipc": response}));
                    }
                    thread::sleep(Duration::from_millis(100));
                }
            }
        }
    }
    request_service_stop(&root, Duration::from_secs(timeout_seconds.max(1)))
}

fn cmd_service_worker(
    root_arg: Option<PathBuf>,
    limit: Option<usize>,
    retry_failed: bool,
) -> Result<Value> {
    let root = runtime_root(root_arg)?;
    request_service_worker(&root, limit, retry_failed)
}

fn cmd_service_register(root_arg: Option<PathBuf>, agent_pids: Vec<u32>) -> Result<Value> {
    let root = runtime_root(root_arg)?;
    register_agent_pids(&root, &agent_pids)
}

fn cmd_migrate(
    root_arg: Option<PathBuf>,
    to_backend: BackendKind,
    remote_uri: Option<String>,
    remote_token: Option<String>,
    new_instance: bool,
    verify_remote: bool,
) -> Result<Value> {
    let root = runtime_root(root_arg)?;
    let mut config = load_runtime_config(&root)?;
    let mut records = read_records_from_backend(&root, &config)?;
    let old_storage = config.storage.clone();
    let mut new_storage = storage_for_backend(
        to_backend,
        remote_uri.or_else(|| old_storage.milvus_remote.uri.clone()),
        remote_token.or_else(|| old_storage.milvus_remote.token.clone()),
        new_instance,
    );
    if !new_instance {
        new_storage.instance_uuid = config.storage.instance_uuid.clone();
        normalize_storage(&mut new_storage);
    }
    config.storage = new_storage;
    ensure_backend(&root, &config, verify_remote)?;
    write_runtime_config(&root, &config)?;
    let timestamp = now();
    for record in &mut records {
        record.embedding_status = "pending".to_string();
        record.embedding_error = None;
        record.embedding_attempts = 0;
        record.embedding_provider = config.embedding_provider.clone();
        record.embedding_model = config.embedding_model.clone();
        record.embedding_dim = config.embedding_dim;
        record.updated_at = timestamp.clone();
        upsert_record_to_backend(&root, &config, record, None)?;
    }
    let worker = cmd_worker(Some(root.clone()), None, true)?;
    persist_storage_to_user_config(&root, &config.storage)?;
    Ok(
        json!({"ok": true, "from": old_storage, "to": config.storage, "records_migrated": records.len(), "worker": worker["worker"].clone()}),
    )
}

fn cmd_milvus_lite_server(
    root_arg: Option<PathBuf>,
    host: String,
    port: u16,
    max_workers: u16,
    stop_service: bool,
    timeout_seconds: u64,
) -> Result<Value> {
    let root = runtime_root(root_arg)?;
    let config = load_runtime_config(&root)?;
    if config.storage.backend != BackendKind::MilvusLite {
        bail!(
            "milvus-lite-server only applies to milvus_lite storage; current backend is {}",
            config.storage.backend
        );
    }

    let current = milvus_lite_server_status_value(&root)?;
    if current.get("active").and_then(Value::as_bool) == Some(true) {
        return Ok(json!({"ok": true, "already_running": true, "server": current}));
    }

    let active_service = service_status(&root)?
        .get("active")
        .and_then(Value::as_bool)
        == Some(true);
    let service_stop = if active_service {
        if !stop_service {
            bail!(
                "agent-memory service is active for {}; stop it first or rerun with --stop-service because Milvus Lite allows one writer/server for the same data directory",
                root.display()
            );
        }
        Some(request_service_stop(
            &root,
            Duration::from_secs(timeout_seconds.max(1)),
        )?)
    } else {
        None
    };

    ensure_backend(&root, &config, false)?;
    fs::create_dir_all(memory_dir(&root))?;
    let data_dir = resolve_under_root(&root, &config.storage.milvus_lite.db_path);
    let log_path = milvus_lite_server_log_path(&root);
    let closed_viewers = stop_other_ui_viewers(&root, &host, port, timeout_seconds)?;
    if TcpStream::connect((attu_connect_host(&host), port)).is_ok() {
        bail!(
            "port {}:{} is already accepting connections; choose another --port or stop the existing Milvus server",
            attu_connect_host(&host),
            port
        );
    }
    let stdout = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;
    let stderr = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;

    let mut command = milvus_lite_server_command(&data_dir, &host, port, max_workers)?;
    command
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    detach_daemon(&mut command);

    let mut child = command.spawn()?;
    let pid = child.id();
    let endpoint = format!("http://{}:{port}", attu_connect_host(&host));
    let started_at = now();
    let state = json!({
        "pid": pid,
        "host": host,
        "port": port,
        "endpoint": endpoint,
        "data_dir": data_dir,
        "database_name": milvus_lite_display_name(&root, &data_dir),
        "log_path": log_path,
        "started_at": started_at,
        "updated_at": started_at,
    });
    write_milvus_lite_server_state(&root, &state)?;
    register_ui_viewer(&root, &state)?;
    wait_for_milvus_lite_server_start(
        &mut child,
        &host,
        port,
        Duration::from_secs(timeout_seconds.max(1)),
        &log_path,
    )?;

    Ok(json!({
        "ok": true,
        "server": milvus_lite_server_status_value(&root)?,
        "service_stop": service_stop,
        "closed_viewers": closed_viewers,
        "attu": {
            "address": endpoint,
            "token": null,
            "recommended": true,
            "project_url": "https://github.com/zilliztech/attu"
        }
    }))
}

fn cmd_ui_start(
    root_arg: Option<PathBuf>,
    host: String,
    port: u16,
    max_workers: u16,
    stop_service: bool,
    timeout_seconds: u64,
) -> Result<Value> {
    let mut result = cmd_milvus_lite_server(
        root_arg,
        host,
        port,
        max_workers,
        stop_service,
        timeout_seconds,
    )?;
    result["ui"] = json!({
        "viewer": "attu",
        "recommendation": "Use Attu to visually inspect the agent_memory Milvus data.",
        "project_url": "https://github.com/zilliztech/attu"
    });
    Ok(result)
}

fn cmd_ui_status(root_arg: Option<PathBuf>) -> Result<Value> {
    let mut result = cmd_milvus_lite_server_status(root_arg)?;
    result["ui"] = json!({
        "viewer": "attu",
        "project_url": "https://github.com/zilliztech/attu"
    });
    Ok(result)
}

fn cmd_ui_stop(root_arg: Option<PathBuf>, timeout_seconds: u64) -> Result<Value> {
    cmd_milvus_lite_server_stop(root_arg, timeout_seconds)
}

fn cmd_milvus_lite_server_status(root_arg: Option<PathBuf>) -> Result<Value> {
    let root = runtime_root(root_arg)?;
    Ok(json!({"ok": true, "server": milvus_lite_server_status_value(&root)?}))
}

fn cmd_milvus_lite_server_stop(root_arg: Option<PathBuf>, timeout_seconds: u64) -> Result<Value> {
    let root = runtime_root(root_arg)?;
    let Some(mut state) = read_milvus_lite_server_state(&root)? else {
        return Ok(
            json!({"ok": true, "stopped": false, "server": milvus_lite_server_status_value(&root)?}),
        );
    };
    let pid = state.get("pid").and_then(Value::as_u64).unwrap_or(0) as u32;
    if pid_exists(pid) {
        terminate_process_group(pid)?;
        let deadline = Instant::now() + Duration::from_secs(timeout_seconds.max(1));
        while pid_exists(pid) {
            if Instant::now() >= deadline {
                bail!(
                    "timed out waiting for Milvus Lite server pid {pid} to stop; log: {}",
                    milvus_lite_server_log_path(&root).display()
                );
            }
            thread::sleep(Duration::from_millis(100));
        }
    }
    state["pid"] = json!(0);
    state["updated_at"] = json!(now());
    state["stopped_at"] = json!(now());
    write_milvus_lite_server_state(&root, &state)?;
    unregister_ui_viewer(&root)?;
    Ok(json!({"ok": true, "stopped": true, "server": milvus_lite_server_status_value(&root)?}))
}

fn cmd_dump(root_arg: Option<PathBuf>, output: Option<PathBuf>) -> Result<Value> {
    let root = runtime_root(root_arg)?;
    let payload = json!({
        "config": load_runtime_config(&root).ok(),
        "records": read_records(&root)?.iter().map(record_value).collect::<Vec<_>>(),
    });
    let path = output.unwrap_or_else(|| {
        memory_dir(&root)
            .join("dumps")
            .join(format!("memory-{}.json", safe_timestamp()))
    });
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, serde_json::to_string_pretty(&payload)? + "\n")?;
    Ok(json!({"ok": true, "dump": path}))
}

fn milvus_lite_server_command(
    data_dir: &Path,
    host: &str,
    port: u16,
    max_workers: u16,
) -> Result<Command> {
    let mut command = Command::new("uv");
    command
        .arg("run")
        .arg("--project")
        .arg(skill_root()?)
        .arg("milvus-lite")
        .arg("server")
        .arg("--data-dir")
        .arg(data_dir)
        .arg("--host")
        .arg(host)
        .arg("--port")
        .arg(port.to_string())
        .arg("--max-workers")
        .arg(max_workers.max(1).to_string());
    Ok(command)
}

fn wait_for_milvus_lite_server_start(
    child: &mut Child,
    host: &str,
    port: u16,
    timeout: Duration,
    log_path: &Path,
) -> Result<()> {
    let deadline = Instant::now() + timeout;
    loop {
        if TcpStream::connect((attu_connect_host(host), port)).is_ok() {
            return Ok(());
        }
        if let Some(status) = child.try_wait()? {
            bail!(
                "Milvus Lite server exited before becoming reachable ({status}); see {}",
                log_path.display()
            );
        }
        if Instant::now() >= deadline {
            bail!(
                "timed out waiting for Milvus Lite server on {}:{}; see {}",
                attu_connect_host(host),
                port,
                log_path.display()
            );
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn milvus_lite_server_status_value(root: &Path) -> Result<Value> {
    let state = read_milvus_lite_server_state(root)?;
    let pid = state
        .as_ref()
        .and_then(|value| value.get("pid"))
        .and_then(Value::as_u64)
        .unwrap_or(0) as u32;
    let active = pid_exists(pid);
    Ok(json!({
        "root": root,
        "state_path": milvus_lite_server_state_path(root),
        "active": active,
        "pid": if pid == 0 { Value::Null } else { json!(pid) },
        "endpoint": state.as_ref().and_then(|value| value.get("endpoint")).cloned().unwrap_or(Value::Null),
        "data_dir": state.as_ref().and_then(|value| value.get("data_dir")).cloned().unwrap_or(Value::Null),
        "database_name": state.as_ref().and_then(|value| value.get("database_name")).cloned().unwrap_or(Value::Null),
        "log_path": milvus_lite_server_log_path(root),
        "state": state,
    }))
}

fn stop_other_ui_viewers(
    root: &Path,
    host: &str,
    port: u16,
    timeout_seconds: u64,
) -> Result<Vec<Value>> {
    let mut closed = Vec::new();
    let mut kept = Vec::new();
    for entry in read_ui_viewer_registry()? {
        let entry_root = entry
            .get("root")
            .and_then(Value::as_str)
            .map(PathBuf::from)
            .unwrap_or_default();
        let entry_pid = entry.get("pid").and_then(Value::as_u64).unwrap_or(0) as u32;
        let entry_port = entry.get("port").and_then(Value::as_u64).unwrap_or(0) as u16;
        let entry_host = entry
            .get("host")
            .and_then(Value::as_str)
            .unwrap_or("127.0.0.1");
        let same_port =
            entry_port == port && attu_connect_host(entry_host) == attu_connect_host(host);
        if entry_root == root || !same_port || !pid_exists(entry_pid) {
            if pid_exists(entry_pid) {
                kept.push(entry);
            }
            continue;
        }
        terminate_process_group(entry_pid)?;
        let deadline = Instant::now() + Duration::from_secs(timeout_seconds.max(1));
        while pid_exists(entry_pid) {
            if Instant::now() >= deadline {
                bail!("timed out stopping existing UI viewer pid {entry_pid}");
            }
            thread::sleep(Duration::from_millis(100));
        }
        if let Some(mut state) = read_milvus_lite_server_state(&entry_root)? {
            state["pid"] = json!(0);
            state["updated_at"] = json!(now());
            state["stopped_at"] = json!(now());
            write_milvus_lite_server_state(&entry_root, &state)?;
        }
        closed.push(json!({
            "root": entry_root,
            "pid": entry_pid,
            "port": entry_port,
            "database_name": entry.get("database_name").cloned().unwrap_or(Value::Null),
        }));
    }
    write_ui_viewer_registry(&kept)?;
    Ok(closed)
}

fn register_ui_viewer(root: &Path, state: &Value) -> Result<()> {
    let mut entries = read_ui_viewer_registry()?;
    entries.retain(|entry| entry.get("root").and_then(Value::as_str).map(Path::new) != Some(root));
    entries.push(json!({
        "root": root,
        "pid": state.get("pid").cloned().unwrap_or(Value::Null),
        "host": state.get("host").cloned().unwrap_or(Value::Null),
        "port": state.get("port").cloned().unwrap_or(Value::Null),
        "endpoint": state.get("endpoint").cloned().unwrap_or(Value::Null),
        "data_dir": state.get("data_dir").cloned().unwrap_or(Value::Null),
        "database_name": state.get("database_name").cloned().unwrap_or(Value::Null),
        "updated_at": now(),
    }));
    write_ui_viewer_registry(&entries)
}

fn unregister_ui_viewer(root: &Path) -> Result<()> {
    let mut entries = read_ui_viewer_registry()?;
    entries.retain(|entry| entry.get("root").and_then(Value::as_str).map(Path::new) != Some(root));
    write_ui_viewer_registry(&entries)
}

fn read_ui_viewer_registry() -> Result<Vec<Value>> {
    let path = ui_viewer_registry_path();
    if !path.exists() {
        return Ok(Vec::new());
    }
    Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
}

fn write_ui_viewer_registry(entries: &[Value]) -> Result<()> {
    let path = ui_viewer_registry_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string_pretty(entries)? + "\n")?;
    Ok(())
}

fn milvus_lite_display_name(root: &Path, data_dir: &Path) -> String {
    let project = root
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("global");
    let db = data_dir
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("memory");
    format!("{project} ({db})")
}

fn read_milvus_lite_server_state(root: &Path) -> Result<Option<Value>> {
    let path = milvus_lite_server_state_path(root);
    if !path.exists() {
        return Ok(None);
    }
    Ok(Some(serde_json::from_str(&fs::read_to_string(path)?)?))
}

fn write_milvus_lite_server_state(root: &Path, state: &Value) -> Result<()> {
    fs::create_dir_all(memory_dir(root))?;
    fs::write(
        milvus_lite_server_state_path(root),
        serde_json::to_string_pretty(state)? + "\n",
    )?;
    Ok(())
}

fn attu_connect_host(host: &str) -> &str {
    match host {
        "0.0.0.0" => "127.0.0.1",
        "::" => "::1",
        value => value,
    }
}

fn terminate_process_group(pid: u32) -> Result<()> {
    let group_target = format!("-{pid}");
    let status = Command::new("kill")
        .arg("-TERM")
        .arg(&group_target)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    if status.map(|status| status.success()).unwrap_or(false) {
        return Ok(());
    }
    let status = Command::new("kill")
        .arg("-TERM")
        .arg(pid.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    if !status.success() {
        bail!("failed to terminate Milvus Lite server pid {pid}");
    }
    Ok(())
}
