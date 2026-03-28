## Context

`esdiag` already supports an encrypted local keystore for host secrets, but command-line runs outside the web session model only resolve keystore-backed hosts when the password is available through `ESDIAG_KEYSTORE_PASSWORD` or an in-process scoped password. This forces repeated password entry or long-lived shell environment secrets for normal CLI workflows, which is especially awkward for agentic command execution.

This change is cross-cutting because it affects CLI surface area, password resolution, local file management, keystore mutation safety, and host authentication flows that span `src/main.rs`, `src/data/keystore.rs`, and `src/data/known_host.rs`. It also introduces new security-sensitive local state in `~/.esdiag/keystore.unlock`, so the design needs to be explicit about lifetime, deletion, and protection boundaries.

## Goals / Non-Goals

**Goals:**
- Let users unlock the local keystore once for CLI workflows and reuse that access for a bounded time without re-entering the password for each command.
- Preserve existing non-interactive behavior so `ESDIAG_KEYSTORE_PASSWORD` remains the highest-precedence persistent source for automation.
- Add explicit CLI commands for unlock, lock, status, password rotation, and safe secret mutation semantics.
- Prevent the unlock lease file from storing the password in cleartext while keeping the implementation self-contained and cross-platform.
- Make expired unlock state self-healing by treating stale lease files as locked state and deleting them on read when possible.

**Non-Goals:**
- Introduce OS-native keychain integration or platform-specific secret storage.
- Change the encrypted keystore file format used for persisted secret records.
- Provide strong protection against a determined local user who already has access to the same account, home directory, and keystore files.
- Add web-session or service-mode changes beyond reusing existing password resolution precedence.

## Decisions

### 1. Add a dedicated CLI keystore lifecycle command set

The CLI will add `esdiag keystore unlock`, `lock`, `status`, `password`, and `update`, while changing `add` to fail when the secret already exists.

Rationale:
- `unlock` and `lock` make the cached-password lifecycle explicit.
- `status` is necessary once unlock state can expire independently of the process.
- `password` separates password rotation from secret mutation and avoids overloading `unlock`.
- Splitting `add` and `update` removes a silent overwrite hazard from an encrypted store whose values are intentionally not echoed back to the user.
- `add` and `update` can still accept explicit flag values, but interactive masked prompts reduce the need to paste API keys or passwords directly into shell history.

Alternatives considered:
- Keep `add` as upsert and document it better. Rejected because accidental overwrite remains easy and hard to inspect.
- Fold password rotation into `unlock`. Rejected because rotating the keystore password is a distinct mutation workflow with different prompts and validation.

### 1a. Prompt for secret material when flags omit values in interactive shells

For `keystore add` and `keystore update`, if the selected auth mode requires an API key or password and the flag is present without a value, or the parsed value is otherwise absent, the CLI will prompt for that secret material using masked terminal input when stdin/stdout are interactive. Non-interactive execution will continue to require explicit values from arguments or other supported non-interactive sources.

Rationale:
- Users often want to avoid exposing API keys or passwords in shell history or pasted command lines.
- The project already uses masked password prompting for keystore passwords, so this extends an existing user interaction pattern.

Alternatives considered:
- Require all secret values inline on the command line. Rejected because it unnecessarily encourages secret exposure in terminal history.
- Make prompting the only supported path. Rejected because automation and copy-paste workflows still need explicit argument-based entry.

### 2. Represent CLI unlock state as a lease file with bounded TTL

`esdiag keystore unlock` will write `~/.esdiag/keystore.unlock` containing a versioned envelope with an expiration epoch and the cached keystore password. The default TTL is 24 hours, `--ttl` accepts human-friendly single-suffix durations such as `90m`, `24h`, and `7d`, and TTL is capped at 30 days.

Rationale:
- A file-based lease survives process boundaries and works for agentic CLI runs.
- A hard cap keeps the feature from quietly becoming indefinite unlock state.
- Embedding the expiration in the file lets ordinary command paths validate the lease without background cleanup machinery.

Alternatives considered:
- No expiration, explicit lock only. Rejected because it is too easy to leave unlock state behind indefinitely.
- Store only a derived key instead of the password. Rejected because the current keystore encryption rotates salt on write and still fundamentally depends on the password as the durable input.

### 3. Use lightweight local encryption for the unlock file

The unlock file will not store the password in plaintext. Instead, it will use a lightweight encrypted envelope derived from machine-local and user-local context plus a per-file salt so casual file reads do not reveal the password directly. This is an obfuscation-strength boundary, not a replacement for the keystore itself, and the design will document that anyone with equivalent local access may still be able to recover the cached password.

Rationale:
- This satisfies the product need to avoid plaintext password exposure in accidental file inspection, including AI agent file reads.
- The repository already depends on `aes-gcm-siv` and `pbkdf2`, so this can be implemented without adding external dependencies.
- A light wrapper is sufficient because the real security boundary remains control of the user account, the unlock file, and the encrypted keystore together.

Alternatives considered:
- Store plaintext with strict file permissions only. Rejected because accidental exposure via file reads remains too easy.
- Add OS-native keychain integration. Rejected as out of scope and contrary to the self-contained, cross-platform goal.

### 4. Extend password source precedence instead of replacing existing behavior

Password lookup for keystore-backed host resolution will use this order:
1. In-process scoped password
2. `ESDIAG_KEYSTORE_PASSWORD`
3. Valid unexpired unlock lease file

Rationale:
- Existing web flows and explicit environment-based automation continue to behave as they do today.
- The new lease file becomes a fallback that enables CLI persistence without surprising CI or scripted usage.

Alternatives considered:
- Prefer the unlock file over the environment variable. Rejected because explicit runtime configuration should override local cached state.

### 4a. Let user-mode web sessions seed from an existing CLI unlock lease once per process

The user-mode web server may consume a valid CLI unlock lease as an initial in-memory session unlock source, but it will not create, refresh, extend, or write a new lease file as part of normal web session lifecycle. It may perform best-effort deletion of expired or otherwise stale leases encountered while reading the file. The seed attempt happens at most once per process lifetime, and an explicit web relock prevents immediate reseeding from the same file until the process restarts.

Rationale:
- This preserves the useful “unlock once locally” behavior for users who intentionally opted into the file-based flow.
- It keeps the web session contract distinct from CLI persistence because the web runtime still maintains its own 12-hour in-memory lease and does not extend the CLI TTL.
- Explicit relock remains meaningful in the browser session.

Alternatives considered:
- Re-read the unlock file on every locked web request. Rejected because web relock would become ineffective and the in-memory lease would effectively disappear.
- Ignore the unlock file completely in the web runtime. Rejected because it undermines the intended cross-CLI/web convenience for users who explicitly created the lease.

### 5. Split keystore validation from keystore creation

The current `authenticate()` behavior of creating an empty keystore when none exists will be separated into explicit operations so `keystore unlock` can distinguish between validating an existing keystore and interactively bootstrapping a new one.

Rationale:
- `unlock` should not silently create new secure state in non-interactive flows.
- Password rotation should fail cleanly if no keystore exists rather than implicitly creating one.

Alternatives considered:
- Keep auto-create behavior and special-case unlock around it. Rejected because it obscures user intent and makes status/rotation semantics harder to reason about.

### 6. Expired or malformed unlock files are self-cleaning best effort

When `esdiag` reads `keystore.unlock`, it will validate version, decrypt the envelope, and compare `expires_at_epoch` to the current time. Expired files are treated as locked state and deleted immediately on a best-effort basis. Malformed or unreadable files are also treated as locked state; the command may warn, but host resolution must not continue with partial state.

Rationale:
- This keeps normal command execution deterministic.
- Best-effort deletion avoids introducing hard failures when filesystem cleanup itself is blocked.

Alternatives considered:
- Fail commands when the unlock file is malformed. Rejected because a corrupt cache should not brick the user’s ability to fall back to env or interactive flows.

## Risks / Trade-offs

- [Local cache is still password-equivalent in practice] → Document clearly that `keystore.unlock` grants local keystore access and must be protected like the keystore itself.
- [Lightweight encryption may create a false sense of security] → State explicitly that the wrapping only prevents casual inspection and does not defend against a determined same-user attacker.
- [TTL parsing ambiguity] → Restrict accepted formats to integer plus a single-character suffix (`m`, `h`, `d`) and cap durations at 30 days.
- [New command semantics may surprise existing users of `keystore add`] → Update docs and help text to state that `add` is create-only and `update` is required for existing secrets.
- [Unlock cleanup failures may leave stale files behind] → Treat cleanup failures as warnings while still enforcing locked behavior for expired leases.
- [Interactive prompting could create ambiguous CLI parsing] → Define prompts only for secret material fields that are required by the chosen auth mode and absent at parse time.

## Migration Plan

1. Add the new CLI commands and data-layer helpers behind the existing `keystore` feature.
2. Refactor keystore helpers so creation, validation, password rotation, add, and update are explicit operations.
3. Extend password resolution to read the unlock lease after checking scoped and environment sources.
4. Update CLI docs and examples to describe unlock leases, TTL rules, `status`, `password`, the add/update split, and masked prompts for secret entry.
5. Existing users do not need data migration for `secrets.yml`; the unlock file is optional and created on demand.
6. Rollback is low-risk: newer CLI-generated unlock files can simply be deleted, and the keystore file format remains unchanged.

## Open Questions

- None at proposal time. The TTL format, maximum TTL, bootstrap behavior, expired-file deletion handling, and add/update split have all been decided for this change.
