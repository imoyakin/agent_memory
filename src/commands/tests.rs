#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qdrant_viewer_url_points_to_gateway_dashboard_proxy() {
        let root = Path::new("/tmp/My Project");
        let url = qdrant_viewer_url("http://127.0.0.1:19531", root);

        assert_eq!(
            url,
            format!(
                "http://127.0.0.1:19531/view/{}/dashboard",
                crate::ipc::root_hash(root)
            )
        );
    }

    #[test]
    fn ps_payload_has_main_memories_remotes_and_pruned() {
        let payload = ps_payload(
            Some(json!({"pid": 100, "endpoint": "http://127.0.0.1:19531"})),
            vec![json!({"role": "memory", "scope": "project"})],
            vec![json!({"name": "remote-a"})],
            2,
        );

        assert!(payload.get("main").is_some());
        assert_eq!(payload["memories"].as_array().unwrap().len(), 1);
        assert_eq!(payload["remotes"].as_array().unwrap().len(), 1);
        assert_eq!(payload["pruned"], 2);
    }

    #[test]
    fn gateway_proxy_route_extracts_local_qdrant_view() {
        let route = gateway_proxy_route("/view/abc123/dashboard", "");

        assert_eq!(
            route,
            Some(ProxyRoute::LocalQdrant {
                root_hash: "abc123".to_string(),
                upstream_path: "/dashboard".to_string(),
            })
        );
    }

    #[test]
    fn gateway_proxy_route_extracts_remote_qdrant_view() {
        let route = gateway_proxy_route("/remote/workbox/view/abc123/dashboard", "");

        assert_eq!(
            route,
            Some(ProxyRoute::RemoteGateway {
                alias: "workbox".to_string(),
                root_hash: "abc123".to_string(),
                upstream_path: "/dashboard".to_string(),
            })
        );
    }
}
