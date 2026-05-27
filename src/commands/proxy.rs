fn gateway_proxy_route(path: &str, referer: &str) -> Option<ProxyRoute> {
    parse_direct_proxy_route(path).or_else(|| {
        let referer_path = referer_path(referer)?;
        let route = parse_direct_proxy_route(&referer_path)?;
        match route {
            ProxyRoute::LocalQdrant { root_hash, .. } => Some(ProxyRoute::LocalQdrant {
                root_hash,
                upstream_path: normalize_upstream_path(path),
            }),
            ProxyRoute::RemoteGateway {
                alias, root_hash, ..
            } => Some(ProxyRoute::RemoteGateway {
                alias,
                root_hash,
                upstream_path: normalize_upstream_path(path),
            }),
        }
    })
}

fn parse_direct_proxy_route(path: &str) -> Option<ProxyRoute> {
    if let Some(rest) = path.strip_prefix("/view/") {
        if let Some((root_hash, upstream_path)) = parse_view_path(rest) {
            return Some(ProxyRoute::LocalQdrant {
                root_hash,
                upstream_path,
            });
        }
    }
    let rest = path.strip_prefix("/remote/")?;
    let (alias, rest) = rest.split_once('/')?;
    let (root_hash, upstream_path) = parse_view_path(rest.strip_prefix("view/")?)?;
    Some(ProxyRoute::RemoteGateway {
        alias: alias.to_string(),
        root_hash,
        upstream_path,
    })
}

fn parse_view_path(rest: &str) -> Option<(String, String)> {
    let (root_hash, tail) = rest.split_once('/').unwrap_or((rest, ""));
    if root_hash.is_empty() {
        return None;
    }
    Some((root_hash.to_string(), normalize_upstream_path(tail)))
}

fn normalize_upstream_path(path: &str) -> String {
    let path = path.trim();
    if path.is_empty() {
        "/dashboard".to_string()
    } else if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    }
}

fn referer_path(referer: &str) -> Option<String> {
    if referer.trim().is_empty() {
        return None;
    }
    reqwest::Url::parse(referer)
        .ok()
        .map(|url| url.path().to_string())
        .or_else(|| Some(referer.to_string()))
}

fn proxy_gateway_request(
    request: &GatewayHttpRequest,
    route: ProxyRoute,
) -> Result<GatewayHttpResponse> {
    match route {
        ProxyRoute::LocalQdrant {
            root_hash,
            upstream_path,
        } => {
            let target = local_qdrant_target(&root_hash)?
                .ok_or_else(|| anyhow::anyhow!("no active local Qdrant target for {root_hash}"))?;
            proxy_to_url(
                request,
                &target,
                &upstream_path,
                None,
                Some(format!("/view/{root_hash}")),
            )
        }
        ProxyRoute::RemoteGateway {
            alias,
            root_hash,
            upstream_path,
        } => {
            let remote = read_gateway_remotes()?
                .into_iter()
                .find(|remote| remote.name == alias)
                .ok_or_else(|| anyhow::anyhow!("remote gateway is not attached: {alias}"))?;
            let upstream_path = format!("/view/{root_hash}{upstream_path}");
            proxy_to_url(
                request,
                &remote.url,
                &upstream_path,
                Some(remote.token.as_str()),
                Some(format!("/remote/{alias}/view/{root_hash}")),
            )
        }
    }
}

fn local_qdrant_target(root_hash: &str) -> Result<Option<String>> {
    let mut roots = Vec::new();
    for viewer in read_ui_viewer_registry()? {
        if let Some(root) = viewer.get("root").and_then(Value::as_str) {
            roots.push(PathBuf::from(root));
        }
    }
    let (processes, _) = memory_processes()?;
    for process in processes {
        if let Some(root) = process.get("root").and_then(Value::as_str) {
            roots.push(PathBuf::from(root));
        }
    }
    roots.sort();
    roots.dedup();
    for root in roots {
        if crate::ipc::root_hash(&root) != root_hash {
            continue;
        }
        let config = load_runtime_config(&root)?;
        return Ok(Some(config.storage.qdrant.uri));
    }
    Ok(None)
}

fn proxy_to_url(
    request: &GatewayHttpRequest,
    upstream_base: &str,
    upstream_path: &str,
    token: Option<&str>,
    location_prefix: Option<String>,
) -> Result<GatewayHttpResponse> {
    let method = reqwest::Method::from_bytes(request.method.as_bytes())?;
    let mut url = format!("{}{}", upstream_base.trim_end_matches('/'), upstream_path);
    if !request.query.is_empty() {
        url.push('?');
        url.push_str(&request.query);
    }
    let client = reqwest::blocking::Client::new();
    let mut upstream = client
        .request(method, url)
        .header(reqwest::header::ACCEPT_ENCODING, "identity");
    if let Some(token) = token {
        upstream = upstream.header("X-Agent-Memory-Token", token);
    }
    if let Some(content_type) = request.headers.get("content-type") {
        upstream = upstream.header(reqwest::header::CONTENT_TYPE, content_type);
    }
    if let Some(accept) = request.headers.get("accept") {
        upstream = upstream.header(reqwest::header::ACCEPT, accept);
    }
    if !request.body.is_empty() {
        upstream = upstream.body(request.body.clone());
    }
    let response = upstream.send()?;
    let status = response.status().as_u16();
    let headers = response.headers().clone();
    let content_type = headers
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_string();
    let body = response.bytes()?.to_vec();
    let mut response_headers = Vec::new();
    for name in [
        reqwest::header::CACHE_CONTROL,
        reqwest::header::CONTENT_ENCODING,
        reqwest::header::LOCATION,
    ] {
        if let Some(value) = headers.get(&name).and_then(|value| value.to_str().ok()) {
            let value = if name == reqwest::header::LOCATION {
                rewrite_proxy_location(value, location_prefix.as_deref())
            } else {
                value.to_string()
            };
            response_headers.push((name.as_str().to_string(), value));
        }
    }
    Ok(GatewayHttpResponse {
        status,
        content_type,
        headers: response_headers,
        body,
    })
}

fn rewrite_proxy_location(location: &str, prefix: Option<&str>) -> String {
    let Some(prefix) = prefix else {
        return location.to_string();
    };
    if location.starts_with('/') && !location.starts_with(prefix) {
        format!("{}{}", prefix.trim_end_matches('/'), location)
    } else {
        location.to_string()
    }
}

fn query_params(query: &str) -> HashMap<String, String> {
    query
        .split('&')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let (key, value) = part.split_once('=').unwrap_or((part, ""));
            (url_decode(key), url_decode(value))
        })
        .collect()
}

fn url_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                out.push(b' ');
                index += 1;
            }
            b'%' if index + 2 < bytes.len() => {
                if let Ok(hex) = u8::from_str_radix(&value[index + 1..index + 3], 16) {
                    out.push(hex);
                    index += 3;
                } else {
                    out.push(bytes[index]);
                    index += 1;
                }
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).to_string()
}
