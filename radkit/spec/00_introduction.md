
# Introduction: An Ecosystem for AI Agents

## 1. Vision

This document specifies a complete ecosystem for building, deploying, and managing next-generation AI agents. The vision is to create a seamless, integrated experience for building and managing AI agents, which consists of two core components:

1.  **The Framework**: An open-source Rust library, `radkit`, designed to help developers build powerful, robust, and A2A-compliant agents with an elegant and productive developer experience.

2.  **The Cloud Platform**: A managed cloud service that seamlessly deploys, hosts, and enhances agents built with the framework. It provides enterprise-grade features like authentication, a global agent gateway, persistent memory, and a rich observability UI for logs and tracing.

## 2. Core Principles

This entire system is built upon a foundation of three core principles:

### a. A2A-Native

The Agent2Agent (A2A) protocol is the native language of all agents built with this framework. The library is designed from the ground up to correctly and elegantly implement the A2A specification, including the Task lifecycle, the distinction between Messages and Tasks, and the use of a shared `contextId` for multi-task collaboration.

### b. Developer Experience & Control

The framework prioritizes developer experience and control above all else. Developers maintain complete control over agent behavior, execution flow, context management, and state. The library provides powerful abstractions without hiding complexity—developers can always drop down to lower-level APIs when needed. 
### c. WASM-Powered

The deployment model is centered around WebAssembly (WASM). Developers write standard Rust code, and the cloud platform's deployment tools compile it into a secure, high-performance, and portable WASM module, specifically targeting **wasip1** (the first stable version of the WebAssembly System Interface). This ensures broad compatibility and security. In the future, the platform will adopt wasip2 and beyond as they become standardized and production-ready.

This specification will detail the architecture of the library, the ideal developer experience, the WASM deployment model, and the features of the cloud platform.
