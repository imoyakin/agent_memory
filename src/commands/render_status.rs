fn render_human(value: &Value) -> String {
    if value.get("main").is_some() && value.get("memories").is_some() {
        return render_ps(value);
    }
    if value.get("gateway").is_some() {
        return render_gateway(value);
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
    if value.get("remotes").is_some() {
        return render_gateway_remotes(value);
    }
    if value.get("server").is_some() || value.get("ui").is_some() {
        return render_ui(value);
    }
    if value.get("dump").is_some() {
        return format!("Dump written\n\n{}", key_value_table(value));
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

fn render_ps(value: &Value) -> String {
    let mut rows = Vec::new();
    if let Some(main) = value.get("main").filter(|item| !item.is_null()) {
        rows.push(process_display_row(main));
    }
    if let Some(memories) = value.get("memories").and_then(Value::as_array) {
        for memory in memories {
            rows.push(process_display_row(memory));
        }
    }
    if let Some(remotes) = value.get("remotes").and_then(Value::as_array) {
        for remote in remotes {
            rows.push(process_display_row(remote));
        }
    }
    if rows.is_empty() {
        "No running agent-memory services found.".to_string()
    } else {
        format!(
            "Agent Memory Processes\n\n{}",
            markdown_table(
                &["Role", "Scope", "Workdir", "Root", "Status", "Viewer", "PID"],
                &rows
            )
        )
    }
}

fn process_display_row(process: &Value) -> Vec<String> {
    vec![
        value_text(&process["role"]),
        value_text(&process["scope"]),
        value_text(&process["workdir"]),
        value_text(&process["root"]),
        value_text(&process["status"]),
        value_text(&process["viewer"]),
        value_text(&process["pid"]),
    ]
}

fn render_gateway(value: &Value) -> String {
    let gateway = value.get("gateway").unwrap_or(&Value::Null);
    let mut output = format!(
        "UI Gateway\n\n{}",
        key_value_rows(&[
            ("Active", value_text(&gateway["active"])),
            ("PID", value_text(&gateway["pid"])),
            ("Endpoint", value_text(&gateway["endpoint"])),
            ("Lease Expires", value_text(&gateway["lease_expires_at"])),
            ("Updated", value_text(&gateway["updated_at"])),
            ("State", value_text(&gateway["state_path"])),
            ("Log", value_text(&gateway["log_path"])),
        ])
    );
    if let Some(projects) = value.get("projects").and_then(Value::as_array) {
        output.push_str("\n\nProjects\n\n");
        output.push_str(&render_gateway_projects_table(projects));
    }
    output
}

fn render_gateway_projects_table(projects: &[Value]) -> String {
    let mut rows = Vec::new();
    for project in projects {
        rows.push(vec![
            value_text(&project["workdir"]),
            value_text(&project["root"]),
            value_text(&project["scope"]),
            value_text(&project["status"]),
            value_text(&project["memory_count"]),
            value_text(&project["viewer_active"]),
            value_text(&project["viewer_endpoint"]),
        ]);
    }
    if rows.is_empty() {
        "No agent-memory projects found.".to_string()
    } else {
        markdown_table(
            &[
                "Workdir", "Root", "Scope", "Status", "Memories", "Viewer", "Endpoint",
            ],
            &rows,
        )
    }
}

fn render_gateway_remotes(value: &Value) -> String {
    let Some(remotes) = value.get("remotes").and_then(Value::as_array) else {
        return key_value_table(value);
    };
    let mut rows = Vec::new();
    for remote in remotes {
        rows.push(vec![
            value_text(&remote["name"]),
            value_text(&remote["url"]),
            value_text(&remote["status"]),
            value_text(&remote["project_count"]),
            value_text(&remote["updated_at"]),
        ]);
    }
    if rows.is_empty() {
        "No attached remote gateways.".to_string()
    } else {
        format!(
            "Attached Remote Gateways\n\n{}",
            markdown_table(&["Name", "URL", "Status", "Projects", "Updated"], &rows)
        )
    }
}
