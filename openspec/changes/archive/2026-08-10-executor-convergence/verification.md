# Executor convergence verification matrix

This matrix records the evidence required by tasks 6.1, 6.5, and 6.6. Unit
and integration tests use isolated temporary homes, mock output services, and
checked-in diagnostic archives. Live cases use a disposable `esdiag-local`
stack.

## CLI parity

| Behavior | Evidence |
| --- | --- |
| Collects a known host and writes an archive | `tests/collection_tests.rs::test_collect_kibana_mock_workflow`; live `collect remote-stack` |
| `collect --upload` maps to staged Save + Send | `src/main.rs::collect_command_parses_upload_id`; `src/job/executor.rs::staged_collect_serializes_before_raw_send_starts` |
| Archive and directory inputs process through the executor | `tests/executor_cli_upload_parity_tests.rs::process_archive_uses_executor_and_writes_selected_local_output`; `process_directory_uses_executor_and_writes_selected_local_output` |
| Known-host and service-link inputs resolve through the executor | `src/job/executor.rs::default_context_executes_saved_job_with_stable_input_and_save_target`; `tests/service_link_wait_tests.rs::service_link_wait_for_completion_processes_synchronously`; live `process remote-stack remote-stack` |
| `--sources` validates the active product | `tests/sources_override_tests.rs` |
| `--save-job` persists phase jobs before execution | `tests/executor_cli_upload_parity_tests.rs::collect_and_process_save_job_persist_phase_jobs_before_execution` |
| Omitted output uses `ESDIAG_OUTPUT_*`; explicit output wins | `tests/executor_cli_upload_parity_tests.rs::process_uses_environment_output_when_output_is_omitted`; `explicit_process_output_overrides_environment_output` |
| Standalone upload preserves custom API URL | `tests/executor_cli_upload_parity_tests.rs::standalone_upload_uses_executor_sender_with_custom_api_url` |
| Summaries include counts, child outcomes, and links | `src/main.rs::collect_summary_uses_collected_counts_and_path`; `process_summary_includes_kibana_link_for_stderr_output`; `process_summary_includes_child_outcomes_for_parent_bundles` |
| Failure preserves a non-zero exit and stage summary | `tests/executor_cli_upload_parity_tests.rs::process_failure_returns_nonzero_with_stage_summary` |

## Web and API parity

| Behavior | Evidence |
| --- | --- |
| Owner-scoped events are visible only to their owner | `src/server/mod.rs::broadcast_stream_delivers_owned_and_broadcast_events_per_subscriber`; `targeted_events_only_reach_matching_user`; `src/server/job_runner.rs::child_diagnostic_events_inherit_parent_owner` |
| Service admission caps and release accounting work | `src/server/mod.rs::service_job_caps_limit_global_and_per_owner_concurrency`; `service_rejected_jobs_do_not_release_active_capacity` |
| Statistics distinguish successful, failed, and rejected jobs | `src/server/mod.rs::record_outcome_counts_failed_outcome_as_failed_job`; `src/server/stats.rs` tests |
| Retained downloads preserve ownership and loaded-bundle policy | `src/server/job_runner.rs::loaded_bundle_process_retention_publishes_owner_scoped_download_without_save`; bundle-download tests |
| Service mode preserves its output and authentication policy | `src/server/job_runner.rs::service_mode_allows_bundle_save_downloads`; `src/server/mod.rs::service_mode_can_use_no_auth_provider_for_local_testing`; `tests/runtime_mode_web_tests.rs` |
| Synchronous APIs project parent and child outcome arrays | `src/server/api.rs::synchronous_api_results_include_parent_and_child_outcomes`; live `/api/api_key?wait_for_completion=true` |
| Process Export and raw Send both run independently | `src/server/job_runner.rs::web_draft_preserves_raw_send_alongside_processed_output`; `forward_web_draft_uses_legacy_remote_target_as_raw_send`; `src/job/executor.rs::export_and_send_outcomes_are_independent_for_every_result_pair`; `raw_send_can_succeed_after_process_failure`; `src/data/saved_jobs.rs::job_draft_round_trips_processed_and_raw_remote_targets_independently` |

## Delta-spec regression coverage

The focused executor tests cover runtime-binding persistence/rejection,
materialized service links, structured events and outcomes, input failure
blocking, all Export/Send success-failure pairs, child identity/platform/export
inheritance, one-level fan-out, saved-job draft rejection, and authoritative
diagnostic verdicts. The relevant test modules are:

- `src/job/executor.rs`
- `src/job/model.rs`
- `src/receiver/resolver.rs`
- `src/data/saved_jobs.rs`
- `src/server/saved_jobs.rs`
- `src/server/job_runner.rs`
- `src/server/api.rs`
- `src/server/template.rs`

Run the local gate with:

```sh
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```
