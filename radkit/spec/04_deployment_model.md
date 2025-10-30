
# Deployment Model: WASM

This document specifies the technical strategy for deploying agents to the cloud platform. The entire model is built around WebAssembly (WASM) to ensure high performance, security, and portability.

## 1. Compilation Target

The standard deployment artifact for an agent is a **`wasm32-wasip1`** module. This is a WebAssembly binary that conforms to the first stable WebAssembly System Interface (WASIp1), allowing it to run outside of a web browser and interact with system resources (like making HTTP requests) in a secure, sandboxed manner. As the WASI standard evolves (e.g., wasip2), the platform will adopt newer versions once they are no longer experimental.

## 2. The Deployment CLI

A developer deploys their agent using a command-line interface (CLI) tool provided by the cloud platform (e.g., `cloud-cli deploy`). This tool is responsible for the entire packaging and deployment process.

When a developer runs `cloud-cli deploy`, the tool performs the following steps:

1.  **Compile to WASM**: It invokes the Rust compiler on the developer's project with the specific WASM target flag: `cargo build --target wasm32-wasip1 --release`.

2.  **Conditional Compilation**: The agent's `main.rs` file contains a `main` function for local testing, but this function is decorated with `#[cfg(not(target_arch = "wasm32"))]`. This attribute instructs the compiler to completely exclude the `main` function when building for the `wasm32` target. The resulting `.wasm` file is therefore a library module, not an executable.

3.  **Locate Artifact**: The CLI tool finds the compiled module in the standard output directory (e.g., `target/wasm32-wasi/release/my_agent.wasm`).

4.  **Upload to Platform**: The CLI then sends this single `.wasm` file to the cloud platform's deployment API via a secure HTTP request.

## 3. Cloud Execution

Once the `.wasm` module is uploaded, the cloud platform handles the rest:

1.  **Instantiation**: The platform loads the `.wasm` module into a high-performance, secure WASM runtime (such as Wasmtime).

2.  **Discovery via Entrypoint**: The runtime calls a single, known function from the module to retrieve the `AgentDefinition`s. The library provides an `#[entrypoint]` macro to designate this function.

    While the system could be designed to find multiple `#[entrypoint]` functions, the convention is to have **one per WASM module** for simplicity and clarity. This function acts as the main configuration entrypoint for the entire project.

    ```rust
    #[entrypoint]
    pub fn configure_agents() -> Vec<AgentDefinition> {
        // ... return agent definitions
    }
    ```

    The macro handles the low-level details of exporting the function with the specific name and signature that the cloud platform expects.

3.  **Service Injection**: The platform provides its own production-grade implementations of the `Runtime` services (TaskManager, Memory, Logging, etc.) to the WASM module.

4.  **Execution**: The platform's gateway routes incoming A2A requests to the appropriate agent instance. The WASM runtime executes the corresponding skill handler logic from the module in response to each request.

## 4. Advantages of the WASM Model

-   **Speed**: `.wasm` files are small and can be uploaded in seconds. WASM runtimes have near-zero cold start times, allowing agents to scale down to zero and respond to requests instantly.
-   **Security**: Every agent runs in its own secure, memory-safe sandbox, completely isolated from other agents.
-   **Portability**: The same `.wasm` file can be run on any server that has a compliant WASM runtime, regardless of the underlying CPU architecture or operating system.
-   **Scalability**: The lightweight nature of WASM instances allows for massive scaling, with thousands of agents running concurrently on a single physical machine.
