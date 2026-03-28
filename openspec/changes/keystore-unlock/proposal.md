## Why

`esdiag` can already store host credentials in the encrypted keystore, but normal CLI runs still require the keystore password to be supplied through environment variables or an interactive prompt at the moment of use. That makes agentic or repeated command-line usage awkward because users either need to keep secrets in shell state or re-enter the password for each workflow.

## What Changes

- Add a CLI-managed keystore unlock lease file at `~/.esdiag/keystore.unlock` so users can unlock once and let later `esdiag` commands reuse the keystore password until the lease expires.
- Add `esdiag keystore unlock`, `lock`, and `status` commands with a default 24-hour unlock duration, optional human-friendly `--ttl` override, and a maximum duration of 30 days.
- Define unlock lease expiration behavior so expired unlock files are treated as locked state and are deleted on read when possible.
- Add interactive bootstrap behavior for `keystore unlock` when no keystore exists, while failing safely in non-interactive shells.
- Add `esdiag keystore password` to rotate the keystore password by re-encrypting the keystore with a new password.
- Change secret management semantics so `esdiag keystore add` creates new secrets only and a new `esdiag keystore update <secret>` command is required to modify an existing secret.
- Prefer masked interactive prompts for secret material entry so `keystore add` and `keystore update` can accept inline values but also prompt for missing `--apikey` or `--password` values in interactive shells.
- Store the unlock lease using minimal encryption instead of plaintext so the cached password is not exposed by casual local file inspection.

## Capabilities

### New Capabilities
- `cli-keystore-lifecycle`: CLI commands and local unlock lease behavior for unlocking, locking, status inspection, password rotation, and explicit secret add/update flows.

### Modified Capabilities
- `host-secret-store`: Host secret resolution now accepts a valid CLI unlock lease as a password source in addition to scoped in-memory passwords and `ESDIAG_KEYSTORE_PASSWORD`.
- `web-keychain-session-unlock`: User-mode web sessions may consume an existing valid CLI unlock lease as an initial in-memory unlock source without persisting or refreshing that file.

## Impact

- Affects Rust CLI command parsing and command handlers in `src/main.rs`.
- Affects keystore password resolution and local keystore file handling in `src/data/keystore.rs`.
- Affects secret-backed host authentication flows in `src/data/known_host.rs` and any command paths that construct clients or exporters from known hosts.
- Requires new CLI and data-layer tests covering unlock lease TTLs, bootstrap behavior, password rotation, and add/update safety rules.
