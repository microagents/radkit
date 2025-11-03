---
title: Progress Updates
description: Provide real-time feedback for long-running tasks with streaming updates and partial artifacts.
---



For tasks that take more than a few seconds, it's important to provide feedback to the user that the agent is still working. The A2A protocol has built-in support for this via streaming updates, and Radkit makes it easy to send them.

You can send two types of updates from within your `SkillHandler`:
-   **Intermediate Updates**: A simple text message indicating the agent's current status (e.g., "Analyzing data...").
-   **Partial Artifacts**: A complete data artifact (like a JSON file) that represents a piece of the final result.

These are sent using methods on the `TaskContext`.

## Sending Intermediate Updates

To send a status update, call `task_context.send_intermediate_update()`. This will send an A2A `TaskStatusUpdateEvent` with the state `working`.

```rust
use radkit::prelude::*;

#[async_trait]
impl SkillHandler for ReportGeneratorSkill {
    async fn on_request(
        &self,
        task_context: &mut TaskContext,
        context: &Context,
        runtime: &dyn Runtime,
        content: Content,
    ) -> Result<OnRequestResult> {
        let llm = runtime.llm_provider().default_llm()?;

        // --- Send a status update ---
        task_context.send_intermediate_update("Starting report generation. This may take a moment...").await?;

        // ... long-running analysis ...
        task_context.send_intermediate_update("Data analysis complete. Generating charts...").await?;

        // ... more work ...
        task_context.send_intermediate_update("Charts generated. Compiling final report...").await?;

        // ... final steps ...

        Ok(OnRequestResult::Completed {
            message: Some(Content::from_text("Report complete!")),
            artifacts: vec![/* ... final artifact ... */],
        })
    }
}
```

## Sending Partial Artifacts

Sometimes, you want to show the user partial results as they are generated. For example, you might generate a data analysis section first, and then a set of charts. You can send these as partial artifacts using `task_context.send_partial_artifact()`.

This sends an A2A `TaskArtifactUpdateEvent`. The artifacts are not considered final until they are included in the `OnRequestResult::Completed` return value.

```rust
use radkit::prelude::*;

#[async_trait]
impl SkillHandler for ReportGeneratorSkill {
    async fn on_request(
        &self,
        task_context: &mut TaskContext,
        context: &Context,
        runtime: &dyn Runtime,
        content: Content,
    ) -> Result<OnRequestResult> {
        let llm = runtime.llm_provider().default_llm()?;

        task_context.send_intermediate_update("Analyzing data...").await?;
        
        // Step 1: Analyze data and send it as a partial artifact
        let analysis = analyze_data().with_llm(llm.clone()).run(content).await?;
        let analysis_artifact = Artifact::from_json("analysis.json", &analysis)?;
        task_context.send_partial_artifact(analysis_artifact).await?;

        task_context.send_intermediate_update("Generating visualizations...").await?;

        // Step 2: Generate charts and send them as another partial artifact
        let charts = generate_charts().with_llm(llm.clone()).run(&analysis).await?;
        let charts_artifact = Artifact::from_json("charts.json", &charts)?;
        task_context.send_partial_artifact(charts_artifact).await?;

        task_context.send_intermediate_update("Compiling final report...").await?;

        // Step 3: Compile the final report
        let final_report = compile_report().with_llm(llm).run(&analysis, &charts).await?;
        let final_artifact = Artifact::from_json("final_report.json", &final_report)?;

        // The final artifact is returned in the 'Completed' result
        Ok(OnRequestResult::Completed {
            message: Some(Content::from_text("Report complete!")),
            artifacts: vec![final_artifact],
        })
    }
}
```

By using progress updates and partial artifacts, you can build agents that feel responsive and transparent, even when performing complex, long-running tasks.