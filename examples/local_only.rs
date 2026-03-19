use bugfixes::{BugfixesLogger, init_global_local, local_logger};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    BugfixesLogger::local()?.info("warming cache")?;
    local_logger().warn("retry loop is still booting")?;

    init_global_local()?;
    bugfixes::local::info!("server started on {}", ":3000")?;
    bugfixes::local::warn!("health check is not ready yet")?;

    Ok(())
}
