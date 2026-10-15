## MODIFIED Requirements

### Requirement: Portable Skill Is Embedded In The Binary
An ESDiag binary built with the Cargo `agent` feature SHALL embed the canonical script-free `.agents/skills/esdiag/` skill at compile time using `rust_embed`. The embedded assets SHALL include `SKILL.md`, `references/`, and supported `agents/` metadata, MUST exclude executable helper scripts, and SHALL identify the running ESDiag package version and a deterministic content digest.

#### Scenario: Cargo-installed binary contains the skill
- **WHEN** a user installs ESDiag with `cargo install esdiag`
- **THEN** the resulting binary contains the ESDiag skill assets compiled from the same package version
- **AND** installation requires no source checkout or later network download

#### Scenario: Homebrew binary contains the skill
- **WHEN** a packaged ESDiag binary is installed through Homebrew
- **THEN** `esdiag setup skills` can read the embedded skill while offline
- **AND** the embedded content matches the binary's ESDiag version

### Requirement: Agent Skills Command Detects Supported Hosts
The CLI SHALL provide `esdiag setup skills [<claude|codex|opencode>]... [--force]` to detect supported user-scoped Claude Code, Codex, and OpenCode skill targets and install the embedded ESDiag skill into every detected target. It SHALL support explicit repeatable positional target selection when automatic detection is insufficient and MUST NOT modify a project-local skill directory implicitly. The former `esdiag agent skills` spelling is removed.

#### Scenario: Multiple installed agents are detected
- **GIVEN** supported user homes exist for Codex and OpenCode
- **WHEN** the user runs `esdiag setup skills`
- **THEN** both targets are selected for installation
- **AND** an undetected Claude Code target is reported without being created implicitly

#### Scenario: Explicit target has no existing home
- **GIVEN** Claude Code is not automatically detectable
- **WHEN** the user runs `esdiag setup skills claude`
- **THEN** the command creates the documented user-scoped skill parent as needed
- **AND** installs no project-local files

#### Scenario: No agent is detected
- **WHEN** default detection finds no supported target
- **THEN** the command makes no filesystem changes
- **AND** returns a structured outcome listing supported explicit targets

#### Scenario: Removed spelling is rejected
- **WHEN** the user runs `esdiag agent skills`
- **THEN** the command fails as a usage error
- **AND** no agent home is modified

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
- **AND** identifies `esdiag setup skills` as the standalone installation command

#### Scenario: Installation fails after core initialization succeeds
- **GIVEN** initialization has completed its required ESDiag configuration stages
- **WHEN** a selected agent target conflicts or cannot be written
- **THEN** the configured identity, keystore, output deployment, hosts, and job remain valid
- **AND** initialization reports the per-target failure and `esdiag setup skills` as the standalone recovery command
- **AND** does not claim that core ESDiag configuration failed
