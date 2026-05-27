#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_uuid_backed_storage() {
        let config = UserConfig::default();
        assert_eq!(config.storage.backend, BackendKind::Qdrant);
        assert!(config
            .storage
            .qdrant
            .storage_path
            .contains(&uuid_hex(&config.storage.instance_uuid)));
        assert!(config
            .storage
            .milvus_lite
            .db_path
            .contains(&uuid_hex(&config.storage.instance_uuid)));
        assert!(config
            .storage
            .milvus_remote
            .database
            .contains(&uuid_hex(&config.storage.instance_uuid)));
    }

    #[test]
    fn global_template_uses_home_runtime_memory_dir() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("memory.yaml");
        write_user_config_template(&path, "global", BackendKind::MilvusLite, None, false).unwrap();

        let config = load_user_config(&path).unwrap();
        assert_eq!(config.install_scope, "global");
        assert_eq!(config.memory_root, "~");
        assert_eq!(
            config.allowed_memory_types,
            vec!["environment".to_string(), "preference".to_string()]
        );
    }

    #[test]
    fn project_storage_names_include_project_slug() {
        let mut storage = StorageConfig::default();
        normalize_storage_for_runtime(&mut storage, Path::new("/tmp/My Project"), "project");
        assert!(storage.qdrant.storage_path.contains("my_project-"));
        assert!(storage.milvus_lite.db_path.contains("my_project-"));
        assert_eq!(storage.milvus_remote.database, "my_project");
        assert_eq!(storage.milvus_remote.collection, "memories");
    }

    #[test]
    fn logical_database_name_uses_project_directory_name() {
        assert_eq!(
            logical_database_name_for_user(Path::new("/tmp/My Project!"), "project", None),
            "my_project"
        );
        assert_eq!(
            logical_database_name_for_user(Path::new("/"), "project", None),
            "memory"
        );
    }

    #[test]
    fn logical_database_name_uses_global_username() {
        assert_eq!(
            logical_database_name_for_user(
                Path::new("/tmp/ignored"),
                "global",
                Some("Reiko User!")
            ),
            "reiko_user"
        );
    }

    #[test]
    fn runtime_storage_names_use_logical_database_name() {
        let mut storage = StorageConfig::default();
        normalize_storage_for_runtime(&mut storage, Path::new("/tmp/My Project!"), "project");

        assert!(storage.qdrant.storage_path.contains("my_project-"));
        assert!(storage.milvus_lite.db_path.contains("my_project-"));
        assert_eq!(storage.milvus_remote.database, "my_project");
        assert_eq!(storage.milvus_remote.collection, "memories");
    }
}
