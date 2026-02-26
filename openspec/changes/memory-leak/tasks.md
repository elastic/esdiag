## 1. Investigation & Root Cause Analysis

- [ ] 1.1 Run `esdiag serve` with the `heaptrack` tool.
- [ ] 1.2 Submit multiple diagnostic jobs sequentially using the provided `kibana-api-diagnostics-9.1.3.zip` via `/upload/submit` and `/upload/process`.
- [ ] 1.3 Analyze the `heaptrack` output to identify the exact location of the memory leak (e.g., `ServerState` maps, `tokio::spawn` detached tasks holding `Arc<Receiver>`, channel buffers).
- [ ] 1.4 Evaluate `serde_json` memory impact: check if `heaptrack` shows massive `BTreeMap` or `String` allocations that are never freed (logical leak) vs freed but with growing OS RSS (memory fragmentation).

## 2. Implementation

- [ ] 2.1 Implement the fix based on the findings from 1.3 and 1.4 (e.g., ensuring `Arc` references are dropped, state is properly cleaned up from `ServerState::uploads` or `ServerState::links` when jobs complete or fail).
- [ ] 2.2 Address any detached `tokio::spawn` tasks if they are identified as the leak source by properly joining or dropping them.
- [ ] 2.3 Swap the global allocator to `mimalloc` (or `jemalloc`) if memory fragmentation from `serde_json` and `BTreeMap` is identified as the root cause.

## 3. Verification

- [ ] 3.1 Re-run `esdiag serve` with `heaptrack` and submit multiple sequential and concurrent diagnostic jobs using the API.
- [ ] 3.2 Verify that the resident memory usage stabilizes after job completion and the `heaptrack` profile shows no ongoing leak.
- [x] 3.3 Run `cargo clippy` and `cargo fmt` to ensure code quality standards.
- [x] 3.4 Run `cargo test` to ensure no existing tests are broken.