fn register_ui_viewer(root: &Path, state: &Value) -> Result<()> {
    let mut entries = read_ui_viewer_registry()?;
    entries.retain(|entry| entry.get("root").and_then(Value::as_str).map(Path::new) != Some(root));
    entries.push(json!({
        "root": root,
        "pid": state.get("pid").cloned().unwrap_or(Value::Null),
        "host": state.get("host").cloned().unwrap_or(Value::Null),
        "port": state.get("port").cloned().unwrap_or(Value::Null),
        "endpoint": state.get("endpoint").cloned().unwrap_or(Value::Null),
        "qdrant_endpoint": state.get("qdrant_endpoint").cloned().unwrap_or(Value::Null),
        "data_dir": state.get("data_dir").cloned().unwrap_or(Value::Null),
        "database_name": state.get("database_name").cloned().unwrap_or(Value::Null),
        "root_hash": state.get("root_hash").cloned().unwrap_or_else(|| json!(crate::ipc::root_hash(root))),
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
    let path = home_path(HomePath::UiViewerRegistry);
    if !path.exists() {
        return Ok(Vec::new());
    }
    Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
}

fn write_ui_viewer_registry(entries: &[Value]) -> Result<()> {
    let path = home_path(HomePath::UiViewerRegistry);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string_pretty(entries)? + "\n")?;
    Ok(())
}
