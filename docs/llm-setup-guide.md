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

- Kibana 9.4 or later.
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

Use this path when Kibana can reach a local or organization-managed
OpenAI-compatible endpoint. Ollama is one example.

Start the model before you configure Kibana:

```sh
ollama pull llama3.2
```

Check the endpoint from the machine that can reach it:

```sh
curl http://localhost:11434/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "llama3.2",
    "messages": [{"role": "user", "content": "Say hello!"}]
  }'
```

## Add the connector

1. In Kibana, open **Stack Management > Connectors**.
2. Select **Create connector**, then select **OpenAI**.
3. Select **Other OpenAI-compatible service**.
4. Set the model name to the exact name served by the endpoint, such as
   `llama3.2`.
5. Enter the endpoint URL that Kibana can reach.
6. Save the connector and select it as the default model in **GenAI Settings**.

For Kibana on the same machine as Ollama, the endpoint is usually:

```text
http://localhost:11434/v1/chat/completions
```

For Kibana in a container, use an address the container can reach. On Docker
Desktop this is commonly:

```text
http://host.docker.internal:11434/v1/chat/completions
```

The connector form may require an API key even when the local endpoint does
not. Use a value that meets the form requirement, and do not reuse a real
credential.

## Check the agent

Open **AI Agent** in the `esdiag` space. Select the connector and confirm
**Elastic AI Agent** is selected. Process a non-sensitive archive with
`esdiag process --ask` or ask a question with `esdiag agent ask`.
