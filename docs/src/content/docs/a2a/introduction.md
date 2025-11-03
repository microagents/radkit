---
title: Introduction to A2A
description: Learn about the Agent-to-Agent protocol and Radkit's A2A-native approach to building agents.
---



Radkit is built from the ground up to support the **Agent-to-Agent (A2A) protocol**. This makes it different from many other agent frameworks that focus only on single-agent, single-user interactions.

## What is A2A?

The [A2A protocol](https://a2a-protocol.org) is an open, standardized set of rules that allows AI agents to communicate and collaborate with each other, regardless of who built them or where they are hosted.

Think of it like email or the web: a common language that enables a vast, decentralized network. Instead of a network of documents, A2A enables a network of intelligent agents.

## Why is A2A Important?

An ecosystem of agents that can't talk to each other is just a collection of isolated silos. A2A provides the missing link, enabling powerful new capabilities:

-   **Interoperability**: An agent built by Company A can discover and delegate tasks to an agent built by Company B.
-   **Composition**: You can build complex systems by composing the specialized skills of multiple agents. Your HR agent could delegate to an IT agent to provision a new account, for example.
-   **Standardization**: A2A provides a standard way to handle common agent workflows, such as:
    -   Agent discovery (finding other agents)
    -   Task lifecycle management (tracking a task from `submitted` to `completed` or `failed`)
    -   Multi-turn conversations where an agent needs more input.
    -   Streaming updates for long-running tasks.
    -   Generating and sharing artifacts (like files or reports).

## Radkit's A2A-Native Approach

Radkit doesn't just "support" A2A; it's **A2A-native**. The entire framework, from its core types to its state management, is designed around the A2A specification.

This has a huge benefit for you as a developer: **if your code compiles, it's automatically A2A-compliant.**

Radkit's type system and abstractions guarantee that your agent correctly handles the A2A protocol's state transitions, event formats, and metadata requirements. You can focus on your agent's unique logic, and Radkit handles the protocol compliance for you.

In the following sections, you'll learn how to build A2A-compliant agents using Radkit's powerful `Skill` and `Agent` primitives.