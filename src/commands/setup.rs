fn apply_logical_database_defaults(user_config: &mut UserConfig, root: &Path) {
    let logical_name = logical_database_name(root, &user_config.install_scope);
    if user_config.collection_name.trim().is_empty()
        || user_config.collection_name == DEFAULT_COLLECTION
    {
        user_config.collection_name = logical_name;
    }
    normalize_storage_for_runtime(&mut user_config.storage, root, &user_config.install_scope);
}

fn preserve_existing_project_config(root: &Path, output: PathBuf, force: bool) -> PathBuf {
    if force {
        return output;
    }
    let default_agents_config = root.join(".agents/agent_memory/memory.yaml");
    let existing_memory_config = root.join(".memory/memory.yaml");
    if output == default_agents_config && existing_memory_config.exists() {
        existing_memory_config
    } else {
        output
    }
}

struct InitOptions {
    root_arg: Option<PathBuf>,
    config_path_arg: Option<PathBuf>,
    provider: Option<String>,
    model: Option<String>,
    dim: Option<usize>,
    endpoint: Option<String>,
    collection: Option<String>,
    force: bool,
    update_agents: bool,
    start_service: bool,
}

fn cmd_init(options: InitOptions) -> Result<Value> {
    let InitOptions {
        root_arg,
        config_path_arg,
        provider,
        model,
        dim,
        endpoint,
        collection,
        force,
        update_agents,
        start_service,
    } = options;
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
        let user_config = UserConfig::default();
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
        user_config.collection_name = collection;
    }
    apply_logical_database_defaults(&mut user_config, &project_root);
    if config_path.exists() {
        persist_user_config_if_exists(&config_path, &user_config)?;
    } else {
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&config_path, serde_yaml::to_string(&user_config)?)?;
    }

    let runtime_path = project_path(&root, ProjectPath::RuntimeConfig);
    if runtime_path.exists() && !force {
        let mut runtime = load_runtime_config(&root)?;
        runtime.schema_version = SCHEMA_VERSION;
        runtime.collection_name = user_config.collection_name.clone();
        runtime.retrieval = user_config.retrieval.clone();
        runtime.storage = user_config.storage.clone();
        runtime.allowed_memory_types = user_config.allowed_memory_types.clone();
        write_runtime_config(&root, &runtime)?;
        ensure_backend(&root, &runtime)?;
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
    ensure_backend(&root, &runtime)?;
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

struct SetupOptions {
    root_arg: Option<PathBuf>,
    output: PathBuf,
    install_scope: String,
    force_template: bool,
    update_agents: bool,
    run_init: bool,
    start_service: bool,
}

fn cmd_setup(options: SetupOptions) -> Result<Value> {
    let SetupOptions {
        root_arg,
        output,
        install_scope,
        force_template,
        update_agents,
        run_init,
        start_service,
    } = options;
    let root = discover_root(root_arg.as_deref())?;
    let output = if output.is_absolute() {
        output
    } else {
        root.join(output)
    };
    let output = preserve_existing_project_config(&root, output, force_template);
    let sync = run_uv_sync()?;
    let created_config = if output.exists() && !force_template {
        false
    } else {
        write_user_config_template(&output, &install_scope, force_template)?;
        let mut user_config = load_user_config(&output)?;
        apply_logical_database_defaults(&mut user_config, &root);
        fs::write(&output, serde_yaml::to_string(&user_config)?)?;
        true
    };
    let agents_file = if update_agents {
        Some(write_agents_config_pointer(&root, &output)?)
    } else {
        None
    };
    let init_result = if run_init || start_service {
        Some(cmd_init(InitOptions {
            root_arg: Some(root.clone()),
            config_path_arg: Some(output.clone()),
            provider: None,
            model: None,
            dim: None,
            endpoint: None,
            collection: None,
            force: false,
            update_agents,
            start_service,
        })?)
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
    update_agents: bool,
    force: bool,
) -> Result<Value> {
    let root = discover_root(root_arg.as_deref())?;
    let output = if output.is_absolute() {
        output
    } else {
        root.join(output)
    };
    let output = preserve_existing_project_config(&root, output, force);
    write_user_config_template(&output, &install_scope, force)?;
    let mut user_config = load_user_config(&output)?;
    apply_logical_database_defaults(&mut user_config, &root);
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
