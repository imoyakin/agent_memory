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
