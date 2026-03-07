# bugfixes-logs

Rust logging client for Bugfixes, extracted from the behavior of the Go `logs` package in this repository.

Current scope:

- local log output
- caller file/line capture
- level filtering
- optional remote reporting to `POST /log`
- error/fatal helpers

Not ported yet:

- Go HTTP middleware utilities
- framework-specific integrations

## Install

```toml
[dependencies]
bugfixes-logs = { path = "rust/bugfixes-logs" }
```

## Example

```rust
use bugfixes_logs::{BugfixesLogger, init_global_local};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let logger = BugfixesLogger::local()?;
    let line = logger.info("server started on :3000")?;
    assert_eq!(line, "Info: server started on :3000");

    init_global_local()?;
    let line = bugfixes_logs::info!("server started on {}", ":3000")?;
    assert_eq!(line, "Info: server started on :3000");

    Ok(())
}
```

Use `BugfixesLogger::from_env()` to enable remote reporting through:

- `BUGFIXES_AGENT_KEY`
- `BUGFIXES_AGENT_SECRET`
- `BUGFIXES_LOG_LEVEL`
- `BUGFIXES_LOCAL_ONLY`
- `BUGFIXES_SERVER`

## API shape

The crate supports both:

- explicit logger instances, which are best for libraries and dependency injection
- global macros, which are the most idiomatic application-facing Rust API

The macros preserve the Go package's functional intent while fitting normal Rust call patterns:

```rust
bugfixes_logs::debug!("loaded {}", 3);
bugfixes_logs::info!("started");
bugfixes_logs::warn!("slow request");
bugfixes_logs::error!("db error: {}", "timeout")?;
```
