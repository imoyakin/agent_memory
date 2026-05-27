fn cmd_agents_hook_install(
    root_arg: Option<PathBuf>,
    output: PathBuf,
    install_scope: String,
    backend: BackendKind,
    remote_uri: Option<String>,
) -> Result<Value> {
    let root = discover_root(root_arg.as_deref())?;
    let output = if output.is_absolute() {
        output
    } else {
        root.join(output)
    };
    let output = preserve_existing_project_config(&root, output, false);
    let created_config = if output.exists() {
        false
    } else {
        write_user_config_template(&output, &install_scope, backend, remote_uri, false)?;
        let mut user_config = load_user_config(&output)?;
        apply_logical_database_defaults(&mut user_config, &root);
        fs::write(&output, serde_yaml::to_string(&user_config)?)?;
        true
    };
    let agents_file = write_agents_config_pointer(&root, &output)?;
    Ok(json!({
        "ok": true,
        "root": root,
        "config_path": output,
        "created_config": created_config,
        "agents_file": agents_file,
    }))
}

fn cmd_agents_hook_remove(root_arg: Option<PathBuf>) -> Result<Value> {
    let root = discover_root(root_arg.as_deref())?;
    let (agents_file, removed) = remove_agents_config_pointer(&root)?;
    Ok(json!({
        "ok": true,
        "root": root,
        "agents_file": agents_file,
        "removed": removed,
    }))
}
