## Context

The Elastic CLI extension system installs extensions from GitHub or npm, derives the command name from an `elastic-<name>` package or repository, and invokes the registered executable as `elastic <name> ...`. The runner passes resolved Elastic context through `ELASTIC_*` environment variables and does not expose extension subcommands in built-in help.

ESDiag is currently a Rust binary named `esdiag`. It already supports environment-backed Elasticsearch output for `process`, `serve`, and `setup` through `ESDIAG_OUTPUT_*`, and it uses `ESDIAG_KIBANA_URL` for Kibana links and setup behavior. It does not currently consume the Elastic CLI extension environment names directly, and a GitHub extension install will not compile Rust unless the extension package provides an install-time entrypoint strategy.

## Goals / Non-Goals

**Goals:**

- Make ESDiag invokable as `elastic diag <args...>` through the Elastic CLI extension system.
- Keep the existing `esdiag` binary, saved hosts, keystore, Job model, and diagnostic stages intact while intentionally renaming the standalone `upload` command and `--upload` flag to `send` and `--send`.
- Provide a focused `elastic diag` command profile from the same native Rust binary.
- Allow Elastic CLI-provided Elasticsearch and Kibana context to drive transient Collect targets without saved host setup.
- Support `.service` active-context target references from extension-provided environment variables.
- Support `.context.service` named-context target references through native Elastic CLI config loading.
- Support a frequently changing input context and a separately configured, relatively fixed output context.
- Keep native Elastic CLI config support read-only for this change.
- Support Cloud-admin proxy targets only when a context reference supplies a deployment identifier and explicit application.
- Keep root `cargo install --path .` and repo-root build/test workflows functional after introducing the workspace.
- Make the `elasticrc` library independently publishable and consumable by Rust projects outside ESDiag.
- Keep the extension runtime compatible with a separately specified self-contained native package registered as `diag`.
- Preserve cross-platform behavior for macOS, Linux, and Windows.

**Non-Goals:**

- Do not move ESDiag into the Elastic CLI core repository.
- Do not replace `~/.esdiag/hosts.yml` or the ESDiag keystore.
- Do not require users to migrate existing `ESDIAG_*` environment variables.
- Do not add new receiver, processor, or exporter trait implementations.
- Do not add extension-provided nested help to the Elastic CLI, since the current extension system does not support that.
- Do not publish an npm package as part of this change.
- Do not write or mutate `.elasticrc` files in this change.
- Do not define remote extension publication, native artifact selection, or installer upgrades in this change.

## Decisions

### Use `elastic-diag` as the extension entrypoint

The extension-facing executable will be the same compiled Rust binary installed under the name `elastic-diag`. The executable name selects an extension-specific Clap profile and renders usage as `elastic diag`, while `esdiag` continues to select the standalone profile. The custom environment marker remains useful for tests and development wrappers but is not the primary identity mechanism.

The short name is an intentional override of the repository and standalone binary name. The separate self-contained packaging change owns the installer mechanism needed to preserve that name.

### Keep distribution separate from runtime behavior

This change defines the Rust command profile and context behavior consumed by an extension installation. A separate OpenSpec change owns native artifact selection, checksums, installer naming, license packaging, publication, and upgrades. Local development may continue to use the existing registration wrapper while the runtime profile is implemented.

### Expose a focused extension command profile

The initial extension profile exposes `collect`, `process`, `send`, `setup`, `job`, `output`, `help`, and `version`. Standalone deployment and credential administration remain under `esdiag`: `local`, `serve`, `init`, `host`, `keystore`, and `agent` are omitted from extension help and parsing.

The extension uses `send` for transmitting a bundle to Elastic Upload Service
because that operation is the Send stage. The same terminology leaves room for
future Send adapters such as object storage.

### Keep context mapping in ESDiag core

ESDiag core will consume Elastic CLI context variables through a typed context adapter. Active Elasticsearch and Kibana values are runtime-only input credentials, not generic field-by-field fallbacks for `ESDIAG_OUTPUT_*`. Output contexts are resolved separately as complete deployments so a frequently changing input context can never silently become the Export destination.

The adapter constructs stage-aware transient resolved hosts: Elasticsearch or Kibana with the Collect role for input, Elasticsearch with the output role for Export, and Kibana with the View role when an output deployment needs it. It keeps service credentials separate and persists none of them.

### Keep packaging non-authoritative

Installer metadata selects and verifies the binary but does not parse ESDiag arguments, resolve contexts, or implement authentication. The shared Rust execution layer remains authoritative for Job construction and execution.

Alternative considered: retain a JavaScript wrapper that locates `esdiag` on `PATH`. Rejected because native Elastic CLI users may not have Node.js, independent installations can drift in version, and the extension would not be self-contained.

### Add active-context leading-dot target references

ESDiag will support leading-dot target references in command arguments that already accept remote targets, saved hosts, URLs, or local paths. The grammar is:

```text
.service
```

`.service` resolves the named service from the active Elastic CLI context passed through the extension environment. Service names include canonical names and short aliases:

- `elasticsearch` or `es`
- `kibana` or `kb`

For arguments that can be multiple kinds of input, such as `process <input> [output]`, resolution order will be:

1. If the value starts with `.` and the value is a known service name or alias, resolve it as an active Elastic context target reference.
2. Otherwise, try the existing saved-host resolution path.
3. Otherwise, continue with existing URL, service-link, local file, directory, and stream handling.

This gives `elastic diag collect .es ./out` an explicit remote-target meaning while keeping existing saved-host and local path behavior. A local hidden path that would otherwise look like a context target can be written with an explicit filesystem prefix such as `./.es`.

The extension never guesses an application. `elastic diag process` without an input fails with guidance to choose `.es`, `.kb`, or an explicit named-context target even when an output context is configured. For example, after selecting active context `prod` and configuring output context `monitoring`, `elastic diag process .es` Collects Elasticsearch data from `prod`, Processes it, and Exports the documents to `monitoring`.

### Add native Elastic CLI config support for named contexts

ESDiag will support a second leading-dot grammar once native Elastic CLI config loading is available:

```text
.context.service
```

`.context.service` resolves the named service from a specific Elastic CLI context in `.elasticrc.yml` or the configured Elastic CLI config file. The resolver parses from the rightmost segment as the service name. This preserves room for context names that contain dots, so `.prod.us-west.es` means context `prod.us-west`, service `elasticsearch`.

For mixed target arguments, `.context.service` resolution uses the same leading-dot precedence rule as `.service`: only references whose rightmost segment is a known service name or alias are handled as Elastic context targets; everything else falls through to existing saved-host, URL, and local path handling.

This enables cross-context workflows such as `elastic diag process .prod.es .diag.es` and standalone workflows such as `esdiag process .prod.elasticsearch .diag.elasticsearch`, once ESDiag can resolve the relevant Elastic CLI config.

Alternative considered: have the `elastic-diag` wrapper preload all contexts into environment variables. The current Elastic CLI extension runner only provides one resolved active context, so named-context support belongs in ESDiag's own config resolver unless the upstream extension API grows multi-context support.

### Configure a separate named output context

`elastic diag output set <context>` stores a symbolic Elastic CLI context reference as the default output deployment. `elastic diag output show` and `elastic diag output clear` inspect and remove it. ESDiag persists the context name and any required non-secret config-file identity, never resolved URLs or credentials, and never mutates `.elasticrc`.

An output context resolves as a deployment rather than a single URI: Elasticsearch is required for Export, while Kibana is optional for ordinary processing and required by commands that install assets or create viewer links. Elasticsearch and Kibana authentication remain separate.

Application configuration and saved Jobs gain typed Elastic context reference variants with backward-compatible deserialization. Existing saved-host names and existing Job input/output variants retain their current serialized meaning; users do not need to migrate them before upgrading.

Output resolution order is:

1. An explicit target in the command's existing output positional, including `.monitoring.es`.
2. The configured Elastic CLI output context.
3. The existing standalone ESDiag output deployment where that profile permits it.
4. A fail-closed error.

No additional output-context option is introduced. A leading-dot context reference in output position resolves as an output deployment because target adaptation is stage-aware. The active input context is never an implicit output fallback. This prevents a changing customer or production input context from accidentally receiving processed diagnostic documents.

### Add Cloud-admin resource target references

The Elastic CLI `cloud` service supplies management-plane credentials; it is not an `Application` and `.cloud` alone is not a collectable target. Cloud-admin collection requires a deployment selector:

```text
.cloud/<deployment-id>/<application>
.context.cloud/<deployment-id>/<application>
```

The application is always explicit, keeping the no-guessing rule consistent and preserving an extensible grammar if the proxy later supports Kibana or another application. Initially only `es` and `elasticsearch` are accepted; unsupported applications fail before client construction. The resolver combines the context's Cloud URL and API key with the deployment identifier to produce a concrete Elasticsearch target using the Cloud admin route and an `ElasticCloudHosted` platform hint.

The existing saved-host template syntax remains independently supported:

```text
<saved-template>://<deployment-id>[/<application>]
```

Context Cloud references do not create or require a saved host, while saved-template references continue to obtain their route and credentials from ESDiag host storage.

### Match Elastic CLI config semantics where target resolution depends on them

Native config support needs read parity for target resolution. Read parity includes config discovery order, explicit config-file overrides, YAML/JSON parsing, structural validation, resolver expression handling, OS secret resolvers, inline-secret compatibility, and loose-permission warnings.

The resolver should keep parity scoped to context and service resolution. Command allow/block policy and Elastic CLI banner settings may be parsed or preserved as config data, but they do not change ESDiag command availability unless a future change explicitly adopts that policy model.

Alternative considered: support only inline `.elasticrc.yml` values at first. That would be fast to implement, but it would fail for the normal Elastic CLI path where secrets are commonly stored as resolver expressions backed by the OS keychain.

For this change, `elasticrc` write support is explicitly deferred. The crate may design public types with future writing in mind, but implementation work should focus on read-only resolution for named input targets, configured output deployments, and Cloud-admin resource references.

The service model should match Elastic CLI's currently supported config service blocks. `elasticsearch` and `kibana` resolve directly to applications. `cloud` may be parsed only as the credential and base-URL source for a resource-qualified Cloud admin route. Logstash (`logstash` / `ls`) is deferred until the Elastic CLI config schema supports it.

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

The Elastic CLI config implementation should live in a dedicated workspace library crate named `elasticrc`, with the main `esdiag` crate depending on it for named-context resolution behind an `elasticrc` Cargo feature. This feature should be enabled in the default feature set. For this change, the crate owns read-only config file discovery, parsing, validation, resolver expressions, OS secret store integration, and inline-secret warnings. The `esdiag` crate owns conversion from resolved config values into stage-aware transient hosts, output deployments, and Cloud-admin routes.

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

### Publish `elasticrc` as an independent library crate

`elasticrc` is a reusable integration boundary rather than an ESDiag-internal module. Its public API owns Elastic CLI config discovery, raw typed configuration, context and service selection, lazy resolver evaluation, redacted authentication, and errors. It must not expose or depend on `KnownHost`, `Uri`, `OutputDeployment`, Job stages, or other ESDiag domain types; callers perform those adaptations.

The crate is licensed under Apache-2.0 independently of the Elastic-2.0 `esdiag` package. Its manifest includes registry metadata, a focused README with a minimal load-and-resolve example, repository and documentation links, license and Rust-version declarations, and registry versions for every runtime dependency. ESDiag uses a combined path-and-version dependency so workspace development uses local source while packaged ESDiag resolves the published crate.

Publication readiness is verified with `cargo package -p elasticrc`, `cargo publish --dry-run -p elasticrc`, public documentation tests, and an external-consumer fixture that depends only on the packaged crate. CI also checks the declared minimum Rust version.

Public compatibility follows semantic versioning. Config loading remains side-effect-free, and resolver execution remains lazy and service-scoped so downstream projects can safely inspect configuration without executing resolver expressions.

Alternative considered: keep `elasticrc` private until ESDiag stabilizes its context model. Rejected because the crate already models an upstream Elastic CLI format independently of ESDiag, and publishing it allows other Rust extensions to share one tested implementation instead of duplicating secret resolver and schema behavior.

### Make help context-aware for Elastic CLI invocations

When the extension profile is active through the `elastic-diag` executable name or the development marker, help output includes Elastic CLI-specific examples such as `elastic diag collect .es ./out` and mentions `.service` target references. A bare `elastic diag` invocation can therefore provide extension-specific guidance. Current Elastic CLI releases consume `--help` before extension dispatch, so users must use `elastic diag help [COMMAND]` for delegated Clap help. This keeps normal `esdiag --help` focused on standalone usage while improving discoverability for extension users. Shell completions remain out of scope.

## Risks / Trade-offs

- Runtime and distribution land separately → Keep the extension profile contract stable and verify packaging against it in the isolated distribution change.
- Input and output contexts may differ → Persist typed symbolic references and resolve each direction independently at Job execution.
- An omitted application is ambiguous in a multi-service context → Require `.es`, `.kb`, or another explicit application selector and never guess, including for Cloud-admin references.
- Leading-dot references can resemble hidden local files → Reserve the leading-dot grammar only when the service segment is a known service name or alias, and document `./.name` for local hidden paths.
- Explicit `.context.service` references require reading Elastic CLI config directly → Isolate `.elasticrc` loading behind the `elasticrc` crate so active-context env resolution and saved-host behavior remain independent.
- Cloud management credentials are not an application target → Require a fully qualified `.context.cloud/<id>/<application>` reference and materialize a Cloud admin route.
- Elastic CLI config parity includes OS-specific secret stores and command execution resolvers → Use `keyring-core` for credential access, use native keyring store crates where practical, and implement command resolvers with bounded execution, platform-specific errors, and focused tests.
- Resolver command parity intentionally avoids shell interpretation → Document the safer argv-based behavior, remove inherited `ELASTIC_*` credentials from child environments, and test rejection of shell-only syntax.
- Workspace conversion can break root install flows → Preserve root package metadata and test `cargo install --path .`.
- A path-only workspace dependency prevents registry packaging → Declare both the local path and published `elasticrc` version in ESDiag.
- Publishing exposes API compatibility obligations → Keep ESDiag adaptation types outside the crate, document the public surface, and apply semantic versioning.
- Dependency MSRV can exceed the crate declaration → Test the packaged crate and its default feature set with the declared Rust version.
- Credentials passed through environment variables can be inherited by child processes → Keep context credentials runtime-only, redact them, and construct minimal child environments.
- The extension profile can drift from standalone `esdiag` → Share Job construction and execution while testing the smaller command grammar independently.
- The Elastic CLI extension feature is experimental → Keep the extension-specific surface small so changes in the Elastic CLI installer contract require limited updates.
