## Context

The Elastic CLI extension system installs extensions from GitHub or npm, derives the command name from an `elastic-<name>` package or repository, and invokes the registered executable as `elastic <name> ...`. The runner passes resolved Elastic context through `ELASTIC_*` environment variables and does not expose extension subcommands in built-in help.

ESDiag is currently a Rust binary named `esdiag`. It already supports environment-backed Elasticsearch output for `process`, `serve`, and `setup` through `ESDIAG_OUTPUT_*`, and it uses `ESDIAG_KIBANA_URL` for Kibana links and setup behavior. It does not currently consume the Elastic CLI extension environment names directly, and a GitHub extension install will not compile Rust unless the extension package provides an install-time entrypoint strategy.

## Goals / Non-Goals

**Goals:**

- Make ESDiag invokable as `elastic diag <args...>` through the Elastic CLI extension system.
- Keep the existing `esdiag` binary, command grammar, host file, keystore, and diagnostics pipeline intact.
- Allow Elastic CLI-provided Elasticsearch, Kibana, and Cloud context to drive env-backed ESDiag workflows without saved host setup.
- Support `.service` active-context target references from extension-provided environment variables.
- Support `.context.service` named-context target references through native Elastic CLI config loading.
- Keep native Elastic CLI config support read-only for this change.
- Support Elastic Cloud service targets where ESDiag can map the Cloud API key path into its existing known-host model.
- Keep root `cargo install --path .` and repo-root build/test workflows functional after introducing the workspace.
- Provide an installation/discovery shape compatible with the Elastic CLI extension installer.
- Preserve cross-platform behavior for macOS, Linux, and Windows.

**Non-Goals:**

- Do not move ESDiag into the Elastic CLI core repository.
- Do not replace `~/.esdiag/hosts.yml` or the ESDiag keystore.
- Do not require users to migrate existing `ESDIAG_*` environment variables.
- Do not add new receiver, processor, or exporter trait implementations.
- Do not add extension-provided nested help to the Elastic CLI, since the current extension system does not support that.
- Do not publish an npm package as part of this change.
- Do not write or mutate `.elasticrc` files in this change.

## Decisions

### Use `elastic-diag` as the extension entrypoint

The extension-facing executable will be named `elastic-diag`, which the Elastic CLI derives to the short command `diag`. The entrypoint will forward all arguments to the existing ESDiag execution layer so commands such as `elastic diag process`, `elastic diag setup`, and `elastic diag serve` preserve the same behavior as `esdiag process`, `esdiag setup`, and `esdiag serve`.

Alternative considered: install the existing `elastic/esdiag` repository directly. This would derive the short name `esdiag`, producing `elastic esdiag`, and would not satisfy the issue's requested `elastic diag` UX.

### Package the extension from the existing ESDiag repository

The first extension packaging path will live in the existing ESDiag repository. It will not require a separate `elastic/elastic-diag` repository, and npm publication is deferred until the extension has been tested locally. The repository will provide an installer-compatible `elastic-diag` entrypoint and metadata for local/GitHub extension installation.

Alternative considered: create a dedicated extension repository or publish an npm package immediately. That would make command naming and installation cleaner, but it adds release surface before the extension behavior is proven locally.

### Require `esdiag` on PATH for the initial wrapper

The initial `elastic-diag` wrapper will require an `esdiag` executable to be available on `PATH`. If it is missing, the wrapper will fail gracefully with installation guidance that points users to the current Cargo-based install flow for this repository, such as `cargo install --git https://github.com/elastic/esdiag.git`. Once ESDiag publishes precompiled release binaries, the extension packaging can change to download or bundle the binary.

Alternative considered: compile or download `esdiag` during extension install. The current release flow does not yet provide precompiled binaries, and the Elastic CLI GitHub installer does not build Rust projects automatically.

### Keep context mapping in ESDiag core, not only in the wrapper

ESDiag will recognize Elastic CLI context variables as fallbacks in the same code path that currently reads `ESDIAG_OUTPUT_*` and `ESDIAG_KIBANA_URL`. Existing `ESDIAG_*` names remain authoritative when both are set. This makes the behavior testable in Rust and keeps direct `esdiag` invocations compatible with Elastic CLI-provided environments.

Alternative considered: map variables only inside the `elastic-diag` wrapper. That is simple for extension runs, but it makes the behavior harder to validate from the core binary and creates different semantics between direct and extension invocation.

### Keep the wrapper thin and non-authoritative

The wrapper should not parse ESDiag arguments, inspect diagnostic inputs, or implement auth logic. Its job is to satisfy Elastic CLI extension entrypoint discovery and delegate execution. Any required env translation may be present as a compatibility layer, but the core binary remains the source of truth for command behavior.

Alternative considered: build a richer extension command layer that rewrites commands or introduces extension-only options. That would create a second CLI surface and increase drift from `esdiag`.

### Mark Elastic CLI extension invocations in the ESDiag namespace

The `elastic-diag` wrapper will set `ESDIAG_ELASTIC_CLI=1` before delegating to the ESDiag execution layer. Core ESDiag may use this marker for extension-specific parsing hints, help text, and diagnostics because it is a wrapper-owned signal rather than a heuristic based on the presence of `ELASTIC_*` service context variables.

Alternative considered: use a generic marker such as `ELASTIC_CLI=1`. That is shorter, but it claims an upstream Elastic CLI namespace that is not currently part of the extension contract.

### Add active-context leading-dot target references

ESDiag will support leading-dot target references in command arguments that already accept remote targets, saved hosts, URLs, or local paths. The grammar is:

```text
.service
```

`.service` resolves the named service from the active Elastic CLI context passed through the extension environment. Service names include canonical names and short aliases:

- `elasticsearch` or `es`
- `kibana` or `kb`
- `cloud`

For arguments that can be multiple kinds of input, such as `process <input> [output]`, resolution order will be:

1. If the value starts with `.` and the value is a known service name or alias, resolve it as an active Elastic context target reference.
2. Otherwise, try the existing saved-host resolution path.
3. Otherwise, continue with existing URL, service-link, local file, directory, and stream handling.

This gives `elastic diag collect .es ./out` an explicit remote-target meaning while keeping existing saved-host and local path behavior. A local hidden path that would otherwise look like a context target can be written with an explicit filesystem prefix such as `./.es`.

Alternative considered: infer the collect source from `ESDIAG_ELASTIC_CLI=1` and active `ELASTIC_*` variables when the user provides only one positional. That is concise, but it becomes ambiguous when the active context contains multiple services and does not scale to cross-context workflows.

### Add native Elastic CLI config support for named contexts

ESDiag will support a second leading-dot grammar once native Elastic CLI config loading is available:

```text
.context.service
```

`.context.service` resolves the named service from a specific Elastic CLI context in `.elasticrc.yml` or the configured Elastic CLI config file. The resolver parses from the rightmost segment as the service name. This preserves room for context names that contain dots, so `.prod.us-west.es` means context `prod.us-west`, service `elasticsearch`.

For mixed target arguments, `.context.service` resolution uses the same leading-dot precedence rule as `.service`: only references whose rightmost segment is a known service name or alias are handled as Elastic context targets; everything else falls through to existing saved-host, URL, and local path handling.

This enables cross-context workflows such as `elastic diag process .prod.es .diag.es` and standalone workflows such as `esdiag process .prod.elasticsearch .diag.elasticsearch`, once ESDiag can resolve the relevant Elastic CLI config.

Alternative considered: have the `elastic-diag` wrapper preload all contexts into environment variables. The current Elastic CLI extension runner only provides one resolved active context, so named-context support belongs in ESDiag's own config resolver unless the upstream extension API grows multi-context support.

### Match Elastic CLI config semantics where target resolution depends on them

Native config support needs read parity for target resolution. Read parity includes config discovery order, explicit config-file overrides, YAML/JSON parsing, structural validation, resolver expression handling, OS secret resolvers, inline-secret compatibility, and loose-permission warnings.

The resolver should keep parity scoped to context and service resolution. Command allow/block policy and Elastic CLI banner settings may be parsed or preserved as config data, but they do not change ESDiag command availability unless a future change explicitly adopts that policy model.

Alternative considered: support only inline `.elasticrc.yml` values at first. That would be fast to implement, but it would fail for the normal Elastic CLI path where secrets are commonly stored as resolver expressions backed by the OS keychain.

For this change, `elasticrc` write support is explicitly deferred. The crate may design public types with future writing in mind, but implementation work should focus on read-only resolution for `.context.service` targets and active context parity.

The service model should match Elastic CLI's currently supported config service blocks. That means `elasticsearch`, `kibana`, and `cloud` may be parsed by the crate. ESDiag should expose `cloud` target references by mapping Elastic CLI Cloud URL/API key data into the existing ESDiag Cloud known-host code path. Logstash (`logstash` / `ls`) is deferred until the Elastic CLI config schema supports it.

### Use `keyring-core` for credential resolver integration

The `elasticrc` crate will depend on `keyring-core` for credential access abstractions and use native keyring-compatible stores for platform credential lookup. Implementation may freely use examples and connection patterns from the `keyring` crate to wire the application store selection, but `keyring-core` is the core dependency boundary for credentials.

Candidate platform store crates include:

- `apple-native-keyring-store` for macOS Keychain
- `windows-native-keyring-store` for Windows Credential Manager
- `zbus-secret-service-keyring-store` for Linux Secret Service

Other supporting dependencies should stay close to the existing project stack:

- `serde_json` for JSON config parsing and the existing YAML parser used by this repository for YAML config parsing unless a migration is already needed.
- `std::process::Command` with explicit args, no shell, and bounded timeout for compatible `cmd` and `pass` resolver behavior when no maintained native Rust equivalent is appropriate.
- `url` and `serde` for service block validation and typed config models.

Native keychain crates should be preferred over invoking `security`, `secret-tool`, or PowerShell when feature parity is practical. Command-backed resolvers remain necessary for Elastic CLI parity with `$(cmd:...)` and `$(pass:...)`, but they must be bounded and documented with the same trust warning as Elastic CLI.

### Protect resolved secrets with `redact`

Resolved secret values in the `elasticrc` crate should use the `redact` crate for debug/display-safe wrappers wherever secrets are held in typed structures. This is not a complete memory protection story, but it prevents accidental logging through common formatting paths and fits the crate boundary well.

Alternative considered: keep secrets as plain `String` values and rely on caller discipline. That is simpler, but this feature introduces multiple credential-loading paths and should make accidental disclosure harder by default.

### Avoid arbitrary shell execution in command resolvers

The `cmd` and `pass` resolver implementations must avoid arbitrary shell execution. Where a command resolver is supported, `elasticrc` should tokenize into an executable and explicit argument vector, execute with a bounded timeout, and reject forms that require shell interpretation. Documentation should still carry the same trust warning as Elastic CLI because command resolvers execute local programs from config.

Alternative considered: exactly mirror Elastic CLI's shell-command behavior. That is closer semantic parity, but it expands the attack surface and is unnecessary for the expected resolver use cases if explicit argv execution is documented.

### Implement native Elastic CLI config support as an `elasticrc` crate

The Elastic CLI config implementation should live in a dedicated workspace library crate named `elasticrc`, with the main `esdiag` crate depending on it for `.context.service` resolution behind an `elasticrc` Cargo feature. This feature should be enabled in the default feature set. For this change, the crate owns read-only config file discovery, parsing, validation, resolver expressions, OS secret store integration, and inline-secret warnings. The `esdiag` crate owns only the conversion from an `elasticrc` resolved service block into ESDiag's transient `KnownHost`/`Uri` model.

The current repository is a single Cargo package, so this change will introduce a Cargo workspace layout while keeping the existing package name and binary intact. The initial layout should be minimal:

```text
Cargo.toml
crates/
  elasticrc/
    Cargo.toml
    src/lib.rs
src/
```

Alternative considered: implement `.elasticrc` support inside `src/data`. That would be faster initially, but it would couple OS keychain and config-writer concerns to ESDiag's diagnostic domain and make reuse or independent testing harder.

### Make help context-aware for Elastic CLI invocations

When `ESDIAG_ELASTIC_CLI=1` is present, ESDiag help output may include Elastic CLI-specific examples such as `elastic diag collect .es ./out` and mention `.service` target references. This keeps normal `esdiag --help` focused on standalone usage while improving discoverability for extension users. Shell completions remain out of scope.

### Treat install packaging as separate from diagnostic runtime

The extension package/repository will provide metadata that the Elastic CLI installer can discover, such as a `package.json` `bin` entry or an executable in an expected location. The diagnostic runtime remains the Rust ESDiag binary. If the package needs to fetch or locate a released binary, that logic belongs in package installation scripts or release packaging rather than in the diagnostic processing path.

Alternative considered: require users to preinstall `esdiag` and register a local extension path only. That is useful for development, but not sufficient for a first-class install flow.

## Risks / Trade-offs

- Extension install cannot build arbitrary Rust repositories → Provide installer-compatible metadata and document whether the extension expects a bundled, downloaded, or preinstalled `esdiag` binary.
- Two environment variable families can diverge → Preserve `ESDIAG_*` precedence and document the fallback order.
- Invocation marker could be confused with Elastic CLI-provided context → Use `ESDIAG_ELASTIC_CLI=1` only as an invocation marker and continue to use `ELASTIC_*` only for service context.
- Leading-dot references can resemble hidden local files → Reserve the leading-dot grammar only when the service segment is a known service name or alias, and document `./.name` for local hidden paths.
- Explicit `.context.service` references require reading Elastic CLI config directly → Isolate `.elasticrc` loading behind the `elasticrc` crate so active-context env resolution and saved-host behavior remain independent.
- Elastic CLI config parity includes OS-specific secret stores and command execution resolvers → Use `keyring-core` for credential access, use native keyring store crates where practical, and implement command resolvers with bounded execution, platform-specific errors, and focused tests.
- Resolver command parity intentionally avoids shell interpretation → Document the safer argv-based behavior and test rejection of shell-only syntax.
- Workspace conversion can break root install flows → Preserve root package metadata and test `cargo install --path .`.
- Initial extension packaging depends on `esdiag` being on `PATH` → Fail with clear install guidance until precompiled release binaries are available.
- Credentials passed through environment variables can be inherited by child processes → Keep the wrapper minimal, avoid logging secret values, and rely on the Elastic CLI extension security model.
- `collect` still requires an explicit host argument → Support the existing command unchanged first, and only add active-context collection semantics if the implementation can keep clap usage unambiguous.
- The Elastic CLI extension feature is experimental → Keep the extension-specific surface small so changes in the Elastic CLI installer contract require limited updates.
