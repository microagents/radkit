---
title: Threads
description: Learn how to manage conversation history and context with LLMs using Threads in Radkit.
---



A `Thread` represents the complete conversation history with an LLM. It's the primary way you'll manage context in Radkit. A thread contains the initial system prompt and a sequence of message exchanges.

## Creating a Thread

You can create a `Thread` in several ways.

### From a User Message

The simplest way is to start a conversation from a single user message.

```rust
use radkit::models::Thread;

let thread = Thread::from_user("Hello, world!");
```

### With a System Prompt

To guide the LLM's behavior, you can provide a system prompt.

```rust
use radkit::models::{Thread, Event};

let thread = Thread::from_system("You are a helpful coding assistant")
    .add_event(Event::user("Explain Rust ownership"));
```

### From a Full Conversation

You can also construct a `Thread` from a complete history of events.

```rust
use radkit::models::{Thread, Event};

let thread = Thread::new(vec![
    Event::user("What is 2+2?"),
    Event::assistant("2+2 equals 4."),
    Event::user("What about 3+3?"),
]);
```

## Type Conversions

For convenience, `Thread` implements `From` for several common types, allowing you to create threads ergonomically.

```rust
use radkit::models::{Thread, Event};

// From a string slice
let thread: Thread = "Hello".into();

// From a String
let thread: Thread = String::from("World").into();

// From a single Event
let thread: Thread = Event::user("Question").into();

// From a Vec<Event>
let thread: Thread = vec![
    Event::user("First"),
    Event::assistant("Response"),
].into();
```

This flexibility makes it easy to integrate `Thread` into your existing code and build conversation histories dynamically.