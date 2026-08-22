## Why

Binary-first ESDiag users currently need to download and install a second
`esdiag-local` executable before they can provision a local output stack. The
standalone launcher then pulls an ESDiag container even when the matching native
binary is already present.

## What Changes

- Add `esdiag local <command>` as a Rust-owned local-stack lifecycle interface
  without a persistent script installation.
- Add `--stack=auto|full|core` lifecycle selection. Native auto deployments
  default to core; `full` explicitly forces the ESDiag container service.
- Persist a resolved stack mode per managed state directory so later PATH
  changes do not silently change a deployment's ownership model.
- Define shared stack-state and lifecycle compatibility for `esdiag-local` and
  `esdiag local`, while keeping ESDiag user configuration and keystore state
  owned by the selected runtime in phase one. Mode changes must not migrate
  configuration implicitly.
- Let interactive initialization offer to start a binary-owned core stack when
  the user selects local processing and no usable local deployment exists.
- Keep the downloaded standalone `esdiag-local` artifact as a script-first
  distribution path, including its existing self-update behavior.
- Add `esdiag-local exec [launcher-options] -- <esdiag-arguments>` as the
  container-only ESDiag CLI path. It will execute commands in the existing
  full-mode Compose service instead of requiring a separate wrapper artifact.

## Capabilities

### New Capabilities

- `embedded-local-stack-launcher`: Rust-owned local-stack lifecycle,
  core/full modes, shared-state compatibility, and onboarding handoff.

### Modified Capabilities

- `standalone-local-stack`: Define mode-aware standalone lifecycle behavior,
  including native-binary detection, core deployments that omit ESDiag
  containers, and full-mode container CLI execution.

## Impact

- Affects the Rust CLI command surface, local-stack state module, Compose
  orchestration, and native process dispatch.
- Affects `bin/esdiag-local`, its standalone tests, release behavior, and
  local-stack documentation and Agent Skill onboarding.
- Changes `esdiag init` to use the binary's direct core-stack lifecycle.
