---
title: Multi-turn Conversations
description: Implement agents that ask follow-up questions to gather information across multiple turns.
---



Not all tasks can be completed in a single step. Often, an agent needs to ask follow-up questions to gather all the necessary information. Radkit has first-class support for these multi-turn conversations.

The flow is as follows:
1.  The `on_request` handler determines that information is missing.
2.  It saves its partial work to the `TaskContext` and returns an `OnRequestResult::InputRequired` variant.
3.  Radkit sends a message to the user asking for the missing information.
4.  The user responds.
5.  Radkit calls the `on_input_received` handler on your skill with the user's new input.
6.  The `on_input_received` handler loads the partial state and continues the work.

## Requesting Input

If your `on_request` handler can't complete its work, it should return `OnRequestResult::InputRequired`.

This result contains two important fields:
-   `message`: The question to ask the user.
-   `slot`: A `SkillSlot` that you define, which acts as a state machine to track *what* you are asking for.

### Using Slots

A "slot" is a piece of information you're trying to fill. You should define an `enum` for your skill that represents the different pieces of information you might need to ask for.

```rust
use serde::{Deserialize, Serialize};

// This enum defines the different states of our conversation.
// We can be waiting for an email, a phone number, or a department.
#[derive(Serialize, Deserialize)]
enum ProfileSlot {
    Email,
    PhoneNumber,
    Department,
}
```

### Returning `InputRequired`

Inside `on_request`, if you find that the email is missing, you save the work you've done so far and return `InputRequired`, specifying the `ProfileSlot::Email` slot.

```rust
// In SkillHandler::on_request...

// ... after extracting a partial profile ...

if profile.email.is_empty() {
    // 1. Save the partial data to the task context.
    task_context.save_data("partial_profile", &profile)?;

    // 2. Return 'InputRequired' to ask the user for the email.
    return Ok(OnRequestResult::InputRequired {
        message: Content::from_text("I have the name and role, but I'm missing the email. What is it?"),
        // 3. Specify which piece of info you're waiting for.
        slot: SkillSlot::new(ProfileSlot::Email),
    });
}
```

## Handling User Input

When the user responds to your question, Radkit will call the `on_input_received` method on your `SkillHandler`. You must override the default implementation of this method to handle the input.

### The `on_input_received` Handler

This handler's job is to continue the work. It receives the user's new input and has access to the same `TaskContext` where you saved your partial state.

```rust
// In the `impl SkillHandler for ProfileExtractorSkill` block...

async fn on_input_received(
    &self,
    task_context: &mut TaskContext,
    context: &Context,
    runtime: &dyn Runtime,
    content: Content, // This is the user's answer
) -> Result<OnInputResult> {
    // 1. Find out what we were waiting for by loading the slot.
    let slot: ProfileSlot = task_context
        .load_slot()?
        .ok_or_else(|| anyhow!("Input received without a slot"))?;

    // 2. Load the saved state.
    let mut profile: UserProfile = task_context
        .load_data("partial_profile")?
        .ok_or_else(|| anyhow!("No partial profile found"))?;

    // 3. Handle the input based on the slot.
    match slot {
        ProfileSlot::Email => {
            profile.email = content.first_text().unwrap_or_default().to_string();
            
            // Now that we have the email, maybe we need the phone number?
            // You can chain input requests!
            if profile.phone_number.is_empty() {
                task_context.save_data("partial_profile", &profile)?;
                return Ok(OnInputResult::InputRequired {
                    message: Content::from_text("Thanks! What's the phone number?"),
                    slot: SkillSlot::new(ProfileSlot::PhoneNumber),
                });
            }
        }
        ProfileSlot::PhoneNumber => {
            profile.phone_number = content.first_text().unwrap_or_default().to_string();
            // ... and so on
        }
        ProfileSlot::Department => { /* ... */ }
    }

    // 4. Once all information is gathered, complete the task.
    let artifact = Artifact::from_json("user_profile.json", &profile)?;
    Ok(OnInputResult::Completed {
        message: Some(Content::from_text("Profile complete!")),
        artifacts: vec![artifact],
    })
}
```

The `on_input_received` handler can also return `InputRequired`, allowing you to chain questions until you have all the information needed to complete the task. This state machine, managed via the `SkillSlot`, is the key to building robust, multi-turn conversational agents.