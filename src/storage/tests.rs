#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qdrant_static_content_dir_prefers_configured_path() {
        let config = QdrantConfig {
            static_content_dir: Some("/tmp/qdrant-static".to_string()),
            ..QdrantConfig::default()
        };

        assert_eq!(
            qdrant_static_content_dir(&config),
            Some(PathBuf::from("/tmp/qdrant-static"))
        );
    }

    #[test]
    fn qdrant_static_content_dir_falls_back_to_binary_sibling() {
        let dir = tempfile::tempdir().unwrap();
        let static_dir = dir.path().join("qdrant-static");
        fs::create_dir(&static_dir).unwrap();
        let config = QdrantConfig {
            binary: dir.path().join("qdrant").to_string_lossy().to_string(),
            ..QdrantConfig::default()
        };

        assert_eq!(qdrant_static_content_dir(&config), Some(static_dir));
    }

    #[test]
    fn qdrant_state_restart_detects_storage_path_mismatch() {
        let config = QdrantConfig {
            binary: "/tmp/qdrant".to_string(),
            ..QdrantConfig::default()
        };
        let state = json!({
            "binary": "/tmp/qdrant",
            "storage_path": "/tmp/old-storage"
        });

        assert!(qdrant_state_requires_restart(
            Path::new("/tmp/new-storage"),
            &config,
            &state
        ));
    }

    #[test]
    fn qdrant_start_requires_service_supervisor_context() {
        assert!(!qdrant_start_allowed_from_env(None));
        assert!(!qdrant_start_allowed_from_env(Some("")));
        assert!(!qdrant_start_allowed_from_env(Some("cli")));
        assert!(qdrant_start_allowed_from_env(Some("service")));
    }
}
