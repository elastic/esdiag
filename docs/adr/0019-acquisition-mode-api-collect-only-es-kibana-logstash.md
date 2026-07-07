---
status: accepted
---

# ESDiag API-collects only Elasticsearch/Kibana/Logstash; everything else is product-provided

`Collect` (pulling live APIs) is scoped to **Elasticsearch, Kibana, and Logstash**.
Elastic Agent and all platform diagnostics are **product-provided**: the product or
platform generates its own bundle, and ESDiag consumes it via `Load` — it never
API-collects them. This defines what the `Collect` stage actually applies to, and
distinguishes deliberate scope boundaries from unfinished work.

## Acquisition by product

- **API-collectable** (ESDiag pulls live APIs, per the collection definition):
  Elasticsearch, Kibana, Logstash.
- **Agent** — Elastic Agent produces its *own* diagnostic bundle. ESDiag `Load`s it;
  API collection is **out of scope by design**. Processing is in progress (PR293).
- **Platform** (ECE, ECK, KubernetesPlatform) — the platform generates its own
  diagnostic. ESDiag `Load`s the platform bundle (and processes ECK/K8P; ECE carries
  no application data). Platform-level API collection is **out of scope**.

## Two kinds of gap (distinct, not interchangeable)

- **Out-of-scope by design:** Agent API collection; platform-level API collection.
- **Not yet implemented (in progress):** Kibana processing (on a branch); Agent
  processing (PR293).

Both surface today as a skip/unsupported error, but they mean opposite things — see
the `Skipped` refinement in ADR-0016.

## Consequences

- **The `Collect` stage applies only to Elasticsearch/Kibana/Logstash.** Agent and
  platform diagnostics enter the pipeline via `Load`, so a job over them is
  `Load → [Process] → …`, never `Collect`.
- **A possible future "trigger generation then Load"** — ESDiag could *initiate* an
  ECE/ECK/Agent bundle generation and then `Load` the result. That is a delegated
  acquisition flow distinct from API `Collect` (the product collects itself); noted as
  a potential capability, not current scope.
- The collection definition / registry (ADR-0005) only ever describes API sources for
  the three API-collectable products.
