pub(crate) fn ensure_backend(root: &Path, config: &RuntimeConfig) -> Result<()> {
    ensure_qdrant_server(root, &config.storage.qdrant)?;
    ensure_qdrant_collection(config)?;
    Ok(())
}
