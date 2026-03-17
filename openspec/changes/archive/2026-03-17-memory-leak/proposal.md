## Why

There is a memory leak in the diagnostic processing pipeline. When a diagnostic finishes processing, not all memory is freed, causing the resident memory usage to grow with each subsequent job. In production environments, this eventually leads to out-of-memory (OOM) events and container crashes when processing multiple diagnostics sequentially or concurrently via the API.

## What Changes

- Identify and fix the root cause of the memory leak during the diagnostic processing lifecycle.
- Ensure all resources allocated during a processing job are properly dropped and memory is reclaimed when the job completes.

## Capabilities

### New Capabilities
- `system-stability`: Memory management and stability

### Modified Capabilities

## Impact

- **Affected code:** Core processing logic, specifically the lifecycle and state management of diagnostic processing jobs within the type-state machines.
- **Affected products:** Processing of diagnostics for Elastic Stack products (e.g., Elasticsearch, Kibana).
- **APIs:** The `/upload/submit` and `/upload/process` endpoints will become stable over multiple invocations without crashing due to OOM.
- **Systems:** Improves stability of the Web UI/API server (`esdiag serve`) by preventing continuous memory growth.
