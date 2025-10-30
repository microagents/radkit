
# Cloud Platform

This document specifies the features and architecture of the managed cloud platform, which provides a seamless experience for deploying, managing, and observing agents.

## 1. The Gateway & Agent Registry

The cloud platform acts as a central **Gateway** and **Agent Registry** for all deployed agents. This is the public-facing entry point for all interactions.

-   **Discovery**: When an agent is deployed, its `AgentCard` is automatically published to the platform's registry. The gateway exposes a queryable API, allowing users and other agents to discover agents based on their name, skills, or other metadata, as described in the A2A `agent-discovery` specification.
-   **Authentication**: The gateway is the authentication boundary. It handles all incoming authentication (OAuth 2.0, API Keys, etc.) as specified in the agent's `AgentCard`. It validates credentials and passes a verified identity principal to the agent's execution environment. The agent's own code does not need to handle complex authentication flows.
-   **Routing**: The gateway is responsible for routing incoming A2A requests to the correct running agent instance based on the request's target.

## 2. Observability UI

A core feature of the cloud platform is a rich web UI that provides deep insights into an agent's behavior. When a developer uses the `PaidRuntime`, all of the following data is automatically streamed to the UI in real-time, even from an agent running on a local machine.

-   **Live Logging**: All logs emitted by the agent (via the `runtime.logging_service()`) are streamed to the UI and are searchable.
-   **Distributed Tracing**: The platform automatically generates and propagates trace IDs for all interactions, including calls between agents (`#[remote_agent]`) and calls to `#[llm_function]s`. The UI presents a visual flame graph or timeline of the entire request, making it easy to debug performance bottlenecks.
-   **Task Visualization**: The UI provides a dedicated view for each A2A `Task`. It shows the current status, the complete history of messages and tool calls, and any generated artifacts. This allows developers to inspect the agent's "thought process" step-by-step.

## 3. Hosted Services

The cloud platform provides scalable, persistent, production-grade implementations of the services defined in the `Runtime` trait. These are the core paid offerings.

-   **Hosted TaskManager**: A database-backed implementation of the `TaskManager`. It persists the state of all A2A `Task`s and their associated `TaskEvent`s, ensuring that no data is lost and that long-running tasks can be resumed reliably.

-   **Hosted Memory Service**: A managed vector database and search service. When a skill calls `runtime.memory_service().save(...)`, the data is embedded and stored in a persistent, scalable vector store. The `.search()` method performs efficient similarity searches against this knowledge base, providing a powerful and easy-to-use RAG (Retrieval-Augmented Generation) capability for all agents on the platform.

## 4. Deployment and Lifecycle Management

The platform is responsible for the entire lifecycle of a deployed agent.

-   **Builds**: For Git-based deployments, the platform automatically builds the agent from source using a Rust Buildpack.
-   **Execution**: The platform runs the agent's `.wasm` module in a secure, multi-tenant WASM runtime.
-   **Scaling**: The platform automatically scales agent instances up and down based on demand, including scaling to zero to save costs.
-   **Updates**: Redeploying an agent is an atomic operation. The platform ensures zero-downtime deployments by routing traffic to the new version only after it is healthy and ready.
