# Elastic AI Agent: LLM Configuration Guide

This guide outlines the prerequisite requirements and step-by-step instructions for configuring Large Language Models (LLMs) to power the Agentic Diagnostic Assistant (ADA) and Elastic AI Agent.

It covers both the cloud-hosted **Elastic Inference Service (EIS)** integration and a **Local LLM** deployment designed for air-gapped or restricted enterprise environments.

---

## Prerequisites

Before starting the configuration, ensure your Elasticsearch and Kibana deployment meets the following foundational requirements:

*   **Kibana 9.4 Minimum**: ADA relies on the brand-new Skills API, which requires Kibana version 9.4 or higher. Older versions (like 9.2 or 9.3) do not support the skills framework.
*   **Enterprise License**: Advanced orchestration features, workflows, and the Agent Builder require an active Enterprise license or an Enterprise Trial license.
*   **Observability View**: This will help with the model management settings - you can navigate to:
    ```text
    Kibana Main Menu -> Stack Management -> Spaces -> Actions -> Edit -> Solution View -> Observability -> Apply changes.
    ```

---

## Method 1: Cloud Connection via Elastic Inference Service (EIS)

To connect your local deployment to the cloud-hosted Elastic Inference Service (EIS), use the following user-interface guided workflow:

1.  **Navigate to Cloud Connect**: Log in to Kibana, go to **Stack Management**, and search for the **Cloud Connect** settings page.
2.  **Authenticate to Elastic Cloud**: Click the connect option. This will redirect your browser to Elastic Cloud to authenticate with your organization or register for a trial.
3.  **Paste the API Key**: After successfully authenticating, copy the generated API key, return to the Kibana interface, and click **Connect**.
4.  **Reconnect EIS (If Prompted) and Verify the Connector**: Navigate to **Stack Management > Connectors** to verify your newly added AI connectors are active.
5.  **Click AI Agent -> Top right to start chatting!**

---

## Method 2: Local LLM Configuration or Air-Gapped Environments

For air-gapped, offline, or highly restricted environments where external internet access is prohibited, you can run a local model.

Kibana treats Ollama as a standard OpenAI service provider because Ollama provides a native, OpenAI-compatible API endpoint. This allows you to perform the entire setup directly within the Kibana user interface.

### Step 1: Prepare Your Local LLM

Before jumping into Kibana, ensure your local LLM instance is up and running with the model you want to use. Below is an example with Ollama.

1.  Open your terminal and download the model you want to run (e.g., Llama 3.2 or Gemma 4):
    ```bash
    ollama pull llama3.2
    ```
2.  Verify that the Ollama instance responds to the OpenAI endpoint format locally:
    ```bash
    curl http://localhost:11434/v1/chat/completions \
      -H "Content-Type: application/json" \
      -d '{
        "model": "llama3.2",
        "messages": [
          {
            "role": "user",
            "content": "Say hello!"
          }
        ]
      }'
    ```

### Step 2: Set Up the OpenAI Connector in Kibana

Configure Kibana to route its generative AI and Playground features to your local computer instead of the cloud.

1.  Log into your local Kibana dashboard.
2.  Navigate to **Stack Management > Alerts and Insights > Connectors** (or search for *Connectors* in the search bar).
3.  Click **Create connector** and select **OpenAI**.
4.  Fill out the connector configuration form with the following exact settings:
    *   **Connector name**: Name it something recognizable (e.g., `Ollama-Local`).
    *   **Select an OpenAI provider**: Choose **Other (OpenAI Compatible Service)**.
    *   **URL**: Enter the API route depending on your environment setup:
        *   *Standard local binary setup*: `http://localhost:11434/v1/chat/completions`
        *   *Docker environment (ES/Kibana running in container)*: `http://host.docker.internal:11434/v1/chat/completions`
        *   *Elastic Cloud (tunneling local port to the web)*: Tunnel your local port using a tool like ngrok and paste that public forwarding URL here (e.g., `https://<your-ngrok-id>.ngrok-free.app/v1/chat/completions`).
    *   **Default model**: Enter the exact model identifier you pulled locally (e.g., `llama3.2` or `gemma4`).
    *   **API key**: Ollama does not require local authentication, but Kibana requires this field to be filled to satisfy UI form validations. Type any arbitrary string (e.g., `local-secret`) to bypass this requirement.
5.  Click **Save**.

### Step 3: Launch and Chat

1.  In Kibana, top right, click **AI Agent**.
2.  Select your new local LLM from the bottom left LLM selector list, e.g., **"Ollama-Local"**.
3.  Ensure **"Elastic AI Agent"** is selected at the bottom right of the AI Prompt.
4.  Chat!

---

## Additional Settings and Documentation Setup

### Step 1: Configure Default AI Connector
From the Kibana left side menu, navigate to:
```text
Stack Management -> AI -> GenAI settings -> General -> Choose your default AI Connector
```

### Step 2: Install Elastic Documentation
Scroll down to the **Documentation** section and click **Install Elastic documentation**.
