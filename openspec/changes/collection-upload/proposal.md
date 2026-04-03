## Why

Collecting a diagnostic archive and then uploading it to Elastic Upload Service is already possible, but it currently requires the user to manage the output filename and run two commands manually. That is awkward when `esdiag collect` generates a filename at runtime, so adding an optional upload handoff makes a common support workflow easier and less error-prone.

## What Changes

- Add an optional `--upload` argument to `esdiag collect` that accepts an Elastic Upload Service `upload_id`.
- Make `esdiag collect` upload the archive it just produced when `--upload` is provided, whether the archive path was supplied explicitly or generated automatically at runtime.
- Reuse the existing raw-bundle uploader behavior so the collected archive is uploaded unchanged after a successful collection.
- Preserve current `collect` behavior when `--upload` is omitted.

## Capabilities

### New Capabilities
- None.

### Modified Capabilities
- `collection-execution`: The CLI collect workflow can optionally hand off its completed output archive to a follow-up upload step without requiring the user to know the generated filename in advance.
- `elastic-uploader`: The existing raw-bundle upload capability is reused from `esdiag collect` so a successful collection can immediately upload its output using a provided `upload_id`.

## Impact

- CLI argument parsing and help text for `esdiag collect`.
- Collection command execution flow, including access to the resolved output archive path after collection completes.
- Reuse of the existing Elastic Upload Service uploader path from the collect command.
- CLI tests and user-facing documentation for collect/upload workflows.
