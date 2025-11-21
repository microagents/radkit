---
title: Deployment (WASM)
description: Deploy Radkit agents as portable, secure WebAssembly modules.
---



Radkit is designed around a modern, portable, and secure deployment model: **WebAssembly (WASM)**. When you deploy a Radkit agent, you are not deploying a heavy container or a virtual machine; you are deploying a small, efficient `.wasm` file.

## The WASM Target

The standard deployment artifact for a Radkit agent is a **`wasm32-wasip1`** module. This is a WebAssembly binary that conforms to the first stable WebAssembly System Interface (WASI), allowing it to run outside of a browser and interact with system resources (like making HTTP requests) in a secure, sandboxed manner.

## The Deployment Process

Deploying an agent is typically handled by a CLI tool provided by the cloud platform (e.g., `radkit-cli deploy`). Here's what happens when you run the deploy command:

1.  **Compile to WASM**: The CLI invokes the Rust compiler on your project with the specific WASM target flag: `cargo build --target wasm32-wasip1 --release`.

2.  **Exclude Local Server**: Your `main.rs` file contains a `main` function for running the local test server. This function is decorated with `#[cfg(not(target_arch = "wasm32"))]`. This attribute tells the compiler to completely exclude the `main` function when building for the `wasm32` target. The resulting `.wasm` file is therefore a library module, not an executable.

3.  **Upload Artifact**: The CLI tool finds the compiled module in `target/wasm32-wasi/release/my_agent.wasm` and uploads this single file to the cloud platform's deployment API.

## Cloud Execution

Once the `.wasm` module is uploaded, the cloud platform takes over:

1.  **Instantiation**: The platform loads the `.wasm` module into a high-performance, secure WASM runtime (like Wasmtime).

2.  **Discovery via Configuration Function**: The runtime calls a known function in the module to retrieve the `AgentDefinition` you have defined. This is typically the `configure_agent()` function.

    ```rust
    pub fn configure_agent() -> AgentDefinition {
        // ... return an agent definition
    }
    ```

3.  **Service Injection**: The cloud platform provides its own production-grade implementations of the `Runtime` services (TaskManager, MemoryService, etc.) to the WASM module.

4.  **Execution**: The platform's gateway routes incoming A2A requests to the appropriate agent instance. The WASM runtime executes the corresponding skill handler logic from your module in response to each request.

## Advantages of the WASM Model

-   **Speed**: `.wasm` files are small and can be uploaded and started in seconds. WASM runtimes have near-zero cold start times, allowing agents to scale down to zero and respond to requests instantly.
-   **Security**: Every agent runs in its own secure, memory-safe sandbox, completely isolated from other agents. The WASI interface ensures that the agent can only access the resources it has been explicitly granted permission to use.
-   **Portability**: The same `.wasm` file can be run on any server that has a compliant WASM runtime, regardless of the underlying CPU architecture or operating system.
-   **Scalability**: The lightweight nature of WASM instances allows for massive scaling, with thousands of agents running concurrently on a single physical machine.
