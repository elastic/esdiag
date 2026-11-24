---
type: Reference
title: "Syscall sources are selected per source, not by platform"
status: accepted
tags: [repository, adr]
---

# Syscall sources are selected per source, not by platform

ESDiag processes host system call output wherever a bundle contains it, one source at
a time, as the user or diagnostic type selects it. Which syscall sources exist, where
their output lives in a bundle, and which diagnostic types process them by default is
recorded in an ESDiag-owned syscall source catalog, separate from the REST
collection definitions. This amends the ADR-0001 consequence that `SelfManaged`
yields opt-in Linux syscalls available under no other platform.

## Context

ADR-0001 dispatched infrastructure data off the platform: `SelfManaged` implied Linux
syscalls, and the presence of a `syscalls` folder was one of the indicators that set
the platform. Real collections break that coupling. ECE allocators and Kubernetes
nodes running ECK can also produce `syscalls/` output such as `sar.txt`, and a single
host there runs several Elasticsearch nodes. Tying processing to `SelfManaged` would
discard that data, and treating it as node data would duplicate host metrics per node.

The upstream `support-diagnostics` `diags.yml` lists dozens of OS commands. Most are
collect-only or feed other processors, and their availability varies by OS and
version. A REST `sources.yml` entry is version-gated by request path, which does not
describe a file in a bundle.

## Decision

- Syscall sources live in `assets/syscalls/sources.yml`, keyed by `diags.yml` command
  keys. Each entry names its bundle-relative file and, when a processor exists, marks
  itself processable, sets its processing weight, and lists the diagnostic types that
  process it by default. The runtime never reads `diags.yml`.
- A processable source is exposed as the processing option `syscalls.<key>`. It is
  never required, and it is processed from any bundle that contains its file,
  regardless of platform.
- A source selected only through its diagnostic type's defaults is skipped silently
  when its file is absent. A source the user explicitly requested is a warning when
  absent. A file that exists but cannot be parsed is a warning in either case.
- Host-scoped output, such as SAR, is written to a host data stream and correlated to
  the Elasticsearch nodes on that host, rather than enriched as node data.

## Alternatives considered

- **Keep dispatching syscalls off `SelfManaged`.** Rejected: it drops ECE-allocator
  and ECK node output, and platform inference is best-effort, so a correct bundle
  could still lose its host data.
- **Add syscall entries to the REST `sources.yml`.** Rejected: REST entries are
  version-gated request paths; syscall entries are bundle files with per-OS commands,
  so a shared schema would carry fields meaningless to each half.
- **Read `diags.yml` at runtime.** Rejected: it is an upstream reconciliation input,
  and most of its commands have no processor.

## Consequences

- Adding a syscall processor means adding one catalog entry and one dispatch entry;
  a startup check keeps the two in agreement.
- The `syscalls` folder may still inform platform inference, but it no longer gates
  processing.
- Default processing stays quiet for bundles without syscall output, while explicit
  requests surface missing or unparseable files as report warnings.
