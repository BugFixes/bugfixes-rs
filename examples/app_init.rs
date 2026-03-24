fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_bugfixes()?;

    bugfixes::info!("server starting on {}", ":3000")?;
    bugfixes::warn!("slow request threshold set to {}ms", 250)?;

    Ok(())
}

fn init_bugfixes() -> Result<(), Box<dyn std::error::Error>> {
    // Missing API keys do not break startup. Logs still print locally and
    // remote reporting is skipped until a key is configured.
    bugfixes::init_global_from_env()?;
    bugfixes::install_global_panic_hook();
    Ok(())
}
