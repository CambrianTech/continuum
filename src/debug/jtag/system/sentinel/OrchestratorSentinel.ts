/**
 * OrchestratorSentinel - LLM-powered task planning and execution
 *
 * Uses a quality model (like ClawdeBot/MoltBot) to:
 * 1. Plan: Break down goals into tasks
 * 2. Execute: Dispatch to specialized sentinels
 * 3. Observe: Check results, interpret outputs
 * 4. Adjust: Modify plan based on failures
 * 5. Stop: Know when goal is achieved (base case)
 *
 * The LLM is the "brain", sentinels are the "hands"
 */

import * as fs from 'fs';
import * as path from 'path';
import { execSync } from 'child_process';
import { BuildSentinel } from './BuildSentinel';
import { VisualSentinel } from './VisualSentinel';
import { ModelConfig, ModelCapacity, ModelProvider, ModelInvoker } from './ModelProvider';
import type { OrchestrateSentinelDefinition } from './SentinelDefinition';

export interface OrchestratorConfig {
  workingDir: string;
  maxIterations?: number;    // Recursion limit

  // Model selection - flexible options
  model?: ModelConfig;       // Full config with capacity/provider/specific model
  capacity?: ModelCapacity;  // Shorthand: just specify power level
  provider?: ModelProvider;  // Shorthand: just specify provider
  modelName?: string;        // Shorthand: specific model string

  screenshotDir?: string;    // Where to save visual feedback
  onThought?: (thought: string) => void;
  onAction?: (action: string, result: string) => void;
  onScreenshot?: (path: string) => void;
}

export interface ExecutionContext {
  goal: string;
  iteration: number;
  history: HistoryEntry[];
  filesCreated: string[];
  filesModified: string[];
  errors: string[];
}

export interface HistoryEntry {
  thought: string;
  action: string;
  result: string;
  success: boolean;
}

type ActionType = 'write' | 'read' | 'build' | 'run' | 'screenshot' | 'done' | 'fail';

interface Action {
  type: ActionType;
  file?: string;
  content?: string;
  command?: string;
  reason?: string;
}

// Internal config with all required fields
interface InternalConfig {
  workingDir: string;
  maxIterations: number;
  screenshotDir: string;
  onThought: (thought: string) => void;
  onAction: (action: string, result: string) => void;
  onScreenshot: (path: string) => void;
}

export class OrchestratorSentinel {
  private config: InternalConfig;
  private invoker: ModelInvoker;
  private modelConfig: ModelConfig;
  private _goal?: string;

  constructor(config: OrchestratorConfig & { workingDir: string }) {
    // Apply defaults
    this.config = {
      workingDir: config.workingDir,
      maxIterations: config.maxIterations ?? 20,
      screenshotDir: config.screenshotDir ?? '/tmp/sentinel-screenshots',
      onThought: config.onThought ?? (() => {}),
      onAction: config.onAction ?? (() => {}),
      onScreenshot: config.onScreenshot ?? (() => {}),
    };

    // Build model config from various shorthand options
    this.modelConfig = config.model ?? {
      capacity: config.capacity ?? ModelCapacity.SMALL,
      provider: config.provider ?? ModelProvider.LOCAL,
      model: config.modelName,
      maxTokens: 2000,
    };

    this.invoker = new ModelInvoker(config.workingDir);
  }

  /**
   * Create an OrchestratorSentinel from a portable definition
   */
  static fromDefinition(
    def: OrchestrateSentinelDefinition,
    callbacks?: Pick<OrchestratorConfig, 'onThought' | 'onAction' | 'onScreenshot'>
  ): OrchestratorSentinel {
    const sentinel = new OrchestratorSentinel({
      workingDir: def.workingDir || process.cwd(),
      maxIterations: def.maxIterations,
      capacity: def.capacity,
      provider: def.provider,
      modelName: def.modelName,
      screenshotDir: def.screenshotDir,
      ...callbacks,
    });
    sentinel._goal = def.goal;
    return sentinel;
  }

  /**
   * Export to portable JSON definition
   */
  toDefinition(name?: string, goal?: string): OrchestrateSentinelDefinition {
    return {
      type: 'orchestrate',
      name: name || `orchestrate-${Date.now()}`,
      version: '1.0',
      goal: goal || this._goal || '',
      workingDir: this.config.workingDir,
      maxIterations: this.config.maxIterations,
      capacity: this.modelConfig.capacity,
      provider: this.modelConfig.provider,
      modelName: this.modelConfig.model,
      screenshotDir: this.config.screenshotDir,
      createdAt: new Date().toISOString(),
    };
  }

  /**
   * Run from definition (uses stored goal)
   */
  async runFromDefinition(): Promise<{ success: boolean; summary: string; context: ExecutionContext }> {
    if (!this._goal) {
      return {
        success: false,
        summary: 'No goal specified in definition',
        context: { goal: '', iteration: 0, history: [], filesCreated: [], filesModified: [], errors: ['No goal'] },
      };
    }
    return this.execute(this._goal);
  }

  /**
   * Take visual feedback screenshot of HTML files
   */
  private async captureVisualFeedback(context: ExecutionContext): Promise<string | undefined> {
    // Find HTML files created
    const htmlFiles = context.filesCreated.filter(f => f.endsWith('.html'));
    if (htmlFiles.length === 0) return undefined;

    const visualSentinel = new VisualSentinel({
      outputDir: this.config.screenshotDir,
    });

    // Screenshot the first HTML file (usually index.html)
    const htmlPath = path.resolve(this.config.workingDir, htmlFiles[0]);
    const screenshotName = `${path.basename(htmlFiles[0], '.html')}-preview.png`;

    const result = await visualSentinel.screenshotFile(htmlPath, screenshotName);
    if (result.success && result.imagePath) {
      this.config.onScreenshot(result.imagePath);
      return result.imagePath;
    }
    return undefined;
  }

  /**
   * Execute a high-level goal using LLM planning
   */
  async execute(goal: string): Promise<{ success: boolean; summary: string; context: ExecutionContext }> {
    const context: ExecutionContext = {
      goal,
      iteration: 0,
      history: [],
      filesCreated: [],
      filesModified: [],
      errors: [],
    };

    this.config.onThought(`Goal: ${goal}`);

    while (context.iteration < this.config.maxIterations) {
      context.iteration++;
      this.config.onThought(`\n--- Iteration ${context.iteration}/${this.config.maxIterations} ---`);

      // 1. Ask LLM what to do next
      const action = await this.think(context);
      this.config.onThought(`Thought: ${action.reason || 'No reason given'}`);

      // 2. Check for termination (base cases)
      if (action.type === 'done') {
        this.config.onThought(`Done: ${action.reason}`);
        return {
          success: true,
          summary: action.reason || 'Goal completed',
          context,
        };
      }

      if (action.type === 'fail') {
        this.config.onThought(`Failed: ${action.reason}`);
        return {
          success: false,
          summary: action.reason || 'Goal failed',
          context,
        };
      }

      // 3. Fix file path if needed (when LLM returns just filename without path)
      if (action.type === 'write' && action.file && !action.file.includes('/')) {
        const pathMatch = goal.match(/(?:at|to|in)\s+(\S+)/i);
        if (pathMatch) {
          const targetPath = pathMatch[1].replace(/\/+$/, '');
          // If target path ends with a filename, use it; otherwise append
          if (targetPath.includes('.')) {
            action.file = targetPath;
          } else {
            action.file = `${targetPath}/${action.file}`;
          }
        }
      }

      // 4. Execute the action
      const result = await this.act(action, context);
      this.config.onAction(`${action.type}: ${action.file || action.command || ''}`, result.output);

      // 5. Record in history
      context.history.push({
        thought: action.reason || '',
        action: `${action.type}: ${action.file || action.command || ''}`,
        result: result.output,
        success: result.success,
      });

      if (!result.success) {
        context.errors.push(result.output);
      }

      // 6. Auto-terminate: If we successfully created the target file, we're done
      if (result.success && action.type === 'write') {
        const pathMatch = goal.match(/(?:at|to|in)\s+(\S+\.\w+)/i);
        if (pathMatch && action.file?.endsWith(pathMatch[1].split('/').pop()!)) {
          this.config.onThought(`Auto-done: Target file created successfully`);
          // Capture visual feedback for HTML files
          const screenshot = await this.captureVisualFeedback(context);
          if (screenshot) {
            this.config.onThought(`Visual feedback: ${screenshot}`);
          }
          return {
            success: true,
            summary: `Created ${action.file}${screenshot ? ` (screenshot: ${screenshot})` : ''}`,
            context,
          };
        }
      }
    }

    // Max iterations reached
    return {
      success: false,
      summary: `Max iterations (${this.config.maxIterations}) reached`,
      context,
    };
  }

  /**
   * Ask LLM to decide the next action
   */
  private async think(context: ExecutionContext): Promise<Action> {
    const prompt = this.buildPrompt(context);

    try {
      const response = await this.callLLM(prompt);
      return this.parseAction(response);
    } catch (error: any) {
      // If LLM fails, return fail action
      return { type: 'fail', reason: `LLM error: ${error.message}` };
    }
  }

  /**
   * Build prompt for LLM
   */
  private buildPrompt(context: ExecutionContext): string {
    const historyStr = context.history.length > 0
      ? context.history.map((h, i) =>
          `${i + 1}. Action: ${h.action}\n   Result: ${h.success ? 'SUCCESS' : 'FAILED'} - ${h.result.slice(0, 200)}`
        ).join('\n')
      : 'No actions taken yet.';

    const filesStr = context.filesCreated.length > 0
      ? `Files created: ${context.filesCreated.join(', ')}`
      : 'No files created yet.';

    // Check if goal might be complete based on state
    const goalMightBeComplete = context.filesCreated.length > 0 && context.errors.length === 0;

    // Extract target path from goal if present
    const pathMatch = context.goal.match(/(?:at|to|in)\s+(\S+)/i);
    const targetPath = pathMatch ? pathMatch[1].replace(/\/+$/, '') : '';

    if (goalMightBeComplete) {
      return `DONE`;  // Force termination
    }

    // Simple direct prompt
    const filename = targetPath ? targetPath.split('/').pop() || 'index.html' : 'index.html';

    return `Create ${filename} for: ${context.goal}

Output the complete file content now:`;
  }

  /**
   * Call LLM using flexible model provider
   */
  private async callLLM(prompt: string): Promise<string> {
    const result = await this.invoker.generate(prompt, this.modelConfig);
    if (result.success && result.text) {
      return result.text;
    }
    throw new Error(result.error || 'No response from model');
  }

  /**
   * Parse LLM response into Action (simple text format)
   */
  private parseAction(response: string): Action {
    // First, check if response contains a markdown code block with WRITE/READ/etc
    const codeBlockMatch = response.match(/```[\w]*\n?([\s\S]*?)```/);
    let cleaned = codeBlockMatch ? codeBlockMatch[1].trim() : response.trim();

    // Remove common LLM preambles and markdown formatting
    cleaned = cleaned
      .replace(/^(Here is|Here's|I'd like to|I will|Let me|Let's|Okay,?|Sure,?|Alright,?|The following|This will)[^.]*\.\s*/i, '')
      .replace(/^(To create|To write|To build|For this task)[^.]*\.\s*/i, '')
      .replace(/\*\*/g, '')  // Remove bold markdown
      .replace(/`/g, '')     // Remove inline code markdown
      .trim();

    // If cleaned still has preamble, look for WRITE/READ/BUILD/RUN/DONE anywhere
    const commandMatch = cleaned.match(/^(WRITE|READ|BUILD|RUN|DONE|FAIL)\b/im);
    if (commandMatch && commandMatch.index && commandMatch.index > 0) {
      cleaned = cleaned.slice(commandMatch.index);
    }

    const lines = cleaned.split('\n');
    const firstLine = lines[0].trim().toUpperCase();

    // DONE reason
    if (firstLine.startsWith('DONE')) {
      return { type: 'done', reason: firstLine.slice(4).trim() || lines.slice(1).join(' ').trim() };
    }

    // FAIL reason
    if (firstLine.startsWith('FAIL')) {
      return { type: 'fail', reason: firstLine.slice(4).trim() || lines.slice(1).join(' ').trim() };
    }

    // BUILD
    if (firstLine === 'BUILD') {
      return { type: 'build', reason: 'Running build' };
    }

    // RUN command
    if (firstLine.startsWith('RUN ')) {
      return { type: 'run', command: lines[0].slice(4).trim(), reason: 'Running command' };
    }

    // READ file
    if (firstLine.startsWith('READ ')) {
      return { type: 'read', file: lines[0].slice(5).trim(), reason: 'Reading file' };
    }

    // WRITE formats:
    // Format 1: WRITE filename\n---content---\nEND
    // Format 2: WRITE\nfilename: X\ncontent:\n...
    // Format 3: WRITE filename\ncontent...
    if (firstLine === 'WRITE' || firstLine.startsWith('WRITE ')) {
      let file: string;
      let content: string;

      if (firstLine === 'WRITE') {
        // Format 2: WRITE on its own line, filename: on next line
        const filenameMatch = cleaned.match(/filename:\s*(.+)/i);
        const contentMatch = cleaned.match(/content:\s*([\s\S]*?)(?:END|$)/i);

        if (filenameMatch) {
          file = filenameMatch[1].trim();
          content = contentMatch ? contentMatch[1].trim() : lines.slice(2).join('\n').trim();
        } else {
          // Just take second line as filename, rest as content
          file = lines[1]?.trim() || 'output.txt';
          content = lines.slice(2).join('\n').trim();
        }
      } else {
        // Format 1/3: WRITE filename on same line
        file = lines[0].slice(6).trim();

        // Try multiple content formats:
        // 1. Markdown code block: ```...\ncontent\n```
        // 2. --- markers: ---\ncontent\nEND
        // 3. Raw: everything after first line

        const afterFirstLine = lines.slice(1).join('\n');

        // Check for markdown code block
        const codeBlockMatch = afterFirstLine.match(/```[\w]*\n?([\s\S]*?)```/);
        if (codeBlockMatch) {
          content = codeBlockMatch[1].trim();
        } else {
          // Check for --- markers
          const contentStart = afterFirstLine.indexOf('---');
          const contentEnd = afterFirstLine.lastIndexOf('END');

          if (contentStart !== -1 && contentEnd !== -1 && contentEnd > contentStart) {
            content = afterFirstLine.slice(contentStart + 3, contentEnd).trim();
          } else {
            // Raw: strip any leading/trailing markers
            content = afterFirstLine.replace(/^---\n?/, '').replace(/\n?END$/, '').trim();
          }
        }
      }

      // Clean up content - remove markdown code blocks if present
      content = content.replace(/^```\w*\n?/, '').replace(/\n?```$/, '').trim();

      return { type: 'write', file, content, reason: 'Writing file' };
    }

    // Raw content fallback: try to extract HTML from chatty response
    // Look for HTML in markdown code blocks first (model might be chatty)
    const htmlCodeBlock = response.match(/```html?\n?([\s\S]*?)```/i);
    if (htmlCodeBlock) {
      return {
        type: 'write',
        file: 'index.html',
        content: htmlCodeBlock[1].trim(),
        reason: 'Extracted HTML from code block',
      };
    }

    // Look for raw HTML content
    if (cleaned.includes('<!DOCTYPE') || cleaned.includes('<html') || cleaned.startsWith('<!') || cleaned.startsWith('<')) {
      // Looks like HTML - infer filename from context
      return {
        type: 'write',
        file: 'index.html',  // Will be resolved by caller with proper path
        content: cleaned.replace(/^---\s*/, '').replace(/\s*END$/, '').trim(),
        reason: 'Raw HTML content',
      };
    }

    // Try JSON as fallback
    const jsonMatch = response.match(/\{[\s\S]*\}/);
    if (jsonMatch) {
      try {
        const parsed = JSON.parse(jsonMatch[0]);
        return {
          type: parsed.type || 'fail',
          file: parsed.file,
          content: parsed.content,
          command: parsed.command,
          reason: parsed.reason || 'Parsed from JSON',
        };
      } catch (e) {
        // Continue to fallback
      }
    }

    // Fallback: if response contains "done" or "complete", treat as done
    const lower = response.toLowerCase();
    if (lower.includes('done') || lower.includes('complete') || lower.includes('finished')) {
      return { type: 'done', reason: response.slice(0, 100) };
    }

    return { type: 'fail', reason: `Could not parse response: ${response.slice(0, 100)}` };
  }

  /**
   * Execute an action
   */
  private async act(action: Action, context: ExecutionContext): Promise<{ success: boolean; output: string }> {
    switch (action.type) {
      case 'write':
        return this.actWrite(action, context);
      case 'read':
        return this.actRead(action);
      case 'build':
        return this.actBuild();
      case 'run':
        return this.actRun(action);
      case 'screenshot':
        return this.actScreenshot(action);
      default:
        return { success: false, output: `Unknown action: ${action.type}` };
    }
  }

  private async actWrite(action: Action, context: ExecutionContext): Promise<{ success: boolean; output: string }> {
    if (!action.file || !action.content) {
      return { success: false, output: 'Write needs file and content' };
    }

    try {
      const fullPath = path.resolve(this.config.workingDir, action.file);
      const dir = path.dirname(fullPath);

      if (!fs.existsSync(dir)) {
        fs.mkdirSync(dir, { recursive: true });
      }

      const existed = fs.existsSync(fullPath);
      fs.writeFileSync(fullPath, action.content);

      if (existed) {
        context.filesModified.push(action.file);
      } else {
        context.filesCreated.push(action.file);
      }

      return { success: true, output: `Wrote ${action.file} (${action.content.length} bytes)` };
    } catch (error: any) {
      return { success: false, output: `Write failed: ${error.message}` };
    }
  }

  private async actRead(action: Action): Promise<{ success: boolean; output: string }> {
    if (!action.file) {
      return { success: false, output: 'Read needs file' };
    }

    try {
      const fullPath = path.resolve(this.config.workingDir, action.file);
      const content = fs.readFileSync(fullPath, 'utf-8');
      return { success: true, output: content.slice(0, 1000) + (content.length > 1000 ? '...' : '') };
    } catch (error: any) {
      return { success: false, output: `Read failed: ${error.message}` };
    }
  }

  private async actBuild(): Promise<{ success: boolean; output: string }> {
    const sentinel = new BuildSentinel({
      command: 'npm run build:ts',
      workingDir: this.config.workingDir,
      maxAttempts: 3,
      canAutoFix: true,
    });

    const result = await sentinel.run();
    return {
      success: result.success,
      output: result.success
        ? `Build succeeded${result.attempts.length > 1 ? ` (${result.attempts.length} attempts, fixes: ${JSON.stringify(result.attempts.filter(a => a.fixApplied).map(a => a.fixApplied))})` : ''}`
        : `Build failed: ${result.escalationReason}`,
    };
  }

  private async actRun(action: Action): Promise<{ success: boolean; output: string }> {
    if (!action.command) {
      return { success: false, output: 'Run needs command' };
    }

    try {
      const output = execSync(action.command, {
        cwd: this.config.workingDir,
        encoding: 'utf-8',
        timeout: 30000,
      });
      return { success: true, output: output.slice(0, 500) };
    } catch (error: any) {
      return { success: false, output: `Run failed: ${error.message}` };
    }
  }

  private async actScreenshot(action: Action): Promise<{ success: boolean; output: string }> {
    // Use JTAG's screenshot command if available
    try {
      const output = execSync('./jtag screenshot --url="http://localhost:8080" --filename="sentinel-screenshot.png"', {
        cwd: this.config.workingDir,
        encoding: 'utf-8',
        timeout: 30000,
      });
      return { success: true, output: 'Screenshot taken: sentinel-screenshot.png' };
    } catch (error: any) {
      return { success: false, output: `Screenshot failed: ${error.message}` };
    }
  }
}

// Test function
export async function testOrchestrator() {
  const orchestrator = new OrchestratorSentinel({
    workingDir: '/Volumes/FlashGordon/cambrian/continuum/src/debug/jtag',
    maxIterations: 10,
    capacity: ModelCapacity.SMALL,
    provider: ModelProvider.LOCAL,
    onThought: (t) => console.log(`[THINK] ${t}`),
    onAction: (a, r) => console.log(`[ACT] ${a}\n[RESULT] ${r.slice(0, 200)}`),
  });

  console.log('\n=== OrchestratorSentinel Test ===\n');

  const result = await orchestrator.execute(
    'Create a simple "Hello World" HTML file at system/sentinel/olympics/hello/index.html'
  );

  console.log('\n=== Final Result ===');
  console.log(`Success: ${result.success}`);
  console.log(`Summary: ${result.summary}`);
  console.log(`Files created: ${result.context.filesCreated.join(', ') || 'none'}`);

  return result;
}
