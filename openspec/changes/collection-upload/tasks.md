## 1. CLI Surface

- [x] 1.1 Extend the `collect` subcommand parser to accept an optional `--upload` argument for an Elastic Upload Service upload id or URL.
- [x] 1.2 Update `collect` help text and any user-facing CLI documentation to describe the new convenience workflow and that the archive is still written locally before upload.
- [x] 1.3 Add or update CLI parsing tests covering `collect` with and without the new `--upload` argument.

## 2. Collect-to-Upload Handoff

- [x] 2.1 Update the `collect` command execution path to capture the resolved archive path returned by the collector after a successful collection.
- [x] 2.2 Invoke the existing Elastic Upload Service uploader helper from `collect` when `--upload` is provided, passing the resolved archive path and upload id.
- [x] 2.3 Ensure upload only runs after successful collection and that upload failures return an error while preserving the collected archive for retry.

## 3. Verification

- [x] 3.1 Add or update tests for successful collect-plus-upload behavior, including use of the resolved runtime archive path.
- [x] 3.2 Add or update tests confirming no upload is attempted when collection fails and that upload failures surface correctly after collection succeeds.
- [x] 3.3 Run `cargo clippy` and resolve any new warnings in changed code.
- [x] 3.4 Run `cargo test` and confirm relevant suites pass.
