pub(crate) fn write_user_config_template(
    path: &Path,
    install_scope: &str,
    force: bool,
) -> Result<()> {
    if path.exists() && !force {
        bail!("config file already exists: {}", path.display());
    }
    let mut config = UserConfig {
        install_scope: install_scope.to_string(),
        ..UserConfig::default()
    };
    if install_scope == "global" {
        config.memory_root = "~".to_string();
        config.allowed_memory_types = vec!["environment".to_string(), "preference".to_string()];
    }
    config.storage = StorageConfig::default();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_yaml::to_string(&config)?)?;
    Ok(())
}

pub(crate) fn load_user_config(path: &Path) -> Result<UserConfig> {
    if !matches!(
        path.extension().and_then(|value| value.to_str()),
        Some("yaml" | "yml")
    ) {
        bail!("user config must be memory.yaml, got: {}", path.display());
    }
    let mut config: UserConfig = serde_yaml::from_str(&fs::read_to_string(path)?)?;
    normalize_storage(&mut config.storage);
    Ok(config)
}

pub(crate) fn persist_user_config_if_exists(path: &Path, config: &UserConfig) -> Result<()> {
    if path.exists() {
        fs::write(path, serde_yaml::to_string(config)?)?;
    }
    Ok(())
}

pub(crate) fn normalize_storage(storage: &mut StorageConfig) {
    if storage.instance_uuid.trim().is_empty() {
        storage.instance_uuid = new_uuid_string();
    }
    if storage.qdrant.uri.trim().is_empty() {
        storage.qdrant.uri = default_qdrant_uri();
    }
    if storage.qdrant.storage_path.trim().is_empty() {
        storage.qdrant.storage_path = QdrantConfig::for_uuid(&storage.instance_uuid).storage_path;
    }
    if storage.qdrant.binary.trim().is_empty() {
        storage.qdrant.binary = default_qdrant_binary();
    }
    if matches!(
        storage.qdrant.static_content_dir.as_deref().map(str::trim),
        Some("")
    ) {
        storage.qdrant.static_content_dir = None;
    }
}

pub(crate) fn normalize_storage_for_runtime(
    storage: &mut StorageConfig,
    runtime_root: &Path,
    install_scope: &str,
) {
    normalize_storage(storage);
    let slug = logical_database_name(runtime_root, install_scope);
    let uuid = uuid_hex(&storage.instance_uuid);
    let default_qdrant = format!(".memory/qdrant/{uuid}");
    let old_named_qdrant = format!(".memory/qdrant/agent_memory-{uuid}");
    if storage.qdrant.storage_path == default_qdrant
        || storage.qdrant.storage_path == old_named_qdrant
        || storage.qdrant.storage_path == default_qdrant_storage_path()
        || storage.qdrant.storage_path.trim().is_empty()
    {
        storage.qdrant.storage_path = format!(".memory/qdrant/{slug}-{uuid}");
    }
}

pub(crate) fn logical_database_name(root: &Path, install_scope: &str) -> String {
    let user = if install_scope == "global" {
        current_user_name()
    } else {
        None
    };
    logical_database_name_for_user(root, install_scope, user.as_deref())
}

fn logical_database_name_for_user(
    root: &Path,
    install_scope: &str,
    user_name: Option<&str>,
) -> String {
    let raw = if install_scope == "global" {
        user_name.unwrap_or("global")
    } else {
        root.file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("memory")
    };
    sanitize_logical_name(raw)
}

fn sanitize_logical_name(raw: &str) -> String {
    let mut slug = String::new();
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
        } else if !slug.ends_with('_') {
            slug.push('_');
        }
    }
    let slug = slug.trim_matches('_');
    if slug.is_empty() {
        "memory".to_string()
    } else {
        slug.to_string()
    }
}

fn current_user_name() -> Option<String> {
    std::env::var("USER")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| {
            std::process::Command::new("whoami")
                .output()
                .ok()
                .filter(|output| output.status.success())
                .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
                .filter(|value| !value.is_empty())
        })
        .or_else(|| {
            std::process::Command::new("id")
                .arg("-un")
                .output()
                .ok()
                .filter(|output| output.status.success())
                .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
                .filter(|value| !value.is_empty())
        })
}

pub(crate) fn load_runtime_config(root: &Path) -> Result<RuntimeConfig> {
    let mut config: RuntimeConfig =
        serde_json::from_str(&fs::read_to_string(project_path(root, ProjectPath::RuntimeConfig))?)?;
    normalize_storage(&mut config.storage);
    Ok(config)
}

pub(crate) fn write_runtime_config(root: &Path, config: &RuntimeConfig) -> Result<()> {
    fs::create_dir_all(project_path(root, ProjectPath::MemoryDir))?;
    fs::write(
        project_path(root, ProjectPath::RuntimeConfig),
        serde_json::to_string_pretty(config)? + "\n",
    )?;
    Ok(())
}
