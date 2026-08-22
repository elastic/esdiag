---
name: esdiag
description: Collect, share, process, or analyze Elasticsearch, Kibana, and Logstash diagnostics with `esdiag`. Setup or install esdiag tools; Ask Kibana Agent Builder diagnostic questions.
---

# ESDiag

Choose one operation guide before suggesting a command:

1. `esdiag` is available. Read `references/esdiag-cli.md`.
2. `esdiag-local` is available. Read `references/esdiag-local.md`.
3. `esdiag-lite.sh` or `esdiag-lite.ps1` is available. Read
   `references/esdiag-lite.md`.
4. None of these tools is available, or the selected tool cannot do the work.
   Read `references/onboarding.md`.

Prefer the first available tool that supports the requested operation. ESDiag
Lite cannot process or analyze diagnostics. A full-mode container-only
installation supports CLI work through `esdiag-local exec -- <arguments>`;
core mode needs the native binary.

Keep passwords, API keys, and keystore values in the user's terminal. Do not
write ESDiag state files in an agent conversation.
