//! Step dispatch — routes PipelineStep variants to their handlers

pub mod coding_agent;
pub mod command;
pub mod condition;
pub mod emit;
pub mod llm;
pub mod loop_step;
pub mod parallel;
pub mod sentinel;
pub mod shell;
pub mod watch;

use futures::future::BoxFuture;
use futures::FutureExt;

use super::types::{ExecutionContext, PipelineContext, PipelineStep, StepResult};

/// Execute a single pipeline step, dispatching to the appropriate handler.
/// Returns BoxFuture to handle recursive steps (condition, loop, parallel, sentinel).
pub fn execute_step<'a>(
    step: &'a PipelineStep,
    index: usize,
    ctx: &'a mut ExecutionContext,
    pipeline_ctx: &'a PipelineContext<'a>,
) -> BoxFuture<'a, Result<StepResult, String>> {
    async move {
        match step {
            PipelineStep::Shell {
                cmd,
                args,
                timeout_secs,
                working_dir,
                allow_failure,
                env,
            } => {
                shell::execute(
                    cmd,
                    args,
                    timeout_secs.unwrap_or(300),
                    working_dir.as_ref(),
                    allow_failure.unwrap_or(false),
                    index,
                    ctx,
                    pipeline_ctx,
                    env.as_ref(),
                )
                .await
            }
            PipelineStep::Llm {
                prompt,
                model,
                provider,
                max_tokens,
                temperature,
                system_prompt,
                tools,
                agent_mode,
                max_iterations,
                active_adapters,
            } => {
                llm::execute(
                    llm::LlmStepParams {
                        prompt,
                        model: model.as_deref(),
                        provider: provider.as_deref(),
                        max_tokens: *max_tokens,
                        temperature: *temperature,
                        system_prompt: system_prompt.as_deref(),
                        tools: tools.as_ref(),
                        agent_mode: *agent_mode,
                        max_iterations: *max_iterations,
                        active_adapters: active_adapters.as_ref(),
                    },
                    index,
                    ctx,
                    pipeline_ctx,
                )
                .await
            }
            PipelineStep::Command { command, params } => {
                command::execute(command, params, index, ctx, pipeline_ctx).await
            }
            PipelineStep::Condition {
                condition,
                then_steps,
                else_steps,
            } => {
                condition::execute(condition, then_steps, else_steps, index, ctx, pipeline_ctx)
                    .await
            }
            PipelineStep::Loop {
                count,
                steps,
                while_condition,
                until,
                max_iterations,
            } => {
                loop_step::execute(
                    *count,
                    while_condition.as_deref(),
                    until.as_deref(),
                    *max_iterations,
                    steps,
                    index,
                    ctx,
                    pipeline_ctx,
                )
                .await
            }
            PipelineStep::Parallel {
                branches,
                fail_fast,
            } => parallel::execute(branches, *fail_fast, index, ctx, pipeline_ctx).await,
            PipelineStep::Emit { event, payload } => {
                emit::execute(event, payload, index, ctx, pipeline_ctx).await
            }
            PipelineStep::Watch {
                event,
                timeout_secs,
            } => watch::execute(event, *timeout_secs, index, ctx, pipeline_ctx).await,
            PipelineStep::Sentinel { pipeline } => {
                sentinel::execute(pipeline, index, ctx, pipeline_ctx).await
            }
            PipelineStep::CodingAgent {
                prompt,
                provider,
                working_dir,
                system_prompt,
                model,
                allowed_tools,
                max_turns,
                max_budget_usd,
                permission_mode,
                resume_session_id,
                capture_training,
                persona_id,
                repo_path,
                task_slug,
            } => {
                coding_agent::execute(
                    prompt,
                    provider.as_deref(),
                    working_dir.as_deref(),
                    system_prompt.as_deref(),
                    model.as_deref(),
                    allowed_tools.as_ref(),
                    *max_turns,
                    *max_budget_usd,
                    permission_mode.as_deref(),
                    resume_session_id.as_deref(),
                    *capture_training,
                    persona_id.as_deref(),
                    repo_path.as_deref(),
                    task_slug.as_deref(),
                    index,
                    ctx,
                    pipeline_ctx,
                )
                .await
            }
        }
    }
    .boxed()
}
