## MODIFIED Requirements

### Requirement: Shell Completion Scripts Own Stdout
When the user runs `esdiag setup completions <bash|zsh|fish|powershell> --print`, stdout SHALL contain only the generated shell completion script. The CLI MUST NOT append a YAML or JSON terminal outcome to that stream. `esdiag setup completions <shell>` without `--print` remains a finite command and SHALL emit one typed `completion_installed` outcome.

#### Scenario: Print completions writes a raw script
- **WHEN** the user runs `esdiag setup completions zsh --print`
- **THEN** stdout contains a shell completion script for `esdiag`
- **AND** no YAML or JSON terminal outcome is appended

#### Scenario: Install completions emits a typed outcome
- **WHEN** the user runs `esdiag setup completions zsh`
- **THEN** stdout contains one YAML or JSON `completion_installed` result
- **AND** the result includes `shell`, `path`, and `instruction`
- **AND** the command does not edit shell startup files
