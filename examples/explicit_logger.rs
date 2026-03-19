use bugfixes::BugfixesLogger;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let logger = BugfixesLogger::from_env()?;
    handle_request(&logger)?;
    Ok(())
}

fn handle_request(logger: &BugfixesLogger) -> Result<(), Box<dyn std::error::Error>> {
    logger.info("request started")?;
    logger.warn("upstream latency is elevated")?;
    Ok(())
}
