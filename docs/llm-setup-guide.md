---
type: Guide
title: Configure an LLM for Elastic AI Agent
description: Connect Elastic Inference Service or a local OpenAI-compatible model.
tags: [ai, agent, configuration]
---

# Configure an LLM for Elastic AI Agent

Agent Builder needs a Kibana inference connection. Use Elastic Inference
Service for a managed model, or connect an OpenAI-compatible local model.

## Before you start

- Kibana 9.5 or later.
- An Enterprise license or trial.
- Access to the Kibana space where ESDiag installed its Agent Builder assets.
- A network path from Kibana to the model endpoint.

If Kibana hides model-management pages, switch the space to the Observability
solution view:

```text
Stack Management > Spaces > Actions > Edit > Solution view > Observability
```

## Elastic Inference Service

Use Elastic Inference Service when the cluster can connect to Elastic Cloud.

1. Sign in to Kibana.
2. Search for **Cloud Connect**.
3. Select **Log in** or **Sign up**, then authenticate to the Elastic Cloud
   organization that will pay for inference.
4. Return to Kibana, provide the Cloud Connect API key, and select
   **Connect**.
5. Under **Cloud connected services**, connect **Elastic Inference Service**.
6. Open **AI Agent** and check that **Elastic AI Agent** is available in the
   `esdiag` space.

If Kibana has no default model, search for **GenAI Settings**, select an
Elastic-managed connector as the default, and save it.

Elastic Inference Service is metered. Check the current account and billing
requirements in Elastic's
[self-managed EIS guide](https://www.elastic.co/docs/explore-analyze/elastic-inference/connect-self-managed-cluster-to-eis).

## Local OpenAI-compatible model

Register the model as an Elasticsearch `chat_completion` inference endpoint.
The Elasticsearch node, rather than Kibana, must be able to reach the model
server. See
[Connect Agent Builder to a local LLM](setup/local-llm.md) for Kibana Dev
Tools requests, equivalent `elastic` CLI commands, and Agent Builder checks.
