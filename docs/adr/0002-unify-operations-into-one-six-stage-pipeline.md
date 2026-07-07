---
status: accepted
---

# Model every diagnostic job as one composable set of six stages

ESDiag's work is modelled four inconsistent ways today — CLI subcommands, the
`Collector`/`Processor` runtime split, the persisted `Job {collect, action}`
shape, and the Web UI `JobSignals` — and the shared verbs ("collect", "process",
"send") are overloaded across all of them. We are unifying the backend on a single
**job** composed from six stages selected within three phases, because every
supported job shape (including live collect-and-process, already supported) is a
point in that one space, and the two-type `Collector`/`Processor` split plus the
`collect + action` fusion are lossy re-encodings of it. ("Job", not "pipeline" or
"workflow" — see ADR-0003 for the naming rationale.)

## The six stages

- **Collect** — call live product APIs for a *new* diagnostic
- **Load** — read an *existing* diagnostic (directory/bundle; CLI `read`, UI upload, service download)
- **Save** — write raw collected APIs to a directory/bundle (`Save` ⟸ `Collect`)
- **Process** — transform diagnostic data into documents
- **Export** — write *processed* documents to a remote/local destination (`Export` ⟸ `Process`)
- **Send** — transmit an existing *bundle* to the Elastic Uploader (`Send` ⟸ a bundle exists)

## The three phases

- **Phase 1 — input (required, exactly one):** `Collect` xor `Load`
- **Phase 2 — middle (optional):** `Save`, `Process`, or `Save` then `Process`
- **Phase 3 — output (optional, at most one):** `Export` xor `Send`

## Considered options

- **Keep `Collector` and `Processor` as separate types.** Rejected: they share the
  same primitives, force per-type product dispatch, and cannot express a workflow
  that both saves a bundle and processes it without bespoke glue. The persisted
  `Job` and the UI `JobSignals` already model the union; the runtime lags.
- **One job of optional stages (chosen).** A single executor over a stage
  selection; `Collect`/`Process`/etc. become stage flags, not distinct types.

## Consequences

- **`Save` sets the execution mode.** `Save` then `Process` is *staged* — collection
  must complete and the bundle materialise before processing begins (the bundle is
  a serialization barrier). `Process` without `Save` is *streaming* — receive,
  transform, and export overlap concurrently (this is what the `get_stream` /
  `StreamingDataSource` / `document_channel` machinery exists for). Enabling the
  bundle is therefore a first-class behavioural switch, not merely an extra sink.
- **The two write-sink roles co-vary and can co-occur.** `Save` targets a bundle
  sink (raw), `Export` targets a document sink (processed); workflow
  Collect→Save→Process→Export uses both. Typing the sinks by role (see the
  `BundleExporter`/`DocumentExporter` direction) makes the invalid pairings —
  processed-docs-to-bundle, raw-to-cluster — unrepresentable.
- **UI verbs stay a presentation-layer translation.** `collect`/`process`/`send`
  and the `JobSignals*` types are UI-only and do not map 1:1 to stages (UI *upload*
  is inbound `Load`; UI *send* is `Export` or the `Send` stage). The backend aligns
  on the six stages; `CONTEXT.md` records the translation.
- **Naming collisions resolved by the stage set.** There is no "upload" stage
  (inbound is `Load`, outbound is `Send`) and no "forward" (streaming is `Process`
  without `Save`).
