fn main() {
    if let Err(error) = agent_memory::run() {
        if std::env::args().any(|arg| arg == "--agent") {
            eprintln!(
                "{}",
                serde_json::to_string_pretty(
                    &serde_json::json!({"ok": false, "error": error.to_string()}),
                )
                .unwrap()
            );
        } else {
            eprintln!("Error: {error}");
        }
        std::process::exit(1);
    }
}
