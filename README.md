# bugfixes

Rust logging client for Bugfixes, extracted from the behavior of the Go `logs` package in this repository.

Current scope:

- local log output
- caller file/line capture
- level filtering
- optional remote reporting to `POST /log`
- optional panic/bug reporting to `POST /bug`
- error/fatal helpers

Not ported yet:

- Go HTTP middleware utilities
- framework-specific integrations

## Install

```toml
[dependencies]
bugfixes = "0.1.0"
```

## Example

```rust
use bugfixes::{BugfixesLogger, init_global_local, local_logger};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let logger = BugfixesLogger::local()?;
    let line = logger.info("server started on :3000")?;
    assert_eq!(line, "Info: server started on :3000");

    let local_line = local_logger().info("stdout only")?;
    assert_eq!(local_line, "Info: stdout only");

    init_global_local()?;
    let line = bugfixes::info!("server started on {}", ":3000")?;
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

Panic capture is available either explicitly:

```rust
let logger = BugfixesLogger::from_env()?;
let _ = logger.report_panic_payload(&"worker crashed");
```

or through a global hook:

```rust
bugfixes::init_global_from_env()?;
bugfixes::install_global_panic_hook();
```

## API shape

The crate supports both:

- explicit logger instances, which are best for libraries and dependency injection
- global macros, which are the most idiomatic application-facing Rust API
- `local_logger()`, which is the direct local-only equivalent of Go's `logs.Local()`
- `local::{...}` macros, which are the local-only namespaced equivalent of the global macros

The macros preserve the Go package's functional intent while fitting normal Rust call patterns:

```rust
bugfixes::debug!("loaded {}", 3);
bugfixes::info!("started");
bugfixes::warn!("slow request");
bugfixes::error!("db error: {}", "timeout")?;
```

For local-only operational noise that should never be sent remotely:

```rust
let _ = bugfixes::local::info!("server started on {}", ":3000");
let _ = bugfixes::local::warn!("retry loop is warming up");
```
