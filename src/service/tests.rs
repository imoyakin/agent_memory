#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removable_storage_health_fails_when_root_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("project");
        fs::create_dir_all(root.join(".memory/qdrant/store")).unwrap();
        let mut config = UserConfig::default();
        config.storage.qdrant.storage_path = ".memory/qdrant/store".to_string();
        fs::remove_dir_all(&root).unwrap();

        assert!(!removable_storage_accessible(&root, &config));
    }

    #[test]
    fn removable_storage_failure_counter_stops_after_three_failures() {
        assert!(!removable_storage_should_stop(2));
        assert!(removable_storage_should_stop(3));
    }

    #[test]
    fn tracked_agent_loss_only_stops_when_all_known_agents_are_gone() {
        assert!(!service_lost_all_tracked_agents(&[], &[]));
        assert!(!service_lost_all_tracked_agents(&[10, 20], &[20]));
        assert!(service_lost_all_tracked_agents(&[10, 20], &[]));
    }
}
