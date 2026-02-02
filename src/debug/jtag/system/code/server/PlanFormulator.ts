/**
 * PlanFormulator - LLM-powered task decomposition for coding tasks
 *
 * Takes a CodingTask + codebase context and produces a CodingPlan (DAG of steps).
 * Uses a reasoning-class model (via CodingModelSelector) to decompose the task
 * into concrete code/* command invocations.
 *
 * The LLM receives:
 * - Task description
 * - Available code/* tools with parameter schemas
 * - Codebase context (tree, relevant file contents)
 * - Constraints (max tool calls, max duration)
 *
 * The LLM returns a JSON CodingPlan that the CodeAgentOrchestrator executes.
 */

import type { CodingTask, CodingPlan, CodingStep, CodingAction, RiskLevel, SecurityTierLevel } from '../shared/CodingTypes';
import { CodingModelSelector } from './CodingModelSelector';
import { AIProviderDaemon } from '../../../daemons/ai-provider-daemon/shared/AIProviderDaemon';
import type { TextGenerationRequest, ChatMessage } from '../../../daemons/ai-provider-daemon/shared/AIProviderTypesV2';
import { Logger } from '../../core/logging/Logger';
import { riskToTier } from './SecurityTier';

const log = Logger.create('PlanFormulator', 'code');

/**
 * Available code/* tools for the LLM to plan with.
 * Each entry describes what the tool does and its parameters.
 */
const CODE_TOOL_SCHEMAS: readonly { name: string; description: string; params: string }[] = [
  {
    name: 'code/tree',
    description: 'List directory tree structure. Shows files and directories with sizes.',
    params: 'path?: string, maxDepth?: number, includeHidden?: boolean',
  },
  {
    name: 'code/search',
    description: 'Search for a regex pattern across workspace files.',
    params: 'pattern: string, fileGlob?: string, maxResults?: number',
  },
  {
    name: 'code/read',
    description: 'Read file contents. Can specify line range.',
    params: 'filePath: string, startLine?: number, endLine?: number',
  },
  {
    name: 'code/write',
    description: 'Create or overwrite a file. Records a ChangeNode for undo.',
    params: 'filePath: string, content: string, description?: string',
  },
  {
    name: 'code/edit',
    description: 'Edit a file using search-replace, line-range, insert-at, or append. Records a ChangeNode.',
    params: 'filePath: string, editMode: { type: "search_replace", search: string, replace: string, replaceAll?: boolean } | { type: "line_range", startLine: number, endLine: number, newContent: string } | { type: "insert_at", line: number, content: string } | { type: "append", content: string }, description?: string',
  },
  {
    name: 'code/diff',
    description: 'Preview an edit as unified diff without applying it.',
    params: 'filePath: string, editMode: (same as code/edit)',
  },
  {
    name: 'code/undo',
    description: 'Undo a specific change or the last N changes.',
    params: 'changeId?: string, count?: number',
  },
  {
    name: 'code/history',
    description: 'View change history for a file or workspace.',
    params: 'filePath?: string, limit?: number',
  },
] as const;

/** Valid actions the LLM can use in plan steps */
const VALID_ACTIONS: ReadonlySet<string> = new Set<CodingAction>([
  'discover', 'search', 'read', 'write', 'edit', 'diff', 'undo', 'verify', 'report',
]);

/** Map from action to the expected code/* command */
const ACTION_TO_COMMAND: Record<CodingAction, string> = {
  discover: 'code/tree',
  search: 'code/search',
  read: 'code/read',
  write: 'code/write',
  edit: 'code/edit',
  diff: 'code/diff',
  undo: 'code/undo',
  verify: 'code/read',   // Verify by reading back
  report: 'code/history',
};

export class PlanFormulator {
  private readonly modelSelector: CodingModelSelector;

  constructor(modelSelector: CodingModelSelector) {
    this.modelSelector = modelSelector;
  }

  /**
   * Generate a CodingPlan for a task.
   *
   * @param task - The coding task to plan
   * @param codebaseContext - Optional pre-fetched context (tree output, file contents)
   * @returns A validated CodingPlan ready for execution
   */
  async formulate(task: CodingTask, codebaseContext?: string): Promise<CodingPlan> {
    const startTime = performance.now();
    log.info(`Formulating plan for task: ${task.description.slice(0, 80)}...`);

    const tier = this.modelSelector.select('planning');
    const messages = this.buildPlanningPrompt(task, codebaseContext);

    const request: TextGenerationRequest = {
      messages,
      model: tier.model,
      temperature: tier.temperature,
      maxTokens: tier.maxTokens,
      preferredProvider: tier.provider,
      purpose: 'coding-plan',
      userId: task.personaId,
    };

    const response = await AIProviderDaemon.generateText(request);

    if (!response.text) {
      throw new Error('PlanFormulator: LLM returned empty response');
    }

    const plan = this.parsePlanResponse(response.text, task, tier.provider, tier.model);
    const durationMs = performance.now() - startTime;

    log.info(`Plan generated: ${plan.steps.length} steps, ${plan.estimatedToolCalls} tool calls (${Math.round(durationMs)}ms)`);
    return plan;
  }

  /**
   * Build the prompt messages for plan generation.
   */
  private buildPlanningPrompt(task: CodingTask, codebaseContext?: string): ChatMessage[] {
    const toolDocs = CODE_TOOL_SCHEMAS
      .map(t => `- **${t.name}**: ${t.description}\n  Params: ${t.params}`)
      .join('\n');

    const maxToolCalls = task.maxToolCalls ?? 15;
    const maxDurationSec = Math.round((task.maxDurationMs ?? 120000) / 1000);

    const systemPrompt = `You are a coding agent planner. Your job is to decompose a coding task into a concrete plan of steps.

## Available Tools
${toolDocs}

## Constraints
- Maximum ${maxToolCalls} tool calls total
- Maximum ${maxDurationSec} seconds execution time
- Always read files before editing them
- Always verify changes after editing (read back or diff)
- Prefer code/edit over code/write for existing files
- Use code/tree and code/search for discovery before making changes

## Output Format
Respond with ONLY a JSON object (no markdown, no explanation):
{
  "summary": "Brief description of the approach",
  "riskLevel": "low|medium|high|critical",
  "riskReason": "Why this risk level was assigned",
  "steps": [
    {
      "stepNumber": 1,
      "action": "discover|search|read|write|edit|diff|undo|verify|report",
      "description": "What this step does",
      "targetFiles": ["path/to/file.ts"],
      "toolCall": "code/tree",
      "toolParams": { "path": "src/" },
      "dependsOn": [],
      "verification": "How to verify success"
    }
  ]
}

## Risk Assessment Guidelines
- **low**: Read-only tasks, documentation, test-only changes, single-file edits
- **medium**: Multi-file edits, adding new functions, standard refactoring
- **high**: API/interface changes, security-sensitive code, cross-module refactoring
- **critical**: System configuration, build scripts, deployment, anything requiring shell execution

## Rules
1. Steps are numbered starting from 1
2. dependsOn lists step numbers that must complete first (DAG)
3. Independent steps CAN have the same dependsOn (parallel execution)
4. Every write/edit MUST have a preceding read of the same file
5. action must be one of: discover, search, read, write, edit, diff, undo, verify, report
6. toolCall must match a code/* command from the tools list
7. toolParams must match the command's parameter schema
8. Keep plans minimal — don't add unnecessary steps`;

    const messages: ChatMessage[] = [
      { role: 'system', content: systemPrompt },
    ];

    if (codebaseContext) {
      messages.push({
        role: 'user',
        content: `## Codebase Context\n${codebaseContext}`,
      });
    }

    if (task.relevantFiles && task.relevantFiles.length > 0) {
      messages.push({
        role: 'user',
        content: `## Relevant Files (hints)\n${task.relevantFiles.join('\n')}`,
      });
    }

    messages.push({
      role: 'user',
      content: `## Task\n${task.description}\n\nGenerate the execution plan as JSON.`,
    });

    return messages;
  }

  /**
   * Parse and validate the LLM's plan response.
   */
  private parsePlanResponse(
    responseText: string,
    task: CodingTask,
    provider: string,
    model: string,
  ): CodingPlan {
    // Extract JSON from response (LLM may wrap in markdown code blocks)
    const jsonMatch = responseText.match(/\{[\s\S]*\}/);
    if (!jsonMatch) {
      throw new Error('PlanFormulator: No JSON object found in LLM response');
    }

    let raw: unknown;
    try {
      raw = JSON.parse(jsonMatch[0]);
    } catch (e) {
      throw new Error(`PlanFormulator: Invalid JSON in LLM response: ${(e as Error).message}`);
    }

    const parsed = raw as { summary?: string; steps?: unknown[]; riskLevel?: string; riskReason?: string };

    if (!parsed.summary || typeof parsed.summary !== 'string') {
      throw new Error('PlanFormulator: Plan missing "summary" field');
    }

    // Extract and validate risk assessment
    const VALID_RISK_LEVELS: ReadonlySet<string> = new Set(['low', 'medium', 'high', 'critical']);
    const riskLevel: RiskLevel = VALID_RISK_LEVELS.has(parsed.riskLevel ?? '')
      ? (parsed.riskLevel as RiskLevel)
      : 'medium'; // Default to medium if LLM omits or gives invalid value
    const riskReason = typeof parsed.riskReason === 'string' ? parsed.riskReason : 'No risk reason provided';
    const requiredTier: SecurityTierLevel = riskToTier(riskLevel);

    if (!Array.isArray(parsed.steps) || parsed.steps.length === 0) {
      throw new Error('PlanFormulator: Plan has no steps');
    }

    const maxToolCalls = task.maxToolCalls ?? 15;
    if (parsed.steps.length > maxToolCalls) {
      throw new Error(`PlanFormulator: Plan has ${parsed.steps.length} steps, exceeds max ${maxToolCalls}`);
    }

    // Validate each step
    const steps: CodingStep[] = parsed.steps.map((rawStep, index) => {
      const step = rawStep as Record<string, unknown>;
      const stepNum = (step.stepNumber as number) ?? (index + 1);

      // Validate action
      const action = step.action as string;
      if (!VALID_ACTIONS.has(action)) {
        throw new Error(`PlanFormulator: Step ${stepNum} has invalid action "${action}"`);
      }

      // Validate toolCall
      const toolCall = (step.toolCall as string) ?? ACTION_TO_COMMAND[action as CodingAction];
      if (!toolCall.startsWith('code/')) {
        throw new Error(`PlanFormulator: Step ${stepNum} toolCall "${toolCall}" is not a code/* command`);
      }

      // Validate dependsOn references
      const dependsOn = (step.dependsOn as number[]) ?? [];
      for (const dep of dependsOn) {
        if (dep < 1 || dep >= stepNum) {
          throw new Error(`PlanFormulator: Step ${stepNum} depends on invalid step ${dep}`);
        }
      }

      return {
        stepNumber: stepNum,
        action: action as CodingAction,
        description: (step.description as string) ?? `Step ${stepNum}`,
        targetFiles: (step.targetFiles as string[]) ?? [],
        toolCall,
        toolParams: (step.toolParams as Record<string, unknown>) ?? {},
        dependsOn,
        verification: (step.verification as string) ?? '',
      };
    });

    return {
      taskId: task.id,
      steps,
      summary: parsed.summary,
      estimatedToolCalls: steps.length,
      generatedBy: { provider, model },
      generatedAt: Date.now(),
      riskLevel,
      riskReason,
      requiredTier,
    };
  }
}
