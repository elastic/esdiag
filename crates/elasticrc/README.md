# elasticrc

`elasticrc` lets Rust programs use connections configured for the Elastic CLI.
It discovers the user's config file, selects an Elasticsearch, Kibana, or
Cloud service from the current or a named context, and resolves that service's
URL and authentication.

The library is intended for Elastic CLI extensions and other integrations that
need context-aware connections without invoking the CLI, duplicating its
resolver behavior, or copying credentials into another configuration store.
It reads `.elasticrc`, `.elasticrc.json`, `.elasticrc.yaml`, and
`.elasticrc.yml`; it never modifies them.

## Usage

```rust
use elasticrc::{ConfigFile, ServiceKind};

let config = ConfigFile::load_with_options(None, None)?;
let elasticsearch =
    config.resolve_current_service(ServiceKind::Elasticsearch)?;

println!("Connecting to {}", elasticsearch.url);

# Ok::<(), elasticrc::Error>(())
```

Loading a config parses and validates its structure without executing resolver
expressions. `resolve_service` and `resolve_current_service` evaluate only the
requested service.

Supported resolvers:

- `env`, `file`, and `cmd`
- `pass`
- macOS `keychain`
- Linux `secret_service`
- Windows `credential_manager`

## Security

API keys and passwords are redacted in debug output. Accessing a resolved
secret requires an explicit `expose_secret()` call.

`cmd` and `pass` run programs directly, without shell interpretation. They have
execution time and output limits, and child processes do not inherit Elastic
CLI credential variables. Use command resolvers only from trusted config files.

## Compatibility

The crate follows semantic versioning and declares its minimum Rust version in
`Cargo.toml`. The upstream Elastic CLI config format remains experimental, so
new supported config fields may be added in minor releases.

## License

[Apache License 2.0](LICENSE). See [NOTICE.txt](NOTICE.txt) for attribution.
