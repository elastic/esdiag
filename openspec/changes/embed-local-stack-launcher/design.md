## Context

The standalone `bin/esdiag-local` launcher owns secure Compose state and
currently always creates ESDiag setup and web-service containers. The Rust
binary already embeds versioned assets and `esdiag init` can detect generated
local-stack state for local output configuration.

## Goals / Non-Goals

**Goals:**

- Make the installed ESDiag binary sufficient for local-stack lifecycle.
- Avoid pulling an ESDiag image when a compatible native binary owns
  configuration and processing.
- Preserve the full containerized web experience as an explicit mode.
- Make local-stack startup an optional, user-approved initialization stage.
- Keep the standalone script usable for script-first users.

**Non-Goals:**

- Remove the standalone release asset or its self-update behavior.
- Migrate container ESDiag user state into host-native ESDiag state.

## Decisions

### Own native lifecycle in Rust

`esdiag local` parses, validates, and executes its lifecycle in Rust. It owns
state reads and writes, Compose file generation, runtime invocation, readiness,
asset setup, browser launch, and native `serve` supervision. It does not render,
write, or execute `bin/esdiag-local`.

The standalone Bash artifact remains a script-first entry point. Both
implementations consume the same documented stack-state schema and conformance
fixtures, but neither invokes the other.

### Resolve stack mode once per state directory

`--stack=auto|full|core` is parsed by each entry point. For `esdiag local`,
`auto` resolves to `core`; `full` remains an explicit container-runtime
override. The standalone launcher selects core only for an exact matching
native binary and otherwise resolves auto to full. Both persist the resolved
mode after readiness and retain it on later automatic starts.

This prevents installation, removal, or replacement of a PATH binary from
silently changing an already-running deployment. A mode transition removes
only superseded ESDiag containers; it does not migrate or delete the
full-mode ESDiag volume unless a confirmed reset requests it.

### Generate mode-specific Compose and lifecycle behavior

Full mode retains the current Compose services, port 2501, ESDiag volume,
one-shot asset setup, and browser-oriented service endpoint. Core mode
generates only Elasticsearch and Kibana containers, omits ESDiag image pulls
and container volume creation, then starts the running binary as a managed
native `serve --mode user` child using the generated output and Kibana runtime
configuration.

The protected `.env` continues to own generated Elasticsearch credentials and
API key in both modes. Native `esdiag init` reads this local-stack state only
after the user selected local processing; it writes any host-native ESDiag
configuration to the normal native state location.

The launcher records native-service ownership in restrictive state files and
captures its logs under the managed log directory. Lifecycle operations verify
the recorded process still identifies as the expected version-matched ESDiag
service before signaling it; stale or mismatched records are removed without
signaling the referenced process. `status`, `logs`, `restart esdiag`, `down`,
and `reset` use that same ownership check.

### Execute commands through the full-mode service

`esdiag-local exec [launcher-options] -- <esdiag-arguments>` is the sole
container-only replacement for a native `esdiag` command. It is available only
when valid managed state records full mode and the full service dependencies are
running. It uses the state-derived Compose invocation to run an ephemeral
command in the `esdiag` service definition, preserving the selected image,
service network, environment, and `esdiag-data` volume. It never downloads a
second wrapper artifact or infers credentials from a caller's shell.

The delimiter separates launcher options from opaque ESDiag arguments. The
subcommand preserves terminal input, output, error, and process exit status so
both a human and an Agent Skill can use interactive `init` and finite commands
such as `process`, `job run`, and `agent ask`.

The wrapper mounts the current working directory for relative input paths and
accepts explicit additional host-path mounts. It rejects paths that cannot be
made visible to the command container rather than silently processing a
different path.

The invoked ESDiag command needs network-internal Elasticsearch and Kibana
addresses, while browser handoffs must use host-published loopback addresses.
Full-mode state therefore distinguishes an internal Kibana service URL from a
public Kibana viewer URL. Container initialization recognizes the managed full
stack and uses its container-owned user state; it MUST NOT offer or attempt to
start a nested core stack.

### Treat stack state as shared and ESDiag user state as mode-owned

Both entry points use the same stack state directory, restrictive `.env`,
schema version, generated credentials, port keys, resolved mode record, and
lifecycle-log locations. The mode record is authoritative: an entry point that
cannot operate the active mode fails without changing it. This allows the
binary to inspect or operate a full standalone stack and prevents a
standalone-only environment from accidentally replacing core mode.

The ESDiag runtime state is intentionally not shared in phase one. Full mode
retains its container volume; core mode uses the native binary's state. An
explicit mode switch never copies, mounts, or merges that state and always
explains the separation. A future one-way container-to-native extraction can
be added as an explicit migration feature.

### Preserve semantic parity with entry-point-specific output

Full and core setup paths must install the same assets and observe the same
license/readiness rules, even though full mode reaches services through Compose
hostnames and core mode reaches host-published endpoints. Lifecycle controls
must report the same state and safe recovery guidance.

The native command follows the existing finite structured-outcome convention:
progress goes to stderr and the final result goes to stdout. The standalone
script retains its shell-oriented output for policy-constrained users. This is
an intentional presentation difference, not a lifecycle or state divergence.

### Integrate core launch into initialization as a narrow exception

During local processing setup, `esdiag init` first checks for usable local-stack
state. If none exists, it asks whether to start a core deployment. On approval,
it calls the same Rust lifecycle used by `esdiag local up --stack=core`, waits
for completion, then resumes local output setup. On decline, it returns to
remote-output selection without creating state.

## Risks / Trade-offs

- [Bash is unavailable or too old] → The Rust binary path remains available;
  the standalone launcher documents and validates its Bash 3.2 requirement.
- [Native and full modes have separate ESDiag user state] → Persist stack mode,
  never migrate implicitly, and document that switching modes does not transfer
  host settings, jobs, or secrets.
- [A mode transition leaves an obsolete service] → Reconcile and remove only
  the ESDiag setup/service containers when leaving full mode, then verify the
  selected mode before committing it.
- [External standalone script selects an incompatible native binary] → Require
  an exact version match and fall back to full mode with a reason.
- [Core users expect the web UI] → Status and startup output explicitly state
  that core mode serves the same local web UI through the managed native binary.
- [Container CLI starts a nested stack] → `exec init` receives an explicit
  managed-full-stack marker and container-network output configuration.
- [Container Agent Builder returns an unreachable link] → Store distinct
  internal service and host-published viewer URLs and return the latter.
- [Container CLI cannot see an archive] → Mount the working directory and
  require explicit mounts for paths outside it.
- [A stale PID targets an unrelated process] → Verify executable identity,
  command role, and managed service metadata before sending a signal; otherwise
  remove only the stale record.
- [Two entry points interpret state differently] → Version the shared stack
  schema, define cross-entry conformance fixtures, and reject unsupported
  active modes without mutation.
- [A mode switch loses user configuration] → Do not claim seamless switching;
  preserve inactive runtime state and state clearly that phase one has no
  migration.

## Migration Plan

1. Existing state without a stack mode is interpreted as full mode, preserving
   today’s running containerized deployment.
2. New `auto` deployments persist their resolved mode after successful startup.
3. Users switch existing state explicitly with `up --stack=core` or
   `up --stack=full`; normal `up` does not change mode and a switch never
   migrates runtime-user configuration.
4. Rollback consists of selecting `--stack=full`; the standalone script and
   existing full-mode volume remain available, while any core native state
   remains separately intact.
