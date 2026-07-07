# ESDiag

ESDiag (Elastic Stack Diagnostics) is an extract-transform-load tool for Elastic
Stack diagnostics. Its ETL stages are decoupled so that data can be collected and
bundled on one host, then read, processed, and exported on another.

## Language

### Job

The universal model of one diagnostic execution — the single data structure shared
by the CLI, the web server, and the executor. A job selects stages within three
phases (below); running one mints a `JobID` that identifies exactly one execution.
A *saved job* is a named, reusable job definition.
_Avoid_: pipeline (in Logstash/Elasticsearch a "pipeline" is a configuration of
transforms — an ESDiag job is stages of an execution, not a transform config),
workflow (collides with the Elastic Stack "Workflows" feature), operation, task

### Stages

A job is composed from six stages — single steps, not operations in their own
right. Underlying ETL roles: *extract* = Collect | Load; *transform* = Process;
*load* = Save | Export | Send.

**Collect**:
Call live product APIs to acquire a *new* diagnostic. Scoped to the API-collectable
products — Elasticsearch, Kibana, Logstash. Agent and platform diagnostics are
*product-provided* and enter via `Load`, not `Collect`.
_Avoid_: gather, receive, fetch

**Load**:
Read an *existing* diagnostic from a directory or bundle. Surfaces as CLI `read`
(a file path), Web UI *upload* (a user-supplied file), or an upload-service
*download* — and is how *product-provided* diagnostics enter (an Agent-generated
bundle, or a platform-generated ECE/ECK/K8P bundle), since ESDiag does not API-collect
those.
_Avoid_: collect, receive, read (a surface form, not the canonical stage)

**Save**:
Write freshly collected raw API responses to a directory or bundle. Only follows
Collect — you save what you newly collected.
_Avoid_: archive, dump, write

**Process**:
Transform diagnostic data into documents. Each processable unit is handled by a
per-API processor.
_Avoid_: transform (acceptable as a gloss; "process" is canonical)

**Export**:
Write *processed* documents to a destination (a remote cluster or a local
file/stream). Only follows Process.
_Avoid_: load, output, send (Send is a distinct stage — do not conflate)

**Send**:
Transmit an existing *bundle* to the Elastic Uploader service. Requires a bundle to
exist, from Save this run or from Load.
_Avoid_: upload (that is an inbound Load surface; Send is outbound), export

### Job phases

A job selects stages within three ordered phases. Only Phase 1 is mandatory.

- **Phase 1 — input (exactly one):** `Collect` (new) or `Load` (existing).
- **Phase 2 — middle (optional):** `Save` (new only), `Process` (new or existing),
  or `Save` then `Process` (new only).
- **Phase 3 — output (optional):** `Export` (processed) **and/or** `Send` (bundle) —
  both may run in one job (e.g. index to a cluster *and* forward the raw bundle to
  Support).

Dependency invariants: `Save` ⟸ `Collect`; `Export` ⟸ `Process`; `Send` ⟸ a bundle
exists (`Load` or `Save`); a job must do something (at least one of `Save`,
`Process`, `Send`). A plain `Collect` + `Save` needs no Phase 3.

**Execution mode is set by `Save`.** `Save` then `Process` is *staged*: collection
must finish and the bundle materialise before processing starts — the bundle is a
serialization barrier. `Process` directly after `Collect` (no `Save`) is
*streaming*: receive, transform, and export overlap concurrently.

### UI verbs (presentation only)

The Web UI and saved-job signals use the deliberately friendlier verbs **collect /
process / send**; these are UI labels, not backend stages, and do not map 1:1. UI
*upload* is a `Load` source (inbound); UI *send* resolves to `Export` or the `Send`
stage depending on whether `Process` ran. The `JobSignals*` types are strictly UI
implementation detail — the backend aligns on the six stages above.

### Deployment and access

**Runtime mode**:
How the web server is deployed — a two-value deployment archetype (`User` |
`Service`). `User` is single-user local (a laptop; one possible user, no remote
access, full capability: keystore, exporter changes, host management). `Service` is
shared and container-hosted for multiple users, deliberately locked down (no shared
keystore, no user-editable exporter — all processed diagnostics must go to the one
shared cluster). **Tenancy drives capability**, and the two are intentionally
bundled: multi-tenancy necessitates the lockdown.
_Avoid_: using "mode" for the auth mechanism (that is a separate axis)

**Authentication**:
An axis *orthogonal* to runtime mode and pluggable by provider (Google IAP today;
potentially another IAP, or Elastic Cloud SSO). It is not implied by runtime mode —
`Service` may run without IAP (e.g. local testing), and `User` may authenticate
(e.g. Cloud SSO). It serves two purposes: *access control* (gating a shared service)
and *identity provenance* (populating `Identifiers` with the user/account, and
authorizing outbound `Send` to the support portal).
_Avoid_: IAP (Google IAP is one provider, not the concept), login

**Credentials**:
Split by stage direction: *input* credentials authenticate to a source being
collected (`Collect`); *output* credentials authenticate to a destination written to
(`Send`/`Export`, and `View` for Kibana links). Custody follows a single rule — **the
app persists secrets only in `User` mode.** The `User`-mode keystore (`secrets.yml`,
encrypted) holds credentials for *saved known hosts* of any role (input `Collect`,
output `Send`/`View`); *ad-hoc* user-provided keys are runtime-only. `Service` mode
persists nothing at the app layer: output credentials are injected from a
vault/secrets service into env vars at container runtime, user identity is handled by
the IAP (see `Authentication`), and input keys are runtime-only. Hence the keystore
is a `User`-mode-only fixture.
_Avoid_: secret (acceptable internally; "credentials" is the concept)

**Keystore unlock**:
A time-limited grant of *use* of the keystore's credentials, without *disclosure* of
them. Unlocking (by password, via CLI or Web UI) writes an unlock file that lets
ESDiag decrypt and use saved-host credentials for a bounded window; a delegated actor
— automation, or an LLM agent — can then collect/process *through* ESDiag but can
never read the plaintext from the encrypted keystore. The keystore is therefore a
use-mediation boundary, not merely storage. Brute-forcing the unlock password is
rate-limited.
_Avoid_: session, login, decrypt (unlock grants use, not disclosure)

**Owner**:
The authenticated user who *executed* a job — the key by which the web UI scopes
state in `Service` mode. Attached to a job *execution* (and to a retained bundle),
not to a saved-job *definition*; authoring/saving a job is a separate, single-user
concern. UI events are scoped to their owner by default and visible only to that
user; only aggregate `stats` (processing state, diagnostics processed, document
count) are broadcast to everyone.
_Avoid_: creator, author (those describe saved-job authorship, not execution
ownership), user (too generic)

### Products and layers

A diagnostic is classified on two orthogonal axes: one **platform** (always) and
zero-or-one **application**.

**Platform**:
The deployment environment a diagnostic was collected from. Required and mutually
exclusive — every diagnostic has exactly one, and there is no "no platform" case
(a bare install is `SelfManaged`): `SelfManaged | ElasticCloudHosted | ECE | ECK |
KubernetesPlatform | Unknown`. ESDiag determines the platform best-effort at the
receiver from indicators (e.g. a `syscalls` folder implies `SelfManaged`, a
manifest `runner` of `ece` implies `ECE`); when provenance cannot be established —
notably for legacy `support-diagnostics` bundles — it is `Unknown`. Platform
diagnostics are *provided by the platform* (loaded, not API-collected). `Platform`
also replaces the legacy `orchestration` field/identifier throughout.
_Avoid_: orchestration (retired — use `platform`), orchestration layer, cloud layer, environment

**Application**:
An Elastic Stack component that produces application-level diagnostic data. A
closed set of exactly four: `Elasticsearch | Kibana | Logstash | Agent` — never a
platform value. Optional: a diagnostic carrying the platform's *own* data (e.g. the
`ECK`/`ECE` orchestration data, or an `ECE` bundle) has `application: None`. A
diagnostic's display label is its `application` if present, else its `platform`.
Cloud platforms will soon enrich the application-level bundle with
orchestration-defined metadata.
_Avoid_: product (see below), service

**Infrastructure data**:
Host/OS-level data whose kind is *determined by the platform*, not an independent
axis: `SelfManaged` yields Linux system calls (opt-in, and available under no other
platform), `ECK` yields container metrics, and managed cloud (`ElasticCloudHosted`,
`ECE`) yields none.
_Avoid_: system data, local data, host layer (it is not a product layer)

**Product** (legacy):
The current flattened enum that conflates platform and application into a single
axis (`Agent`, `ECE`, `ECK`, `ElasticCloudHosted`, `Elasticsearch`, `Kibana`,
`KubernetesPlatform`, `Logstash`). Being replaced by independent `Platform` and
`Application`; do not add new concepts to it.

### Receiving

**Receiver**:
The abstraction that resolves a data source for a `Collect` or `Load` stage. A
receiver is either remote (uses a client) or local (reads files, no client).

**Client**:
The transport used to receive from a *remote* source — the HTTP layer that talks
to a live cluster's REST API. Local receivers do not use a client.
_Avoid_: connection, transport

**Data source**:
A named unit of diagnostic data, and the unit of extensibility — adding one is what
it takes to collect or process something new. Deliberately transport-neutral: it is
acquired by a REST API call, a system command-line tool (e.g. self-managed
syscalls), or a file read, depending on the receiver. A data source has a **role**:
*collect-only* sources are saved into the bundle for human reading (e.g.
Elasticsearch `_cat` text APIs) but never processed; *processable* sources are also
transformed and additionally carry a `DataSource`/`DocumentExporter` impl.
_Avoid_: API (an API is one *kind* of data source, not the general term — so
`ElasticsearchApi` is a narrow name for what is really a data-source set)

**Weight**:
A data source's scheduling cost, on two orthogonal graded axes: *source weight* (load
on the system the source is pulled from — governs collect concurrency, protecting the
source) and *processing weight* (ESDiag CPU/time to transform it — governs processing
concurrency). Lives in the collection definition. How a weight maps to concurrency is
deployment-tunable policy.
_Avoid_: heavy/light (the legacy binary; weight is now two graded axes)

**Collection definition**:
The per-product registry of data sources (`assets/<product>/sources.yml`, embedded
but overridable at runtime via `--sources`) — what to collect and how. Each source
is **version-gated**: its request path resolves from semver ranges to a per-version
query (`get_url(version)`), so one definition serves many target versions. Ported
from `support-diagnostics`' definitions (`elastic-rest.yml` for API sources,
`diags.yml` for OS-command sources); the per-version compatibility knowledge there
is maintained every release, so ESDiag treats upstream as a reconciliation *input*.
Intended to be the single source of truth from which the collect list, the process
dispatch, and the diagnostic-type sets are all derived; that migration is currently
only half-complete (Support/Light derive from it, Minimal/Standard are hardcoded).
_Avoid_: sources file, API list

**Bundle**:
An archive or directory of collected diagnostic output — the artifact produced by
`Save` and consumed by `Load`. It is the boundary at which a job can decouple
across hosts, and the serialization barrier between `Save` and `Process`.
_Avoid_: diagnostic, dump, capture

**Data stream** (output):
The destination for processed documents, named `{class}-{subtype}[.sub]-esdiag`
(class ∈ `metrics` | `settings` | `logs` | `health`). Its schema is defined by
composable `esdiag@*` component/index templates — ECS-*inspired* but aligned to the
source API's shape for recognizability, not ECS-compliant. Each doc carries a
`diagnostic.*` / `cluster.*` provenance envelope (`diagnostic.application` +
`diagnostic.platform`, per ADR-0001). The name is a *contract* spanning processor
code, index templates, and hand-authored Kibana dashboards — verified across the
ESDiag-owned layers, not derived from a single source.
_Avoid_: index (these are data streams), output target

**Diagnostic outcome**:
The verdict of a diagnostic — `Complete | Partial | Failed | Skipped` — derived from
the events recorded in its report. One type for *any* diagnostic, parent or child
(`Skipped` covers unsupported cases, e.g. Kibana processing). `Partial` is the common
real case (some sources captured, some failed).
_Avoid_: status (reserved for HTTP/transport codes), success (a boolean loses Partial)

**Diagnostic report**:
The persisted record of one diagnostic: its `Diagnostic outcome`, aggregate counts,
and **all error/warning/success-level events** (each with source and reason) — not
just counts. Failures are collected here, never dropped to logs; it is the source of
truth the owner-scoped job feed renders and the CLI/WebUI status reads. Status is
two-level — a `_bulk` *request* code (e.g. `200`) is distinct from *document* codes
(e.g. `409`/`429` per doc); HTTP `0` means a non-HTTP exporter (file/stream/dir), not
"mixed".
_Avoid_: summary, log

**Manifest**:
The descriptor at the root of a bundle — its product/platform, versions, collection
date, and `included_diagnostics`. An interchange artifact ESDiag reads from both
`support-diagnostics` and its own bundles: read-compatibility is permanent and
changes are **additive-only** (ESDiag adds its own properties, never removes or
repurposes existing ones), so manifests are never migrated — only tolerated and
extended.
_Avoid_: metadata, header

**Included diagnostic**:
An application diagnostic nested inside a platform diagnostic (the manifest's
`included_diagnostics`). Inclusion is one level deep and layer-homogeneous: a
platform includes applications (`ECK`/`KubernetesPlatform` include Elasticsearch,
Kibana, Logstash), never another platform; applications include nothing. `ECE`
currently includes none. Platform does not propagate inherently: when ESDiag
processes included diagnostics it sets the `platform` on each child diagnostic job
as it spawns it.
_Avoid_: child diagnostic, sub-bundle
