## Context

There is a reported memory leak in the ESDiag application during the processing of diagnostics, specifically noticeable when uploading archives via the API (`/upload/submit` and `/upload/process`). The resident memory keeps increasing after each job finishes, eventually leading to OOM crashes.

The application uses Tokio for asynchronous execution and processes archives entirely in memory (`ArchiveBytesReceiver` holds a `ZipArchive` of `Bytes`). We suspect the leak is related to how the receiver, state machines, or background sub-processors manage the lifecycle of these `Bytes`.

## Goals / Non-Goals

**Goals:**
- Reproduce the memory leak using the provided `kibana-api-diagnostics-9.1.3.zip` diagnostic.
- Identify the root cause (e.g., circular references, detached tasks holding locks, or un-removed state in `ServerState`).
- Fix the leak to ensure resident memory stabilizes after a job completes.

**Non-Goals:**
- Refactoring the entire `Processor` architecture.
- Changing from in-memory processing to disk-based processing (unless absolutely necessary for the fix).

## Decisions

1. **Investigate `ServerState` Map Management:** 
   Verify if the `Bytes` from uploaded archives are properly removed from `state.uploads` under all success/failure conditions.
2. **Investigate Detached Tasks:** 
   `spawn_sub_processors` returns a `FuturesUnordered<JoinHandle<()>>` which is immediately dropped, leaving sub-processors detached. We need to ensure these tasks eventually terminate and drop their `Arc<Receiver>`.
3. **Investigate Tokio Channels:** 
   Check if any `mpsc::Sender` or `oneshot::Sender` is kept alive longer than expected, preventing receivers from closing and tasks from completing.

## Risks / Trade-offs

- [Risk] Memory profiling in Rust can be complex, especially with asynchronous tasks. → We will use tools like `heaptrack` and manual code inspection to isolate the leak.
- [Risk] If the fix involves awaiting sub-processors, it might increase the apparent latency of the main job. → We will carefully evaluate if sub-processors should be awaited or if they are functioning correctly as background tasks.
