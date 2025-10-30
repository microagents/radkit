//! Comprehensive unit tests for radkit models module.

mod common;

use radkit::models::{
    BaseLlm, BaseLlmExt, Content, ContentPart, Data, DataSource, Event, LlmResponse, Role, Thread,
    TokenUsage,
};
use radkit::tools::{ToolCall, ToolResponse, ToolResult};
use serde_json::json;

// ============================================================================
// Thread Tests
// ============================================================================

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_thread_new() {
    let events = vec![Event::user("Hello"), Event::assistant("Hi there")];
    let thread = Thread::new(events.clone());

    assert_eq!(thread.events().len(), 2);
    assert!(thread.system().is_none());
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_thread_from_system() {
    let thread = Thread::from_system("You are a helpful assistant");

    assert_eq!(thread.system(), Some("You are a helpful assistant"));
    assert_eq!(thread.events().len(), 0);
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_thread_from_user() {
    let thread = Thread::from_user("What is Rust?");

    assert_eq!(thread.events().len(), 1);
    assert!(matches!(thread.events()[0].role(), Role::User));
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_thread_with_system() {
    let thread = Thread::new(vec![])
        .with_system("You are an expert")
        .add_event(Event::user("Hello"));

    assert_eq!(thread.system(), Some("You are an expert"));
    assert_eq!(thread.events().len(), 1);
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_thread_add_event() {
    let thread = Thread::new(vec![])
        .add_event(Event::user("First"))
        .add_event(Event::assistant("Second"));

    assert_eq!(thread.events().len(), 2);
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_thread_add_events() {
    let events = vec![Event::user("One"), Event::user("Two")];
    let thread = Thread::new(vec![]).add_events(events);

    assert_eq!(thread.events().len(), 2);
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_thread_into_parts() {
    let thread = Thread::from_system("System").add_event(Event::user("User"));

    let (system, events) = thread.into_parts();

    assert_eq!(system, Some("System".to_string()));
    assert_eq!(events.len(), 1);
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_thread_into_events() {
    let thread = Thread::new(vec![Event::user("Test")]);
    let events = thread.into_events();

    assert_eq!(events.len(), 1);
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_thread_to_prompt() {
    let thread = Thread::from_system("You are helpful")
        .add_event(Event::user("Hello"))
        .add_event(Event::assistant("Hi"));

    let prompt = thread.to_prompt();

    assert!(prompt.contains("<system>"));
    assert!(prompt.contains("You are helpful"));
    assert!(prompt.contains("<User>"));
    assert!(prompt.contains("<Assistant>"));
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_thread_from_string() {
    let thread: Thread = "Hello".into();
    assert_eq!(thread.events().len(), 1);

    let thread: Thread = String::from("World").into();
    assert_eq!(thread.events().len(), 1);

    let s = String::from("Test");
    let thread: Thread = (&s).into();
    assert_eq!(thread.events().len(), 1);
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_thread_from_content() {
    let content = Content::from_text("Test");
    let thread: Thread = content.into();

    assert_eq!(thread.events().len(), 1);
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_thread_from_event() {
    let event = Event::user("Test");
    let thread: Thread = event.into();

    assert_eq!(thread.events().len(), 1);
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_thread_from_vec_events() {
    let events = vec![Event::user("One"), Event::user("Two")];
    let thread: Thread = events.into();

    assert_eq!(thread.events().len(), 2);
}

// ============================================================================
// Event Tests
// ============================================================================

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_event_system() {
    let event = Event::system("System prompt");

    assert!(matches!(event.role(), Role::System));
    assert_eq!(event.content().first_text(), Some("System prompt"));
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_event_user() {
    let event = Event::user("User message");

    assert!(matches!(event.role(), Role::User));
    assert_eq!(event.content().first_text(), Some("User message"));
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_event_assistant() {
    let event = Event::assistant("Assistant response");

    assert!(matches!(event.role(), Role::Assistant));
    assert_eq!(event.content().first_text(), Some("Assistant response"));
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_event_into_content() {
    let event = Event::user("Test");
    let content = event.into_content();

    assert_eq!(content.first_text(), Some("Test"));
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_event_into_parts() {
    let event = Event::assistant("Response");
    let (role, content) = event.into_parts();

    assert!(matches!(role, Role::Assistant));
    assert_eq!(content.first_text(), Some("Response"));
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_event_from_tool_calls() {
    let tool_calls = vec![ToolCall::new("id1", "tool_name", json!({}))];
    let event = Event::from(tool_calls);

    assert!(matches!(event.role(), Role::Assistant));
    assert!(event.content().has_tool_calls());
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_event_from_tool_response() {
    let response = ToolResponse::new("call_id".to_string(), ToolResult::success(json!({})));
    let event = Event::from(response);

    assert!(matches!(event.role(), Role::Tool));
    assert!(event.content().has_tool_responses());
}

// ============================================================================
// Content Tests
// ============================================================================

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_content_from_text() {
    let content = Content::from_text("Hello");

    assert_eq!(content.first_text(), Some("Hello"));
    assert!(content.is_text_only());
    assert_eq!(content.len(), 1);
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_content_from_parts() {
    let parts = vec![
        ContentPart::Text("Part 1".to_string()),
        ContentPart::Text("Part 2".to_string()),
    ];
    let content = Content::from_parts(parts);

    assert_eq!(content.len(), 2);
    assert_eq!(content.texts().len(), 2);
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_content_from_tool_calls() {
    let tool_calls = vec![
        ToolCall::new("id1", "tool1", json!({})),
        ToolCall::new("id2", "tool2", json!({})),
    ];
    let content = Content::from_tool_calls(tool_calls);

    assert_eq!(content.len(), 2);
    assert!(content.has_tool_calls());
    assert_eq!(content.tool_calls().len(), 2);
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_content_append() {
    let content = Content::from_text("First").append(ContentPart::Text("Second".to_string()));

    assert_eq!(content.len(), 2);
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_content_push() {
    let mut content = Content::from_text("First");
    content.push(ContentPart::Text("Second".to_string()));

    assert_eq!(content.len(), 2);
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_content_extended() {
    let content =
        Content::from_text("First").extended(vec![ContentPart::Text("Second".to_string())]);

    assert_eq!(content.len(), 2);
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_content_parts() {
    let content = Content::from_text("Test");
    let parts = content.parts();

    assert_eq!(parts.len(), 1);
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_content_into_parts() {
    let content = Content::from_text("Test");
    let parts = content.into_parts();

    assert_eq!(parts.len(), 1);
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_content_texts() {
    let content = Content::from_parts(vec![
        ContentPart::Text("First".to_string()),
        ContentPart::Text("Second".to_string()),
    ]);

    let texts = content.texts();
    assert_eq!(texts.len(), 2);
    assert_eq!(texts[0], "First");
    assert_eq!(texts[1], "Second");
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_content_into_texts() {
    let content = Content::from_parts(vec![
        ContentPart::Text("First".to_string()),
        ContentPart::Text("Second".to_string()),
    ]);

    let texts = content.into_texts();
    assert_eq!(texts.len(), 2);
    assert_eq!(texts[0], "First");
    assert_eq!(texts[1], "Second");
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_content_first_text() {
    let content = Content::from_parts(vec![
        ContentPart::Text("First".to_string()),
        ContentPart::Text("Second".to_string()),
    ]);

    assert_eq!(content.first_text(), Some("First"));
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_content_into_first_text() {
    let content = Content::from_text("Only");

    assert_eq!(content.into_first_text(), Some("Only".to_string()));
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_content_joined_texts() {
    let content = Content::from_parts(vec![
        ContentPart::Text("First".to_string()),
        ContentPart::Text("Second".to_string()),
    ]);

    let joined = content.joined_texts();
    assert!(joined.is_some());
    let joined_str = joined.unwrap();
    assert!(joined_str.contains("First"));
    assert!(joined_str.contains("Second"));
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_content_into_joined_texts() {
    let content = Content::from_parts(vec![
        ContentPart::Text("First".to_string()),
        ContentPart::Text("Second".to_string()),
    ]);

    let joined = content.into_joined_texts();
    assert!(joined.is_some());
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_content_is_empty() {
    let content = Content::default();
    assert!(content.is_empty());

    let content = Content::from_text("Not empty");
    assert!(!content.is_empty());
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_content_len() {
    let content = Content::default();
    assert_eq!(content.len(), 0);

    let content = Content::from_text("Test");
    assert_eq!(content.len(), 1);
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_content_is_text_empty() {
    let content = Content::from_text("");
    assert!(content.is_text_empty());

    let content = Content::from_text("   ");
    assert!(content.is_text_empty());

    let content = Content::from_text("Not empty");
    assert!(!content.is_text_empty());
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_content_is_text_only() {
    let content = Content::from_text("Text");
    assert!(content.is_text_only());

    let tool_call = ToolCall::new("id", "tool", json!({}));
    let content = Content::from_parts(vec![ContentPart::ToolCall(tool_call)]);
    assert!(!content.is_text_only());
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_content_has_text() {
    let content = Content::from_text("Text");
    assert!(content.has_text());

    let content = Content::default();
    assert!(!content.has_text());
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_content_has_tool_calls() {
    let tool_call = ToolCall::new("id", "tool", json!({}));
    let content = Content::from_parts(vec![ContentPart::ToolCall(tool_call)]);
    assert!(content.has_tool_calls());

    let content = Content::from_text("Text");
    assert!(!content.has_tool_calls());
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_content_has_tool_responses() {
    let response = ToolResponse::new("call_id".to_string(), ToolResult::success(json!({})));
    let content = Content::from_parts(vec![ContentPart::ToolResponse(response)]);
    assert!(content.has_tool_responses());

    let content = Content::from_text("Text");
    assert!(!content.has_tool_responses());
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_content_from_string() {
    let content: Content = "Hello".into();
    assert_eq!(content.first_text(), Some("Hello"));

    let content: Content = String::from("World").into();
    assert_eq!(content.first_text(), Some("World"));

    let s = String::from("Test");
    let content: Content = (&s).into();
    assert_eq!(content.first_text(), Some("Test"));
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_content_from_tool_response() {
    let response = ToolResponse::new("id".to_string(), ToolResult::success(json!({})));
    let content: Content = response.into();

    assert!(content.has_tool_responses());
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_content_from_vec_tool_calls() {
    let tool_calls = vec![ToolCall::new("id", "tool", json!({}))];
    let content: Content = tool_calls.into();

    assert!(content.has_tool_calls());
}

// ============================================================================
// ContentPart Tests
// ============================================================================

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_content_part_text() {
    let part = ContentPart::Text("Hello".to_string());

    assert_eq!(part.as_text(), Some("Hello"));
    assert!(part.as_tool_call().is_none());
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_content_part_data_creation() {
    let result = ContentPart::from_base64("image/png", "base64data", None);
    assert!(result.is_ok());

    let part = result.unwrap();
    assert!(part.as_data().is_some());
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_content_part_data_invalid_content_type() {
    let result = ContentPart::from_base64("", "base64data", None);
    assert!(result.is_err());
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_content_part_data_invalid_base64() {
    let result = ContentPart::from_base64("image/png", "", None);
    assert!(result.is_err());
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_content_part_from_uri() {
    let result = ContentPart::from_uri("image/png", "https://example.com/image.png", None);
    assert!(result.is_ok());

    let part = result.unwrap();
    assert!(part.as_data().is_some());

    let data = part.as_data().unwrap();
    match &data.source {
        DataSource::Uri(uri) => assert_eq!(uri, "https://example.com/image.png"),
        _ => panic!("Expected Uri source"),
    }
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_content_part_from_uri_invalid() {
    let result = ContentPart::from_uri("image/png", "", None);
    assert!(result.is_err());
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_content_part_tool_call() {
    let tool_call = ToolCall::new("id", "name", json!({"arg": "value"}));
    let part = ContentPart::ToolCall(tool_call.clone());

    assert!(part.as_tool_call().is_some());
    assert_eq!(part.as_tool_call().unwrap().name(), "name");
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_content_part_tool_response() {
    let response = ToolResponse::new("call_id".to_string(), ToolResult::success(json!({})));
    let part = ContentPart::ToolResponse(response);

    assert!(part.as_tool_response().is_some());
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_content_part_into_text() {
    let part = ContentPart::Text("Test".to_string());
    let text = part.into_text();

    assert_eq!(text, Some("Test".to_string()));
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_content_part_into_tool_call() {
    let tool_call = ToolCall::new("id", "name", json!({}));
    let part = ContentPart::ToolCall(tool_call.clone());
    let extracted = part.into_tool_call();

    assert!(extracted.is_some());
}

// ============================================================================
// LlmResponse Tests
// ============================================================================

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_llm_response_new() {
    let content = Content::from_text("Response");
    let usage = TokenUsage::new(10, 20, 30);
    let response = LlmResponse::new(content.clone(), usage.clone());

    assert_eq!(response.content().first_text(), Some("Response"));
    assert_eq!(response.usage().total_tokens(), 30);
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_llm_response_content() {
    let content = Content::from_text("Test");
    let usage = TokenUsage::default();
    let response = LlmResponse::new(content, usage);

    assert_eq!(response.content().first_text(), Some("Test"));
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_llm_response_usage() {
    let content = Content::from_text("Test");
    let usage = TokenUsage::new(5, 10, 15);
    let response = LlmResponse::new(content, usage);

    assert_eq!(response.usage().input_tokens(), 5);
    assert_eq!(response.usage().output_tokens(), 10);
    assert_eq!(response.usage().total_tokens(), 15);
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_llm_response_into_content() {
    let content = Content::from_text("Content");
    let usage = TokenUsage::default();
    let response = LlmResponse::new(content, usage);

    let extracted = response.into_content();
    assert_eq!(extracted.first_text(), Some("Content"));
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_llm_response_into_parts() {
    let content = Content::from_text("Content");
    let usage = TokenUsage::new(1, 2, 3);
    let response = LlmResponse::new(content, usage);

    let (extracted_content, extracted_usage) = response.into_parts();
    assert_eq!(extracted_content.first_text(), Some("Content"));
    assert_eq!(extracted_usage.total_tokens(), 3);
}

// ============================================================================
// TokenUsage Tests
// ============================================================================

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_token_usage_new() {
    let usage = TokenUsage::new(100, 200, 300);

    assert_eq!(usage.input_tokens(), 100);
    assert_eq!(usage.output_tokens(), 200);
    assert_eq!(usage.total_tokens(), 300);
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_token_usage_empty() {
    let usage = TokenUsage::empty();

    assert_eq!(usage.input_tokens(), 0);
    assert_eq!(usage.output_tokens(), 0);
    assert_eq!(usage.total_tokens(), 0);
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_token_usage_partial() {
    let usage = TokenUsage::partial(Some(50), None, Some(100));

    assert_eq!(usage.input_tokens(), 50);
    assert_eq!(usage.output_tokens(), 0); // Should default to 0
    assert_eq!(usage.total_tokens(), 100);
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_token_usage_opt_methods() {
    let usage = TokenUsage::partial(Some(10), None, Some(10));

    assert_eq!(usage.input_tokens_opt(), Some(10));
    assert_eq!(usage.output_tokens_opt(), None);
    assert_eq!(usage.total_tokens_opt(), Some(10));
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_token_usage_builder_methods() {
    let usage = TokenUsage::empty()
        .with_input_tokens(50)
        .with_output_tokens(100)
        .with_total_tokens(150);

    assert_eq!(usage.input_tokens(), 50);
    assert_eq!(usage.output_tokens(), 100);
    assert_eq!(usage.total_tokens(), 150);
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_token_usage_default() {
    let usage = TokenUsage::default();

    assert_eq!(usage.input_tokens(), 0);
    assert_eq!(usage.output_tokens(), 0);
    assert_eq!(usage.total_tokens(), 0);
}

// ============================================================================
// BaseLlm Tests (using mock)
// ============================================================================

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_base_llm_model_name() {
    let mock = common::MockLlm::new("test-model");
    assert_eq!(mock.model_name(), "test-model");
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_base_llm_generate_content() {
    let mock = common::MockLlm::new("test-model");
    let thread = Thread::from_user("Hello");

    let response = mock.generate_content(thread, None).await;
    assert!(response.is_ok());

    let response = response.unwrap();
    assert_eq!(response.content().first_text(), Some("Mock response"));
    assert_eq!(mock.call_count().await, 1);
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_base_llm_ext_generate() {
    let mock = common::MockLlm::new("test-model");

    // Test with &str
    let response = mock.generate("Hello", None).await;
    assert!(response.is_ok());

    // Test with String
    let response = mock.generate(String::from("World"), None).await;
    assert!(response.is_ok());

    // Test with Thread
    let thread = Thread::from_user("Test");
    let response = mock.generate(thread, None).await;
    assert!(response.is_ok());

    assert_eq!(mock.call_count().await, 3);
}

// ============================================================================
// Role Tests
// ============================================================================

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_role_display() {
    assert_eq!(format!("{}", Role::System), "System");
    assert_eq!(format!("{}", Role::User), "User");
    assert_eq!(format!("{}", Role::Assistant), "Assistant");
    assert_eq!(format!("{}", Role::Tool), "Tool");
}

// ============================================================================
// Data Tests
// ============================================================================

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_data_new_valid() {
    let result = Data::new(
        "image/png",
        DataSource::Base64("validbase64".to_string()),
        None,
    );
    assert!(result.is_ok());

    let data = result.unwrap();
    assert_eq!(data.content_type, "image/png");
    match &data.source {
        DataSource::Base64(base64) => assert_eq!(base64, "validbase64"),
        _ => panic!("Expected Base64 source"),
    }
    assert!(data.name.is_none());
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_data_new_with_name() {
    let result = Data::new(
        "image/jpeg",
        DataSource::Base64("base64data".to_string()),
        Some("photo.jpg".to_string()),
    );
    assert!(result.is_ok());

    let data = result.unwrap();
    assert_eq!(data.name, Some("photo.jpg".to_string()));
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_data_new_empty_content_type() {
    let result = Data::new("", DataSource::Base64("base64".to_string()), None);
    assert!(result.is_err());
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_data_new_empty_base64() {
    let result = Data::new("image/png", DataSource::Base64("".to_string()), None);
    assert!(result.is_err());
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_data_new_with_uri() {
    let result = Data::new(
        "image/png",
        DataSource::Uri("https://example.com/image.png".to_string()),
        None,
    );
    assert!(result.is_ok());

    let data = result.unwrap();
    assert_eq!(data.content_type, "image/png");
    match &data.source {
        DataSource::Uri(uri) => assert_eq!(uri, "https://example.com/image.png"),
        _ => panic!("Expected Uri source"),
    }
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_data_new_empty_uri() {
    let result = Data::new("image/png", DataSource::Uri("".to_string()), None);
    assert!(result.is_err());
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_data_new_invalid_mime_type() {
    let result = Data::new(
        "invalid",
        DataSource::Base64("base64data".to_string()),
        None,
    );
    assert!(result.is_err());
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_data_new_invalid_base64_chars() {
    let result = Data::new(
        "image/png",
        DataSource::Base64("invalid!@#$%chars".to_string()),
        None,
    );
    assert!(result.is_err());
}
