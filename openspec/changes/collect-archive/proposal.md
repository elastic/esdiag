## Why

The legacy support diagnostic workflow produces a single archive, while the current `esdiag` flow writes a directory tree. This gap makes parity workflows harder for users who expect one artifact that is easy to move, upload, and retain.

## What Changes

- Add a `--zip` option to `esdiag collect` to write a single `.zip` diagnostic artifact instead of a directory tree.
- Define `collect --zip` as a boolean mode switch. Destination is controlled by the existing `output` positional argument (which defaults to `.`).
- Use the same base naming convention as the existing directory output and append `.zip` (for example, `diagnostic-<id>-<timestamp>.zip`).
- Write API outputs directly into the zip archive during collection, rather than writing to a directory first and bundling afterward.
- Add a `--zip` option to `esdiag process` with optional destination directory semantics (`Option<Path>`, default `.` when omitted) that stores all API call outputs in the standard diagnostic archive naming format.

## Capabilities

### New Capabilities
- `diagnostic-zip-output`: Optional zip-based output mode for `collect` and `process` that streams collected API data directly into a single archive file.

### Modified Capabilities
<!-- No requirement changes to existing specs; this proposal introduces a new capability. -->

## Impact

- **CLI**: Extends `collect` and `process` flags and output behavior.
- **Collection/Processing Pipeline**: Introduces archive-writing paths that run during API fetch/emit flow.
- **I/O Semantics**: Reduces intermediate filesystem materialization when zip mode is enabled.
- **Compatibility**: Preserves existing directory output defaults unless `--zip` is requested.
