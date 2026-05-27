fn handle_gateway_http_stream(stream: &mut TcpStream) -> Result<()> {
    let request = read_http_request(stream)?;
    let response = gateway_http_response(&request);
    write_http_response(stream, response)
}

#[derive(Debug)]
struct GatewayHttpRequest {
    method: String,
    path: String,
    query: String,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

struct GatewayHttpResponse {
    status: u16,
    content_type: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ProxyRoute {
    LocalQdrant {
        root_hash: String,
        upstream_path: String,
    },
    RemoteGateway {
        alias: String,
        root_hash: String,
        upstream_path: String,
    },
}

fn read_http_request(stream: &mut TcpStream) -> Result<GatewayHttpRequest> {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 8192];
    loop {
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
        if find_header_end(&buffer).is_some() {
            break;
        }
        if buffer.len() > 128 * 1024 {
            bail!("HTTP request headers are too large");
        }
    }
    let header_end =
        find_header_end(&buffer).ok_or_else(|| anyhow::anyhow!("invalid HTTP request"))?;
    let header_text = String::from_utf8_lossy(&buffer[..header_end]);
    let mut lines = header_text.split("\r\n");
    let request_line = lines.next().unwrap_or_default();
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("GET").to_string();
    let target = parts.next().unwrap_or("/").to_string();
    let mut headers = HashMap::new();
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
    }
    let content_length = headers
        .get("content-length")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let body_start = header_end + 4;
    while buffer.len().saturating_sub(body_start) < content_length {
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
    }
    let body_end = (body_start + content_length).min(buffer.len());
    let body = buffer[body_start..body_end].to_vec();
    let (path, query) = target
        .split_once('?')
        .map(|(path, query)| (path.to_string(), query.to_string()))
        .unwrap_or_else(|| (target.clone(), String::new()));
    Ok(GatewayHttpRequest {
        method,
        path,
        query,
        headers,
        body,
    })
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

fn gateway_http_response(request: &GatewayHttpRequest) -> GatewayHttpResponse {
    if let Some(route) = gateway_proxy_route(
        &request.path,
        request
            .headers
            .get("referer")
            .map(String::as_str)
            .unwrap_or(""),
    ) {
        return proxy_gateway_request(request, route).unwrap_or_else(error_response);
    }
    match request.path.as_str() {
        "/" => json_http_response(Ok(json!({
            "ok": true,
            "gateway": gateway_status_value().ok(),
            "projects": gateway_projects().unwrap_or_default()
        }))),
        "/api/projects" => {
            let local_only = query_params(&request.query)
                .get("local")
                .map(|value| value == "true" || value == "1")
                .unwrap_or(false);
            json_http_response(gateway_projects_with_options(local_only).map(|projects| {
                json!({"ok": true, "gateway": gateway_status_value().ok(), "projects": projects})
            }))
        }
        "/api/status" => json_http_response(gateway_status_value().map(|gateway| {
            json!({"ok": true, "gateway": gateway})
        })),
        "/api/memories" => json_http_response(Err(anyhow::anyhow!(
            "agent-memory no longer serves a custom memory viewer; use the Qdrant dashboard proxy URL"
        ))),
        _ => GatewayHttpResponse {
            status: 404,
            content_type: "application/json; charset=utf-8".to_string(),
            headers: Vec::new(),
            body: json!({"ok": false, "error": "not found"}).to_string().into_bytes(),
        },
    }
}

fn json_http_response(result: Result<Value>) -> GatewayHttpResponse {
    match result {
        Ok(value) => GatewayHttpResponse {
            status: 200,
            content_type: "application/json; charset=utf-8".to_string(),
            headers: Vec::new(),
            body: value.to_string().into_bytes(),
        },
        Err(error) => error_response(error),
    }
}

fn error_response(error: anyhow::Error) -> GatewayHttpResponse {
    GatewayHttpResponse {
        status: 500,
        content_type: "application/json; charset=utf-8".to_string(),
        headers: Vec::new(),
        body: json!({"ok": false, "error": error.to_string()})
            .to_string()
            .into_bytes(),
    }
}

fn write_http_response(stream: &mut TcpStream, response: GatewayHttpResponse) -> Result<()> {
    let reason = match response.status {
        200 => "OK",
        204 => "No Content",
        404 => "Not Found",
        500 => "Internal Server Error",
        _ => "Internal Server Error",
    };
    write!(
        stream,
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n",
        response.status,
        reason,
        response.content_type,
        response.body.len(),
    )?;
    for (name, value) in response.headers {
        write!(stream, "{name}: {value}\r\n")?;
    }
    write!(stream, "\r\n")?;
    stream.write_all(&response.body)?;
    Ok(())
}
