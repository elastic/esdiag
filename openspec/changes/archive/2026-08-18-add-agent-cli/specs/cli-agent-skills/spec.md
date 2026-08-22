## ADDED Requirements

### Requirement: Portable Skill Is Embedded In The Binary
An ESDiag binary built with the Cargo `agent` feature SHALL embed the canonical script-free `.agents/skills/esdiag/` skill at compile time using `rust_embed`. The embedded assets SHALL include `SKILL.md`, `references/`, and supported `agents/` metadata, MUST exclude executable helper scripts, and SHALL identify the running ESDiag package version and a deterministic content digest.

#### Scenario: Cargo-installed binary contains the skill
- **WHEN** a user installs ESDiag with `cargo install esdiag`
- **THEN** the resulting binary contains the ESDiag skill assets compiled from the same package version
- **AND** installation requires no source checkout or later network download

#### Scenario: Homebrew binary contains the skill
- **WHEN** a packaged ESDiag binary is installed through Homebrew
- **THEN** `esdiag agent skills` can read the embedded skill while offline
- **AND** the embedded content matches the binary's ESDiag version

### Requirement: Agent Skills Command Detects Supported Hosts
The CLI SHALL provide `esdiag agent skills [--target <claude|codex|opencode>]... [--force]` to detect supported user-scoped Claude Code, Codex, and OpenCode skill targets and install the embedded ESDiag skill into every detected target. It SHALL support explicit repeatable target selection when automatic detection is insufficient and MUST NOT modify a project-local skill directory implicitly.

#### Scenario: Multiple installed agents are detected
- **GIVEN** supported user homes exist for Codex and OpenCode
- **WHEN** the user runs `esdiag agent skills`
- **THEN** both targets are selected for installation
- **AND** an undetected Claude Code target is reported without being created implicitly

#### Scenario: Explicit target has no existing home
- **GIVEN** Claude Code is not automatically detectable
- **WHEN** the user explicitly selects the Claude Code target
- **THEN** the command creates the documented user-scoped skill parent as needed
- **AND** installs no project-local files

#### Scenario: No agent is detected
- **WHEN** default detection finds no supported target
- **THEN** the command makes no filesystem changes
- **AND** returns a structured outcome listing supported explicit targets

### Requirement: Installation Is Offline And Version-Correct
Skill installation SHALL copy only from the running binary's embedded assets and MUST NOT contact GitHub, a plugin marketplace, a package registry, or another network service. The installed result SHALL report the running ESDiag version and embedded content digest.

#### Scenario: Installation runs without network
- **GIVEN** the machine has no network access
- **WHEN** a supported agent target is selected
- **THEN** installation completes from embedded bytes
- **AND** the installed skill corresponds to the running binary rather than a latest remote release

### Requirement: Existing Installations Are Handled Safely
The installer SHALL leave an exact matching skill unchanged, update an intact installer-owned older skill atomically, and refuse to overwrite an unrecognized or locally modified skill directory without explicit force. Installer ownership metadata MUST contain only installer identity, ESDiag version, and content digest.

#### Scenario: Matching installation is unchanged
- **GIVEN** the installed ESDiag skill has the same manifest and digest as the embedded skill
- **WHEN** installation runs
- **THEN** the target reports `unchanged`
- **AND** no skill file is rewritten

#### Scenario: Installer-owned older version is updated
- **GIVEN** the target contains an intact ESDiag-managed skill from an older binary version
- **WHEN** installation runs
- **THEN** the new skill is staged and validated before replacement
- **AND** the target is atomically updated with a recoverable backup until success
- **AND** ownership metadata is written last

#### Scenario: Locally modified installation is protected
- **GIVEN** the target's files do not match its recorded installer digest
- **WHEN** installation runs without explicit force
- **THEN** the target reports a conflict
- **AND** no existing file is overwritten or deleted

#### Scenario: User explicitly replaces a conflicting installation
- **GIVEN** the target contains an unrecognized or locally modified ESDiag skill
- **WHEN** installation runs with `--force`
- **THEN** the original directory is preserved as a recoverable backup until the staged embedded skill is installed successfully
- **AND** the result identifies that the target was force-updated

### Requirement: Skill Installation Has Structured Per-Target Results
The command SHALL emit one finite structured YAML outcome by default, or equivalent compact JSON when requested, containing detected targets, selected targets, action for each target, destination, ESDiag version, digest, and restart/reload guidance. It SHALL exit non-zero when any selected target fails or conflicts while preserving successful per-target facts in the structured failure context.

#### Scenario: Two targets install successfully
- **WHEN** the embedded skill is installed into two selected agent homes
- **THEN** the outcome contains one `installed`, `updated`, or `unchanged` entry per target
- **AND** indicates that running agent processes may need to restart or reload

#### Scenario: One target fails after another succeeds
- **WHEN** one selected target installs and another cannot be written
- **THEN** the command exits non-zero
- **AND** its structured failure reports both the successful and failed target actions
- **AND** it does not roll back or conceal the independently completed target

### Requirement: Installer Is Composable By Onboarding
The skill installer SHALL expose the same in-process target detection, preflight, and installation service for `esdiag init`; onboarding MUST NOT spawn another ESDiag process, download a skill, or duplicate host-specific path or ownership logic. Skill installation remains an optional final onboarding stage: declining it, or a target conflict or failure, MUST NOT invalidate already-completed ESDiag configuration.

#### Scenario: User accepts detected or explicit targets during initialization
- **GIVEN** initialization has completed its required diagnostic workflow stages
- **AND** one or more supported agent homes are detected or explicitly selected
- **WHEN** the user approves skill installation
- **THEN** initialization invokes the embedded installer in process
- **AND** its outcome contains each target action and restart/reload guidance

#### Scenario: User declines optional skill installation
- **WHEN** the user declines the agent-skill stage during initialization
- **THEN** initialization completes successfully without modifying any agent home
- **AND** identifies `esdiag agent skills` as the standalone installation command

#### Scenario: Installation fails after core initialization succeeds
- **GIVEN** initialization has completed its required ESDiag configuration stages
- **WHEN** a selected agent target conflicts or cannot be written
- **THEN** the configured identity, keystore, output deployment, hosts, and job remain valid
- **AND** initialization reports the per-target failure and standalone recovery command
- **AND** does not claim that core ESDiag configuration failed

### Requirement: Installed Skill Remains Portable And Script-Free
Every installed target SHALL receive the same portable skill content used by repository and plugin packaging, without `scripts/`, provider-specific workflow duplication, or external executable runtime dependencies.

#### Scenario: Installed target is inspected
- **WHEN** a target installation completes
- **THEN** it contains the embedded `SKILL.md` and references
- **AND** contains no `scripts/` directory
- **AND** its operational instructions compose native `esdiag` commands
