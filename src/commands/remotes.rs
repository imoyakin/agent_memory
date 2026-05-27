#[derive(Clone, Debug, Deserialize, Serialize)]
struct GatewayRemote {
    name: String,
    url: String,
    token: String,
    created_at: String,
    updated_at: String,
}

fn sanitize_remote_alias(raw: &str) -> Result<String> {
    let alias = raw.trim();
    if alias.is_empty() {
        bail!("remote name is required");
    }
    if !alias
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
    {
        bail!("remote name may only contain ASCII letters, numbers, '-' and '_'");
    }
    Ok(alias.to_string())
}

fn read_gateway_remotes() -> Result<Vec<GatewayRemote>> {
    let path = home_path(HomePath::GatewayRemotes);
    if !path.exists() {
        return Ok(Vec::new());
    }
    Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
}

fn write_gateway_remotes(remotes: &[GatewayRemote]) -> Result<()> {
    let path = home_path(HomePath::GatewayRemotes);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string_pretty(remotes)? + "\n")?;
    Ok(())
}

fn gateway_remote_statuses(local_gateway_endpoint: Option<&str>) -> Result<Vec<Value>> {
    let mut statuses = Vec::new();
    for remote in read_gateway_remotes()? {
        let mut status = json!({
            "role": "remote",
            "scope": "remote",
            "name": remote.name.clone(),
            "url": remote.url.clone(),
            "root": remote.url.clone(),
            "workdir": Value::Null,
            "status": "inactive",
            "viewer": Value::Null,
            "pid": Value::Null,
            "project_count": 0,
            "updated_at": remote.updated_at.clone(),
        });
        if let Ok(projects) = remote_gateway_projects(&remote, true) {
            status["status"] = json!("attached");
            status["project_count"] = json!(projects.len());
            status["projects"] = json!(rewrite_remote_projects(
                &remote.name,
                &projects,
                local_gateway_endpoint
            ));
        }
        statuses.push(status);
    }
    statuses.sort_by(|left, right| value_text(&left["name"]).cmp(&value_text(&right["name"])));
    Ok(statuses)
}

fn remote_gateway_projects(remote: &GatewayRemote, local_only: bool) -> Result<Vec<Value>> {
    let mut url = format!("{}/api/projects", remote.url.trim_end_matches('/'));
    if local_only {
        url.push_str("?local=true");
    }
    let client = reqwest::blocking::Client::new();
    let response = client
        .get(url)
        .header("X-Agent-Memory-Token", &remote.token)
        .send()?;
    let status = response.status();
    let body = response.text().unwrap_or_default();
    if !status.is_success() {
        bail!(
            "remote gateway {} returned HTTP {status}: {body}",
            remote.name
        );
    }
    let value: Value = serde_json::from_str(&body)?;
    Ok(value
        .get("projects")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default())
}

fn rewrite_remote_projects(
    alias: &str,
    projects: &[Value],
    local_gateway_endpoint: Option<&str>,
) -> Vec<Value> {
    projects
        .iter()
        .map(|project| {
            let mut project = project.clone();
            let root_hash = project
                .get("root_hash")
                .and_then(Value::as_str)
                .map(ToString::to_string)
                .or_else(|| {
                    project
                        .get("root")
                        .and_then(Value::as_str)
                        .map(|root| crate::ipc::root_hash(Path::new(root)))
                });
            project["remote"] = json!(alias);
            if let (Some(endpoint), Some(root_hash)) = (local_gateway_endpoint, root_hash) {
                project["viewer_endpoint"] = json!(format!(
                    "{}/remote/{}/view/{}/dashboard",
                    endpoint.trim_end_matches('/'),
                    alias,
                    root_hash
                ));
                project["viewer"] = project["viewer_endpoint"].clone();
            }
            project
        })
        .collect()
}
