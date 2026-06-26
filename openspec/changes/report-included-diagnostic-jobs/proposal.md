## Why

ECK and KubernetesPlatform bundles can contain multiple `included_diagnostics`, and esdiag already processes each supported child diagnostic. The web job UI and CLI summary currently report only the parent bundle's diagnostic id and Kibana link, which is misleading because the parent orchestration bundle may have no processed documents while the useful Elasticsearch child diagnostics are hidden from the user.

## What Changes

- Report each processed child diagnostic from a parent ECK/KubernetesPlatform bundle as its own web job result.
- Show a separate progress/result box for every child diagnostic started from `included_diagnostics`.
- Link each completed child web result to its own `diagnostic.id` Kibana URL instead of only showing the parent bundle id.
- Include each completed child diagnostic id and Kibana link in CLI process output.
- Return synchronous API processing results as a JSON array with one entry for the parent diagnostic and one entry for each included diagnostic outcome.
- Preserve and report parent bundle context even when it is empty today.
- Represent recognized but unsupported included diagnostics as `info` results so users can see that they were skipped intentionally.

## Capabilities

### New Capabilities
- `included-diagnostic-jobs`: Web and CLI reporting for parent bundles that fan out into supported and unsupported included diagnostics.

### Modified Capabilities
- `service-link-wait`: Synchronous processing responses return multiple diagnostic result entries instead of a single result object.

## Impact

- Target products: Elastic Cloud Kubernetes and generic KubernetesPlatform diagnostic bundles, with child Elasticsearch diagnostics as the primary processed output.
- Web UI: job feed/status rendering, child progress boxes, completion boxes, skipped/info boxes, and Kibana links.
- CLI: `process` command summary output for child diagnostic ids, Kibana links, and skipped child diagnostics.
- API: synchronous `/api/api_key` and `/api/service_link` response bodies for parent bundles with included diagnostics.
- Core processing/job orchestration: expose per-child processing outcomes from parent bundle processing so the web runner, CLI formatter, and synchronous API handlers can emit status for each diagnostic.
- Tests: processor/job-runner/CLI-summary/API coverage for multiple included diagnostics, unsupported skipped diagnostics, and UI fragments for child result rendering.
