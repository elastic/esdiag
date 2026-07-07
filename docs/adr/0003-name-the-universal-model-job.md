---
type: Reference
title: "Name the universal execution model \"Job\""
status: accepted
tags: [repository, adr]
---

# Name the universal execution model "Job"

The single data structure that models one diagnostic execution (ADR-0002) is named
**`Job`**, not "Pipeline" or "Workflow". A run of a job mints a `JobID` that
identifies exactly one execution.

## Considered options

- **Workflow** — rejected: collides with the Elastic Stack's new "Workflows"
  feature. Using it inside an Elastic tool would be actively confusing.
- **Pipeline** — rejected for two reasons. (1) In Logstash ("pipelines") and
  Elasticsearch ("ingest pipelines") a *pipeline is a configuration of transforms*;
  an ESDiag job is *stages of a collection/processing execution*, not a transform
  config — the term would mislead. (2) "Job" reads as a concrete action with a
  concrete identity: a `JobID` refers to exactly one execution, whereas a
  "Pipeline ID" implies a reusable config, not a run.
- **Job (chosen)** — concrete, already the runtime's per-execution unit
  (`NEXT_JOB_ID` / `new_job_id()` / child "diagnostic jobs"), and already the name
  of the persisted `saved_jobs` concept. A *saved job* is a named, reusable job
  definition; running it is one execution.

## Consequences

- "pipeline" and "workflow" are avoided as concept-names in ESDiag code and docs
  (`CONTEXT.md` records the ruling). Generic prose use is fine; type/module/field
  names are not.
- The word "job" already carries the per-execution meaning at runtime, so this
  aligns the persisted model, the executor, and the UI on one existing term.
