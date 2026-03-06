## 1. Local Node Matching from `_nodes`

- [x] 1.1 Add a collector utility that gathers local identity signals (hostname, interface names, interface IPs, interface hostnames, canonical hostnames)
- [x] 1.2 Implement node matching against `_nodes` entries using `host`/`ip` comparisons
- [x] 1.3 Implement first-match order tie-break behavior for multi-node matches and warning behavior for no-match cases
- [x] 1.4 Wire matched node context into collect runtime state for syscall variable resolution

## 2. Syscall Command Inventory Integration

- [x] 2.1 Implement typed loading/parsing of `assets/elasticsearch/syscalls.yml` command definitions
- [x] 2.2 Implement OS selection logic that resolves the correct command set for the active host OS
- [x] 2.3 Implement `{{VARIABLE}}` placeholder rendering while preserving existing template syntax and current YAML shape
- [x] 2.4 Extract `LOGPATH` from matched `nodes.<id>.settings.path.logs` and `PID` from matched `nodes.<id>.process.id`
- [x] 2.5 Resolve `JAVA_HOME` by inspecting `ps -ef` (or OS equivalent) for the resolved PID and using the first executable path in the process description
- [x] 2.6 Add validation and warning behavior for missing/invalid command entries or unresolved placeholders

## 3. Receiver and PathType Integration

- [x] 3.1 Add syscall receiver path for pure raw-text collection flow (no parsing) using raw retrieval/save interfaces
- [x] 3.2 Introduce a dedicated syscall path discriminator (for example `PathType::SystemCall`) in data-source routing
- [x] 3.3 Ensure receiver dispatch differentiates syscall commands from file and URL source handling

## 4. Collect Workflow Integration

- [x] 4.1 Add conditional syscall execution phase to `collect` that runs when `_nodes`-based local node matching succeeds
- [x] 4.2 Integrate syscall output persistence into the existing archive bundle path (no separate artifact)
- [x] 4.3 Ensure syscall command errors/timeouts are warning-only and do not abort API collection or archive finalization

## 5. Tests and Verification

- [x] 5.1 Add unit tests for local identity gathering, `_nodes` matching, first-match order behavior, and no-match handling
- [x] 5.2 Add unit tests for node-derived `PID`/`LOGPATH` extraction and PID-based `JAVA_HOME` inference
- [x] 5.3 Add unit tests for receiver raw-path syscall handling and syscall path-type dispatch
- [ ] 5.4 Add integration tests confirming syscall execution happens only when local node matching succeeds and outputs are bundled with API data
- [x] 5.5 Run `cargo clippy --all-targets --all-features` and resolve issues
- [x] 5.6 Run `cargo test` and ensure all new syscall-related tests pass
