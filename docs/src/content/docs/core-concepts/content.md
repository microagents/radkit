---
title: Content
description: Working with multi-modal message content including text, images, documents, and tool calls.
---



`Content` represents the payload of a single message in a conversation. It's a powerful primitive that supports multi-modal messages, including text, images, documents, tool calls, and tool responses.

## Creating Content

### Simple Text

The most common type of content is simple text.

```rust
use radkit::models::Content;

// Create from a string slice
let content = Content::from_text("Hello!");

// Or from a String
let content = Content::from_text(String::from("Hello, world!"));
```

### Multi-Modal Content

For more complex messages, you can combine different types of `ContentPart`. This is how you would send an image to a vision-capable model.

```rust
use radkit::models::{Content, ContentPart};

let content = Content::from_parts(vec![
    ContentPart::Text("Check out this image:".to_string()),
    ContentPart::from_data(
        "image/png",
        "base64_encoded_image_data_here",
        Some("photo.png".to_string())
    ).unwrap(),
]);
```

:::note
`ContentPart::from_data` returns a `Result` because it validates the MIME type and data format.
:::

## Working with Content

Once you have a `Content` object, you can easily inspect and extract information from it.

### Accessing Text

You can get the first text part, all text parts, or a single joined string.

```rust
use radkit::models::Content;

let content = Content::from_text("First part. Second part.");

// Get the first text part
if content.has_text() {
    println!("First text: {}", content.first_text().unwrap());
}

// Iterate over all text parts
for text in content.texts() {
    println!("Part: {}", text);
}

// Get a single string with all text parts joined together
if let Some(combined) = content.joined_texts() {
    println!("Combined: {}", combined);
}
```

### Checking for Other Modalities

You can also check for the presence of other content types, like tool calls or images.

```rust
use radkit::models::Content;

let content = Content::from_text("Example"); // Assume this might have other parts

if content.has_tool_calls() {
    println!("Content has {} tool calls", content.tool_calls().len());
}

if content.has_images() {
    println!("Content has images");
}
```

The `Content` API provides a safe and convenient way to handle the potentially complex, multi-modal data that flows through an agent system.