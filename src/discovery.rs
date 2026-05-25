use anyhow::{anyhow, Result};
use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};

use crate::config::{load_user_config, UserConfig};
use crate::paths::absolutize;
use crate::{AGENTS_MARKER_END, AGENTS_MARKER_START};

pub(crate) fn active_user_config(
    root_arg: Option<PathBuf>,
    config_arg: Option<PathBuf>,
) -> Result<(PathBuf, PathBuf, UserConfig)> {
    if let Some(config_path) = config_arg {
        let config_path = absolutize(config_path)?;
        let project_root = root_arg
            .map(absolutize)
            .transpose()?
            .unwrap_or_else(|| infer_project_root_from_config_path(&config_path));
        let user_config = load_user_config(&config_path)?;
        return Ok((project_root, config_path, user_config));
    }
    let discovered = discover(root_arg.as_deref())?
        .ok_or_else(|| anyhow!("memory.yaml not found. Run `agent-memory setup` first."))?;
    let user_config = load_user_config(&discovered.config_path)?;
    Ok((discovered.project_root, discovered.config_path, user_config))
}

pub(crate) struct Discovery {
    pub(crate) project_root: PathBuf,
    pub(crate) config_path: PathBuf,
    pub(crate) source: String,
}

pub(crate) fn discover(root: Option<&Path>) -> Result<Option<Discovery>> {
    let base = discover_root(root)?;
    let mut roots = vec![base.clone()];
    if root.is_none() {
        roots.extend(base.ancestors().skip(1).map(Path::to_path_buf));
    }
    for project_root in &roots {
        let agents_file = project_root.join("AGENTS.md");
        if !agents_file.exists() {
            continue;
        }
        if let Some(pointer) = read_agents_pointer(&agents_file)? {
            let config_path = if Path::new(&pointer).is_absolute() {
                PathBuf::from(pointer)
            } else {
                project_root.join(pointer)
            };
            if config_path.exists() {
                return Ok(Some(Discovery {
                    project_root: infer_project_root_from_config_path(&config_path),
                    config_path,
                    source: "agents.md".to_string(),
                }));
            }
        }
    }
    for project_root in &roots {
        for config_path in [
            project_root.join("memory.yaml"),
            project_root.join(".memory").join("memory.yaml"),
            project_root
                .join(".agents")
                .join("agent_memory")
                .join("memory.yaml"),
        ] {
            if config_path.exists() {
                return Ok(Some(Discovery {
                    project_root: project_root.clone(),
                    config_path,
                    source: "fallback".to_string(),
                }));
            }
        }
    }
    Ok(None)
}

pub(crate) fn read_agents_pointer(path: &Path) -> Result<Option<String>> {
    let text = fs::read_to_string(path)?;
    let marker = Regex::new(&format!(
        "{}(?s:(.*?)){}",
        regex::escape(AGENTS_MARKER_START),
        regex::escape(AGENTS_MARKER_END)
    ))?;
    if let Some(capture) = marker.captures(&text) {
        if let Some(path) =
            extract_config_path(capture.get(1).map(|item| item.as_str()).unwrap_or(""))
        {
            return Ok(Some(path));
        }
    }
    for line in text.lines() {
        let lowered = line.to_lowercase();
        if (lowered.contains("agent-memory") || lowered.contains("agent_memory"))
            && (lowered.contains("memory.yaml") || lowered.contains("memory.yml"))
        {
            if let Some(path) = extract_config_path(line) {
                return Ok(Some(path));
            }
        }
    }
    Ok(None)
}

pub(crate) fn extract_config_path(text: &str) -> Option<String> {
    Regex::new(r#"(?P<path>(?:~|\.{1,2}|/|[A-Za-z0-9_-])[\w./~@+-]*memory\.ya?ml)"#)
        .ok()?
        .captures(text)
        .and_then(|capture| capture.name("path"))
        .map(|item| item.as_str().trim_matches(['`', '\'', '"']).to_string())
}

pub(crate) fn write_agents_config_pointer(root: &Path, config_path: &Path) -> Result<PathBuf> {
    let agents_file = root.join("AGENTS.md");
    let relative = config_path.strip_prefix(root).unwrap_or(config_path);
    let block = format!(
        "{AGENTS_MARKER_START}\nAgent Memory configuration: `{}`.\n\nAgent Memory magic word:\n- If the user writes `$agent_memory init`, run `agent-memory --agent init --start-service` for the current project, then report the service status.\n\nAgent Memory usage:\n- Before starting a non-trivial task, run `agent-memory --agent memory discover` to load the active memory configuration.\n- Search memory before asking the user when prior decisions, repository conventions, user preferences, known failures, domain knowledge, or research may matter: `agent-memory --agent memory search \"<query>\"`.\n- Treat memory as advisory. Current user instructions, live repository contents, official documentation, and fresh tool output override stored memory.\n- After making a durable, reusable, evidence-backed discovery, consider writing it with `agent-memory --agent memory add --content \"<memory>\" --type <type> --source-kind <kind> --source-ref <ref> --confidence <0..1> --keys \"<search keys>\"`.\n- Run embedding work with `agent-memory --agent service worker --once`, or keep the resident service available with `agent-memory --agent service start` and stop it with `agent-memory --agent service stop`.\n- Write project-specific memories to project scope and global/user-preference memories to global scope when a global memory config is available.\n- If only project memory is configured, write otherwise-global relevant memories to the project memory instead of dropping them.\n{AGENTS_MARKER_END}",
        relative.display()
    );
    let updated = if agents_file.exists() {
        let text = fs::read_to_string(&agents_file)?;
        let marker = Regex::new(&format!(
            "{}(?s:.*?){}",
            regex::escape(AGENTS_MARKER_START),
            regex::escape(AGENTS_MARKER_END)
        ))?;
        if marker.is_match(&text) {
            marker.replace(&text, block.as_str()).to_string()
        } else {
            format!("{}\n\n## Agent Memory\n\n{}\n", text.trim_end(), block)
        }
    } else {
        format!("# Agent Guidelines\n\n{}\n", block)
    };
    fs::write(&agents_file, updated)?;
    Ok(agents_file)
}

pub(crate) fn runtime_root(root_arg: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(root) = root_arg {
        return absolutize(root);
    }
    if let Some(discovered) = discover(None)? {
        let config = load_user_config(&discovered.config_path)?;
        return resolve_memory_root(&config, &discovered.config_path, &discovered.project_root);
    }
    discover_root(None)
}

pub(crate) fn resolve_memory_root(
    config: &UserConfig,
    _config_path: &Path,
    project_root: &Path,
) -> Result<PathBuf> {
    let raw = PathBuf::from(config.memory_root.replace(
        '~',
        &std::env::var("HOME").unwrap_or_else(|_| "~".to_string()),
    ));
    if raw.is_absolute() {
        Ok(raw)
    } else {
        Ok(project_root
            .join(raw)
            .canonicalize()
            .unwrap_or_else(|_| project_root.join(&config.memory_root)))
    }
}

pub(crate) fn infer_project_root_from_config_path(path: &Path) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    if parent.file_name().and_then(|value| value.to_str()) == Some(".memory") {
        return parent.parent().unwrap_or(parent).to_path_buf();
    }
    if parent.file_name().and_then(|value| value.to_str()) == Some("agent_memory")
        && parent
            .parent()
            .and_then(|value| value.file_name())
            .and_then(|value| value.to_str())
            == Some(".agents")
    {
        return parent
            .parent()
            .and_then(Path::parent)
            .unwrap_or(parent)
            .to_path_buf();
    }
    parent.to_path_buf()
}

pub(crate) fn discover_root(root: Option<&Path>) -> Result<PathBuf> {
    if let Some(root) = root {
        return absolutize(root.to_path_buf());
    }
    let mut current = std::env::current_dir()?;
    loop {
        if current.join(".memory").exists() {
            return Ok(current);
        }
        if !current.pop() {
            break;
        }
    }
    Ok(std::env::current_dir()?)
}
