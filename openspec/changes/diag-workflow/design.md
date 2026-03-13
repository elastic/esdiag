## Context

The current home page is organized around a single "Process Diagnostics" panel with tabbed intake sources. That shape assumes every workflow immediately processes a diagnostic after intake, even though the product now needs to support three different execution paths: collect and process, collect and send without processing, and process an already-available archive before sending it onward.

This change cuts across the web template layer, Datastar signal model, server request handlers, and workflow orchestration. It also has mode-aware behavior: `user` mode can use locally persisted known hosts, while `service` mode must avoid local host persistence and require explicit remote credentials. The design must preserve existing collection sources while making the execution path explicit and safe, with each stage choosing between two concrete modes instead of a single generic form.

## Goals / Non-Goals

**Goals:**
- Introduce a clear three-stage workflow model with `Collect`, `Process`, and `Send` panels on the home page.
- Separate source selection from processing options and delivery options so each stage can be chosen, validated, and executed independently.
- Model each stage with two explicit modes:
  - `Collect`: `Collect` or `Upload`
  - `Process`: `Process` or `Forward`
  - `Send`: `Remote` or `Local`
- Preserve existing intake sources while fitting them into the new stage model: remote inputs under `Collect`, local archive upload under `Upload`.
- Allow remote collection to optionally save a bundle locally before later processing, forwarding, or local delivery by reusing the existing `collect --save` behavior.
- Support processing subsets via diagnostic product/type selection and advanced API option overrides limited to fully implemented options.
- Support send targets appropriate to the workflow outcome:
  - `Remote`: processed output to diagnostic cluster targets, forwarded archives to Elastic Upload Service endpoints
  - `Local`: processed output to localhost diagnostic clusters or local directories, forwarded archives handled through the `Collect` save path
- Reuse existing exporter concepts behind the UI so `Remote` and `Local` are presentation choices over already-supported output types wherever possible.

**Non-Goals:**
- Changing CLI command semantics for `collect`, `process`, or `send`.
- Defining new diagnostic products or new source API implementations beyond what already exists.
- Expanding the advanced processing list to partially implemented or placeholder APIs.
- Reworking the settings modal beyond what is necessary to reuse persisted send targets.

## Decisions

1. **Adopt an explicit two-option mode per stage**
   - Decision: Replace the current top-level `tab`-centric interaction with a workflow state that tracks a selected mode for each stage: `Collect`/`Upload`, `Process`/`Forward`, and `Remote`/`Local`, plus the configuration associated with each active mode.
   - Rationale: The requested workflow is not just a visual split; it is a decision tree where each stage chooses one of two behaviors that materially changes validation and execution.
   - Alternatives considered:
     - Keep the current tabs and add more conditional sections: rejected because validation and send/skip-processing flows become harder to reason about.
     - Split each path into a separate route/page: rejected because the requested experience is a single staged home page workflow.

2. **Normalize both remote collection and local upload into one collected-artifact contract**
   - Decision: `Collect -> Collect` and `Collect -> Upload` SHALL both resolve into a shared workflow artifact contract describing the archive kind, provenance, whether the bundle is already local, and whether a persisted saved copy exists. When `Collect` save is enabled, this SHALL reuse the same archive persistence behavior as the CLI `collect --save` path.
   - Rationale: Downstream `Process` and `Send` stages should consume one normalized contract instead of branching on raw form origin, and archive persistence should not invent a second save mechanism separate from the existing CLI behavior.
   - Alternatives considered:
     - Let process/send handlers inspect original form payloads directly: rejected because it couples downstream stages to UI source details.

3. **Model processing as `Process` or `Forward`**
   - Decision: The `Process` panel SHALL explicitly choose between `Process` and `Forward`. `Process` builds processed diagnostic output using product/type and advanced processor selection. `Forward` preserves the raw archive unchanged from the collected/uploaded artifact.
   - Rationale: Forwarding raw data is a first-class workflow, not just "processing disabled."
   - Alternatives considered:
     - Always process and add a "minimal output" option: rejected because the user explicitly needs to forward unprocessed archives in some flows.
     - Model forwarding as a hidden side effect of disabling processing: rejected because the user asked for an explicit stage option.

4. **Use one job or two jobs depending on save behavior**
   - Decision: `Collect -> Collect -> Process -> Send` without `Save` SHALL continue using the current on-demand API retrieval flow as a single job. When `Save` is enabled, collection SHALL become its own job that persists an archive, and `Process + Send` SHALL run as a second job consuming the saved artifact.
   - Rationale: This preserves today's efficient in-memory/on-demand path while allowing saved archives to become explicit handoff points for later workflow stages.
   - Alternatives considered:
     - Always split into two jobs: rejected because it would add unnecessary persistence and orchestration overhead to the current on-demand flow.
     - Never split into two jobs: rejected because saved archives need a durable boundary between collection and later processing/sending.

5. **Define "fully implemented" from product processor implementations**
   - Decision: The advanced checkbox list SHALL treat a processing option as fully implemented when that option has a concrete processor implementation in the corresponding product subtree under `src/processor`. If the runtime code cannot infer this cleanly from the module layout, the implementation SHALL introduce a per-product authoritative enum or registry that represents the same set explicitly and can also carry dependency metadata needed by processing-plan resolution.
   - Rationale: The UI needs a stable, code-backed definition of which processing options are genuinely supported, and the `src/processor` tree is the current source of truth for that support. If we need a code-level fallback, that fallback should also encode the dependency relationships needed for required-option locking so implemented membership and dependency rules do not drift apart.
   - Alternatives considered:
     - Show every source from `sources.yml`: rejected because it exposes unsupported behavior.
     - Hardcode per-panel checkbox lists in the template without a code-backed source of truth: rejected because it drifts from actual processor support.
     - Require runtime filesystem inspection only: rejected as brittle for compiled binaries and harder to test than a code-level registry fallback.
     - Maintain one authoritative list for implemented processors and a separate dependency map: rejected because the two sources can diverge and produce invalid locked-option behavior.

6. **Treat required processors as non-optional advanced selections**
   - Decision: The advanced processing list SHALL distinguish between selectable optional processors and required processors that are locked on because they are minimum requirements, direct dependencies, or necessary to build metadata or manifest outputs. When a per-product authoritative enum or registry is used, it SHOULD be capable of expressing these dependency relationships directly.
   - Rationale: Allowing users to deselect required processors would create invalid or partially coherent processed diagnostics even when the UI appears to support the selection.
   - Alternatives considered:
     - Allow any checkbox to be deselected and fail only during execution: rejected because it creates confusing late validation and weakens trust in the processing controls.
     - Hide required processors entirely: rejected because users still need visibility into why certain processors are always included.

7. **Keep runtime-mode boundaries at the input/validation layer**
   - Decision: `user` mode SHALL allow known-host selection in `Collect -> Collect` and local bundle save targets, while `service` mode SHALL use explicit endpoint/API key inputs and SHALL not depend on persisted local host artifacts.
   - Rationale: This preserves the existing runtime-mode contract while still allowing the same high-level workflow in both modes.
   - Alternatives considered:
     - Allow service mode to read local known hosts for convenience: rejected because it violates the existing shared-instance contract.

8. **Default bundle persistence to an OS-aware Downloads directory**
   - Decision: When `Save Bundle` is enabled, the workflow SHALL use a configurable local directory target with an operating-system-aware default of the current user's `Downloads` directory.
   - Rationale: Users need a predictable default save location that matches normal desktop expectations while still being able to redirect bundle storage when needed.
   - Alternatives considered:
     - Restrict saves to a fixed workspace/application directory: rejected because it is less discoverable and less user-friendly for downloaded archives.
     - Require manual directory entry every time: rejected because it adds friction to a common workflow.

9. **Model send as `Remote` or `Local` with output-aware subtargets**
   - Decision: The `Send` panel SHALL explicitly choose between `Remote` and `Local`. For `Process -> Process`, `Remote` targets a diagnostic cluster and `Local` targets either a localhost diagnostic cluster or a local directory. For `Process -> Forward`, `Remote` targets a new Elastic Upload Service exporter endpoint and `Local` is disabled because the raw archive's local persistence is handled by `Collect` save behavior. The current footer output selector SHALL move into this panel as part of the send-target UI.
   - Rationale: The user-visible distinction is not only artifact kind; it is also whether delivery leaves the machine or remains local, and the output target belongs to the send stage rather than as a page-global footer control.
   - Alternatives considered:
     - Funnel both through one generic "target" input: rejected because it obscures incompatible validation and transport rules.
     - Allow `Local` forwarding as a second save destination separate from `Collect`: rejected because it duplicates the bundle-save concept and creates conflicting local archive ownership.

10. **Disable invalid send targets from current workflow state**
   - Decision: The `Send` panel SHALL derive target availability from the active `Collect` and `Process` configuration and disable invalid targets before execution. Incompatible targets may remain visible for clarity, but they must not be selectable while the current workflow state makes them invalid.
   - Rationale: Immediate affordance feedback is clearer than allowing a stale or incompatible send target to remain active until submit-time validation.
   - Alternatives considered:
     - Allow any target selection and fail only on submit: rejected because it creates avoidable user confusion.
     - Hide incompatible targets entirely: rejected because disabled targets better communicate why a delivery path is unavailable.

11. **Auto-enable local save when forward + local is selected**
   - Decision: If the user selects `Send -> Local` while `Process -> Forward` is active, the UI SHALL disable local send execution, explain that the local bundle is managed in `Collect`, and automatically enable `Collect` save if it is currently off.
   - Rationale: Forwarded archives do not need a second local delivery mechanism; they just need the collected archive persisted locally.
   - Alternatives considered:
     - Keep `Send -> Local` disabled without changing `Collect` save: rejected because the user asked for local forwarded bundle handling and auto-enabling save reduces friction.
     - Create a second independent local-save destination in `Send`: rejected because it duplicates archive persistence behavior.

12. **Introduce a dedicated Elastic Upload Service upload capability**
   - Decision: Unprocessed bundle delivery to Elastic Upload Service SHALL be implemented as a new capability with a dedicated CLI entrypoint `esdiag upload <file_name> <upload_id>`. The workflow `Send -> Remote` path for `Process -> Forward` SHALL rely on this uploader capability instead of pretending the existing receiver-side service-link integration already covers exporting.
   - Rationale: Downloading from Elastic Upload Service already exists, but uploading raw bundles does not. This needs an explicit exporter/CLI contract rather than being implicit in the current web processing handlers.
   - Alternatives considered:
     - Reuse the existing service-link receiver path for upload: rejected because it only downloads/receives bundles, not exports them.
     - Hide uploader behavior only inside the web server: rejected because the user explicitly wants a CLI surface and reusable implementation.

## Risks / Trade-offs

- **[Risk] The three-panel workflow increases UI state complexity** -> **Mitigation:** define a single normalized workflow signal model and keep cross-panel derived state server-validated.
- **[Risk] Folder-based implementation detection can be awkward to represent at runtime** -> **Mitigation:** use the product processor module layout as the conceptual source of truth, but introduce a per-product enum/registry when direct inference is not clean in compiled code.
- **[Risk] Required processor rules can drift from actual processor dependencies** -> **Mitigation:** derive locked selections from the same per-product authoritative enum/registry or equivalent planning registry used to define implemented processors, including product-specific rules like Elasticsearch `version` and `cluster_settings_defaults`.
- **[Risk] Save-enabled workflows can diverge from CLI collect behavior** -> **Mitigation:** reuse the same `collect --save` archive persistence logic instead of building a separate web-only save path.
- **[Risk] Optional local bundle saving can conflict with service mode artifact restrictions** -> **Mitigation:** gate local save path inputs behind runtime-mode policy and fail validation early when local artifact writes are disallowed.
- **[Risk] OS-specific Downloads resolution can vary by environment** -> **Mitigation:** centralize path resolution behind one cross-platform helper and allow the user to override the default directory before execution.
- **[Risk] Send behavior can become ambiguous when the user changes collect/process options after configuring targets** -> **Mitigation:** recompute send-target affordances from current workflow state, disable incompatible targets immediately, and normalize forward-plus-local behavior back into the `Collect` save path.
- **[Risk] Elastic Upload Service export is a new integration surface** -> **Mitigation:** define it as a separate capability with a CLI contract, reference implementation, and dedicated tests rather than coupling it loosely into existing receiver code.

## Migration Plan

1. Introduce workflow state and template structure for the three-panel home page while preserving existing backend handlers behind adapted orchestration.
2. Add collect/upload normalization so remote collection, uploader-service intake, and local upload all produce a shared workflow result, reusing `collect --save` persistence when requested.
3. Preserve the current one-job on-demand path for unsaved collect-plus-process-plus-send and add the saved two-job handoff path.
4. Add explicit process/forward controls, implemented-option filtering sourced from product processor implementations or an equivalent enum/registry with dependency metadata, and required-processor locking.
5. Add remote/local send controls by moving output-target selection from the footer into the send panel, including localhost/local-directory processed delivery and forward-plus-local normalization into `Collect` save behavior.
6. Implement the new Elastic Upload Service exporter/CLI capability for forwarded raw bundles.
7. Update user/service mode validation to match the new panel inputs, local bundle save rules, and OS-aware default save directory behavior.
8. Add UI and integration coverage for collect/upload, process/forward, remote/local send flows, and uploader behavior.

Rollback strategy:
- Restore the prior single-panel template and route wiring while leaving lower-level collection/processing helpers intact.
- Keep new workflow normalization internal so it can be removed without changing CLI behaviors.

## Open Questions
