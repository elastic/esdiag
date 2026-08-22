# ESDiag Agent Skill

Collect, process, and analyze Elastic Stack diagnostics from Claude Code, Codex, or OpenCode.

The analysis itself runs on **your** Elastic deployment, through the diagnostic skill that `esdiag setup` installs into Agent Builder. Your local agent orchestrates and presents; the cluster reasons. Every analysis and follow-up is saved as a normal Kibana Agent Builder conversation, so you can continue the same thread from Kibana later.

- The analysis always reflects the assets actually installed on that cluster, so it cannot drift from what Kibana would tell you.
- The token cost lands on the deployment's inference connector rather than your Claude quota. A measured analysis consumes roughly 110,000 input tokens on the cluster and returns about 1,100 tokens of markdown. Follow-up questions cost almost nothing locally because the conversation lives on the cluster.

That trade is a transfer, not a saving: cluster input tokens grow with each turn as the conversation replays.

## Claude Code installation

```sh
/plugin marketplace add elastic/esdiag
/plugin install esdiag@esdiag
```

Claude Code discovers the skill through `.claude-plugin/plugin.json`. The explicit skill command is `/esdiag:esdiag`, or you can ask naturally.

## Codex and OpenCode

The package also includes `.codex-plugin/plugin.json`, pointing at the same generated `skills/` directory. Codex users working in this repository can invoke the canonical project skill as `$esdiag` without installing the package.

OpenCode discovers the same canonical skill directly at `.agents/skills/esdiag/`. No JavaScript or TypeScript OpenCode plugin is needed because this integration is an Agent Skill, not an OpenCode event hook.

Any installed `esdiag` binary can instead install its matching offline skill in a supported user scope:

```sh
esdiag agent skills
```

Use `--target claude`, `--target codex`, or `--target opencode` for an explicit target. Existing locally modified skills are protected unless the user explicitly chooses `--force`.

## Two setups, not one

These are separate problems with separate prerequisites and separate failure modes. Most users only need the second.

### Provisioning a cluster (once per deployment, often already done)

An Elastic deployment with the ESDiag assets installed (`esdiag setup`), a suitable subscription, and **model access**.

Model access is a deployment prerequisite that no `esdiag` command provisions. Satisfy it by activating the Elastic Inference Service through Cloud Connect with an Elastic Cloud account, or by configuring your own LLM provider and connector.

### Configuring this machine (once per user, per machine)

Run the terminal-native initializer to configure a diagnostic user, encrypted credentials, linked Elasticsearch/Kibana output deployment, collection host, and first saved job. Do not provide secrets to an agent conversation.

```sh
esdiag init
```

`esdiag agent ask` uses this same output deployment. It needs no analysis-specific URL, API-key, inference, job, freshness, or local conversation configuration.

## Invocation

| Host | Explicit invocation |
|---|---|
| Claude Code plugin | `/esdiag:esdiag` |
| Codex project skill | `$esdiag` |
| OpenCode project skill | Ask OpenCode to use the `esdiag` skill |

For every host, natural requests work too: *"Check my ESDiag connection"* or *"How is my cluster looking today?"*

## Asking Agent Builder

Submit a finite question to the configured Agent Builder agent:

```sh
esdiag agent ask "Analyze diagnostic <diagnostic-id>"
```

The YAML outcome includes the answer, a conversation ID, and a Kibana handoff link. Use `--conversation <id>` for an explicit follow-up; omitting it starts a new Kibana conversation. An interrupted request that returns `retry_safe: false` must not be resent automatically: continue from the Kibana link instead.

The agent ID defaults to `elastic-ai-agent` and can be overridden with `--agent`. Inference routing and Agent Builder tooling belong to the deployment configuration, not local environment variables.

## When a new diagnostic gets collected

Collection issues API calls against a live production cluster, so it follows the request rather than a default:

- "my last diagnostic", an explicit id, or any follow-up → reuses what exists
- "collect a new diagnostic" → collects, no confirmation needed
- "how is my cluster looking today?" → reuses a diagnostic newer than `ESDIAG_DIAGNOSTIC_MAX_AGE`, and **asks first** when the latest is older or absent

## Development

The bundled skill under `skills/` is generated from `.agents/skills/esdiag/`, including its `references/` directory:

```sh
./bin/sync-plugin-skill.sh          # regenerate
./bin/sync-plugin-skill.sh --check  # fail on drift
./tests/plugin.sh                   # non-networked tests
```

Edit `.agents/skills/esdiag/`, never the generated `plugin/skills/esdiag/`. Host manifests contain discovery metadata only; workflow behavior belongs in the canonical skill.
