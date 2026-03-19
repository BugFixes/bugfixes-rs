# bugfixes

Rust logging client for Bugfixes.

It provides:

- local log output with file and line capture
- global logging macros for application code
- explicit logger instances for dependency injection
- optional remote reporting to `POST /log`
- optional panic and bug reporting to `POST /bug`
- local-only logging paths for operational noise

## Install

```toml
[dependencies]
bugfixes = "0.1.0"
```

## Environment

Remote reporting is configured through:

- `BUGFIXES_AGENT_KEY`
- `BUGFIXES_AGENT_SECRET`
- `BUGFIXES_LOG_LEVEL`
- `BUGFIXES_LOCAL_ONLY`
- `BUGFIXES_SERVER`

If the agent credentials are missing, logs still print locally and remote reporting is skipped.

## Examples

Runnable examples live in [`examples/`](examples):

- `cargo run --example app_init`
- `cargo run --example explicit_logger`
- `cargo run --example local_only`

## Recommended app setup

For most binaries, initialize the global logger once at startup and then use the macros everywhere else:

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_bugfixes()?;

    bugfixes::info!("server starting on {}", ":3000")?;
    bugfixes::warn!("slow request threshold set to {}ms", 250)?;

    Ok(())
}

fn init_bugfixes() -> Result<(), Box<dyn std::error::Error>> {
    bugfixes::init_global_from_env()?;
    bugfixes::install_global_panic_hook();
    Ok(())
}
```

That is usually all you need. You do not need a custom init function to fall back to local-only mode when credentials are absent.

## Explicit logger instances

Use explicit logger instances in library code, services built around dependency injection, or anywhere you want logging to be passed in explicitly:

```rust
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
```

## Local-only logging

Use local-only logging when a message should never be sent remotely:

```rust
use bugfixes::{BugfixesLogger, init_global_local, local_logger};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    BugfixesLogger::local()?.info("warming cache")?;
    local_logger().warn("retry loop is still booting")?;

    init_global_local()?;
    bugfixes::local::info!("server started on {}", ":3000")?;
    bugfixes::local::warn!("health check is not ready yet")?;

    Ok(())
}
```

## API shape

The crate supports both:

- explicit logger instances for libraries and injected dependencies
- global macros for application code
- `local_logger()` for direct local-only logging
- `bugfixes::local::{...}` macros for namespaced local-only logging

Typical macro usage looks like this:

```rust
bugfixes::debug!("loaded {}", 3);
bugfixes::info!("started");
bugfixes::warn!("slow request");
bugfixes::error!("db error: {}", "timeout")?;
```

Panic capture is available either explicitly:

```rust
let logger = bugfixes::BugfixesLogger::from_env()?;
let _ = logger.report_panic_payload(&"worker crashed");
```

or through a global hook:

```rust
bugfixes::init_global_from_env()?;
bugfixes::install_global_panic_hook();
```
