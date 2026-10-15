# First implementation verification

## Completeness

All nine implementation tasks are exercised. Native layer 3, PR438, has head `elastic/esdiag:stack/pr330-scrub-traversal` and base `stack/pr330-typed-loading` (PR437), not main. It includes the typed loader layer; the standalone evidence below is retained from before migration. Joint review, combined verification and required archival remain before native stack merge. Specs are intentionally skipped (`skip_specs: true`); the existing malformed-IP and streaming contracts remain authoritative.

## Correctness

- Before cutover: raw, typed/streaming and export characterization suites (20 tests) plus 22 scrub module tests passed.
- After cutover and the recursion correction: 21 integration tests across raw, typed/streaming and export suites, plus 46 receiver library tests passed. Workspace clippy returned zero errors and four unchanged warnings outside this change.
- Found and fixed an exact raw nesting regression: the initial replacement accepted 128 containers where `serde_json::Value` rejects them. The expanded receiver test failed before the fix; afterward both the permanent test and an independent executable accepted depths 126/127 and rejected 128/129.
- Raw file/bytes/directory coverage retains complete validation, duplicate decoded-key last-wins values, surviving transformation counts, numeric types/negative zero/overflow, response byte size, and exact disabled/unsupported content.
- Eager and streaming consumers retain original number/key/string lexemes, duplicate interpretation, downstream error policies and temporary-file lifetime; injected reader failure propagates without consuming partial output.
- Archive and directory exports cover valid IPv4, unbounded decimal octets, pure-IP versus optional-port fields, unrelated names and scrub-disabled addresses.
- Actual release CLI, isolated home, scrub off/on: 487 documents and zero failures in each run; exported metric/settings node ID, `hot-dee0`, original/normalized address and transport port verified. Large fixtures produced 10,128 and 79,792 documents respectively, with zero failures.

## Resource evidence

Synthetic `nodes_stats.json` payloads: 5,060,754 and 40,485,616 bytes; both nesting depth 10 and maximum token 207 bytes. Release binaries measured with macOS `/usr/bin/time -l`; one-run observations, not a statistical benchmark. The isolated executable includes the real normalization module; its PR330 comparison changes only the included module path.

| Adapter | Input bytes | PR330 peak RSS bytes | Refactor peak RSS bytes |
|---|---:|---:|---:|
| temp | 5060754 | 2424832 | 2359296 |
| raw | 5060754 | 59998208 | 46776320 |
| temp | 40485616 | 2473984 | 2457600 |
| raw | 40485616 | 449347584 | 341082112 |

Temporary-file normalizer RSS stays approximately constant across 8× payload growth. Raw RSS scales with required String output and object/key/fragment bookkeeping; it is not bounded-memory. Both adapters reported exactly 96 and 768 transformed fields, respectively, matching PR330.

| CLI input bytes | Scrub off RSS bytes | Scrub on RSS bytes | Increase |
|---|---:|---:|---:|
| 5060754 | 52674560 | 53477376 | 1.52% |
| 40485616 | 108920832 | 120274944 | 10.42% |

Both actual CLI runs satisfy the documented ≤20% scrub-on target. End-to-end RSS includes processors and exported documents, separately from the isolated normalizer measurement. For the larger scrub-on fixture, PR330/refactor real time was 0.80/0.83 seconds and user CPU 1.82/1.82 seconds; this single observation shows no material CPU regression, not a speedup claim.

## Coherence

One private field-scope walk owns decoded child-key and inherited-array scope. Two actual output adapters retain their separate interpretation policies; `normalize_value` is deleted. No new public interface, dependency, whole-payload typed allocation, retries or swallowed error. Nearby documentation now describes resource ownership and separates normalizer memory from Process memory; the memory commands use positional output correctly. CHANGELOG is intentionally unchanged for preserved behavior. Native strict OpenSpec validation passed; active-change merge-completion remains pending review/archival.

## Inherited parent depth-budget correction

The current parent explicitly limits enabled, supported normalization to 128 containers. Proposal, design and task language now treat that approved parent policy as the compatibility baseline rather than implying unlimited typed depth. Direct-reader bypasses remain unchanged; raw `Value` acceptance remains stricter and still rejects 128 containers.

After propagation, all 49 receiver library tests and all six typed/streamed compatibility tests passed, as did formatting and strict validation. The consumer-depth case now accepts 127 containers and rejects 128 through file, memory and directory receivers, proving that a stricter `Value` consumer still governs inside the normalization budget. The inherited reader regressions separately prove `RawValue` acceptance at 128, early rejection at 129 for arrays and objects, and retained 512-level direct-reader acceptance. Historical RSS measurements above were not rerun and do not claim to measure this nesting-policy correction.
