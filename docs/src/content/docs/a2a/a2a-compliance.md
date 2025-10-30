---
title: A2A Compliance
description: How Radkit uses the Rust type system to guarantee A2A protocol compliance at compile time.
---



A core promise of Radkit is that if your agent code compiles, it is automatically compliant with the A2A protocol. This isn't magic; it's a result of careful API design that uses the Rust type system to enforce correctness at compile time.

Here are the key mechanisms Radkit uses to achieve this.

## 1. Typed State Management

The return types for the `SkillHandler` trait methods are not simple strings or booleans; they are specific enums that map directly to the A2A protocol's task states.

For `on_request`:
```rust
pub enum OnRequestResult {
    // Maps to A2A TaskState::InputRequired
    InputRequired { message: Content, slot: SkillSlot },
    // Maps to A2A TaskState::Completed
    Completed { message: Option<Content>, artifacts: Vec<Artifact> },
    // Maps to A2A TaskState::Failed
    Failed { error: String },
    // Maps to A2A TaskState::Rejected
    Rejected { reason: String },
}
```

**Guarantee:** It is impossible for you to return an invalid task state. You cannot, for example, accidentally return a state of "Done" or "Error". The compiler forces you to choose from one of the valid, A2A-defined terminal states.

## 2. Constrained Intermediate Updates

When you send a progress update during a task, you use the `TaskContext`:

```rust
// Always maps to an A2A TaskStatusUpdateEvent with state=working and final=false
task_context.send_intermediate_update("Processing...").await?;

// Always maps to an A2A TaskArtifactUpdateEvent
task_context.send_partial_artifact(artifact).await?;
```

**Guarantee:** The methods on `TaskContext` are carefully designed to prevent protocol violations. You cannot accidentally mark an intermediate update as "final" or send a "completed" status mid-execution. The API only allows you to send valid, non-terminal updates.

## 3. Automatic Metadata Generation

The `#[skill]` macro is not just for documentation. It's a code generation tool that creates the A2A metadata for you.

```rust
#[skill(
    id = "summarize_resume",
    name = "Resume Summarizer",
    /* ... */
)]
```

**Guarantee:** The macro automatically generates the `AgentSkill` entries for the agent's discovery card. This ensures that the advertised capabilities of your agent are always perfectly in sync with its actual implementation. There is no risk of your code and your agent's public card becoming inconsistent.

## 4. Automatic Protocol Mapping

Radkit provides a set of core types like `Content`, `Artifact`, and `Event`. The framework takes on the responsibility of converting these types to and from the raw A2A protocol types.

| Radkit Type                | A2A Protocol Type                |
| -------------------------- | -------------------------------- |
| `Content`                  | `Message` with `Part[]`          |
| `Artifact::from_json()`    | `Artifact` with `DataPart`       |
| `OnRequestResult::Completed` | `Task` with `state=completed`    |
| `OnRequestResult::InputRequired` | `Task` with `state=input-required` |

**Guarantee:** You never have to work with the low-level A2A protocol types directly. This eliminates a whole class of potential bugs related to serialization, deserialization, and protocol versioning. You work with ergonomic Rust types, and Radkit handles the correct protocol mapping behind the scenes.

By leveraging these compile-time checks and abstractions, Radkit allows you to focus on your agent's business logic with confidence, knowing that the underlying A2A compliance is handled for you.