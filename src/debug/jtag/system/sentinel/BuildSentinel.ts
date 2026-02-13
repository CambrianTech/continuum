/**
 * BuildSentinel - Agentic loop for compilation
 *
 * Like ClawdeBot/MoltBot but focused on ONE goal: code compiles.
 *
 * The LLM is the BRAIN that:
 * 1. Sees build errors
 * 2. Reasons about what's wrong
 * 3. Decides how to fix
 * 4. Applies the fix
 * 5. Retries and evaluates
 * 6. Knows when to escalate
 *
 * NOT pattern-matching with LLM fallback - LLM IS the reasoning engine.
 */

import { execSync, exec } from 'child_process';
import * as fs from 'fs';
import * as path from 'path';
import type { BuildSentinelDefinition } from './SentinelDefinition';
import { ModelConfig, ModelCapacity, ModelProvider, ModelInvoker } from './ModelProvider';

export interface BuildError {
  file: string;
  line: number;
  column: number;
  message: string;
  code?: string;
  raw: string;
}

export interface BuildAttempt {
  attemptNumber: number;
  command: string;
  success: boolean;
  errors: BuildError[];
  fixApplied?: string;
  durationMs: number;
}

export interface BuildResult {
  success: boolean;
  attempts: BuildAttempt[];
  finalErrors?: BuildError[];
  escalated?: boolean;
  escalationReason?: string;
}

export interface SentinelProgress {
  phase: 'building' | 'parsing' | 'fixing' | 'verifying' | 'done' | 'escalating';
  attempt: number;
  maxAttempts: number;
  message: string;
  errors?: BuildError[];
}

export interface BuildSentinelConfig {
  command: string;
  workingDir: string;
  maxAttempts?: number;
  timeoutMs?: number;
  canAutoFix?: boolean;
  onProgress?: (progress: SentinelProgress) => void;

  // LLM-assisted fixing (optional - enables smarter error recovery)
  useLLM?: boolean;                // Enable LLM for fixes pattern-matching can't handle
  model?: ModelConfig;             // Full model config
  capacity?: ModelCapacity;        // Shorthand: power level (SMALL for fast, MEDIUM for better)
  provider?: ModelProvider;        // Shorthand: LOCAL, OLLAMA, ANTHROPIC, etc.
  modelName?: string;              // Shorthand: specific model string
}

export class BuildSentinel {
  private config: Required<Omit<BuildSentinelConfig, 'model' | 'capacity' | 'provider' | 'modelName' | 'useLLM'>> & { useLLM: boolean };
  private attempts: BuildAttempt[] = [];
  private invoker?: ModelInvoker;
  private modelConfig?: ModelConfig;

  constructor(config: BuildSentinelConfig) {
    this.config = {
      maxAttempts: 3,
      timeoutMs: 120000,
      canAutoFix: true,
      onProgress: () => {},
      useLLM: config.useLLM ?? false,
      command: config.command,
      workingDir: config.workingDir,
    };

    // Setup LLM if enabled
    if (config.useLLM) {
      this.invoker = new ModelInvoker(config.workingDir);
      this.modelConfig = config.model ?? {
        capacity: config.capacity ?? ModelCapacity.SMALL,
        provider: config.provider ?? ModelProvider.LOCAL,
        model: config.modelName,
        maxTokens: 2000,
      };
    }
  }

  /**
   * Create a BuildSentinel from a portable definition
   */
  static fromDefinition(def: BuildSentinelDefinition, onProgress?: BuildSentinelConfig['onProgress']): BuildSentinel {
    return new BuildSentinel({
      command: def.command,
      workingDir: def.workingDir || process.cwd(),
      maxAttempts: def.maxAttempts,
      timeoutMs: def.timeout,
      canAutoFix: def.canAutoFix,
      useLLM: def.useLLM,
      capacity: def.llmCapacity,
      provider: def.llmProvider,
      onProgress,
    });
  }

  /**
   * Export to portable JSON definition
   */
  toDefinition(name?: string): BuildSentinelDefinition {
    return {
      type: 'build',
      name: name || `build-${Date.now()}`,
      version: '1.0',
      command: this.config.command,
      workingDir: this.config.workingDir,
      maxAttempts: this.config.maxAttempts,
      timeout: this.config.timeoutMs,
      canAutoFix: this.config.canAutoFix,
      useLLM: this.config.useLLM,
      llmCapacity: this.modelConfig?.capacity,
      llmProvider: this.modelConfig?.provider,
      createdAt: new Date().toISOString(),
    };
  }

  /**
   * Run the sentinel loop until success or escalation
   */
  async run(): Promise<BuildResult> {
    for (let attempt = 1; attempt <= this.config.maxAttempts; attempt++) {
      this.report('building', attempt, `Running build (attempt ${attempt}/${this.config.maxAttempts})`);

      const buildAttempt = await this.build(attempt);
      this.attempts.push(buildAttempt);

      if (buildAttempt.success) {
        this.report('done', attempt, 'Build succeeded!');
        return { success: true, attempts: this.attempts };
      }

      this.report('parsing', attempt, `Build failed with ${buildAttempt.errors.length} error(s)`, buildAttempt.errors);

      // Should we escalate?
      if (this.shouldEscalate(buildAttempt.errors)) {
        const reason = this.getEscalationReason(buildAttempt.errors);
        this.report('escalating', attempt, `Escalating: ${reason}`);
        return {
          success: false,
          attempts: this.attempts,
          finalErrors: buildAttempt.errors,
          escalated: true,
          escalationReason: reason,
        };
      }

      // Try to fix if we have attempts left
      if (attempt < this.config.maxAttempts && this.config.canAutoFix) {
        this.report('fixing', attempt, 'Attempting auto-fix...');
        const fixed = await this.attemptFix(buildAttempt.errors);
        if (fixed) {
          this.report('verifying', attempt, `Applied fix: ${fixed}`);
          buildAttempt.fixApplied = fixed;
        } else {
          this.report('fixing', attempt, 'Could not auto-fix, will retry...');
        }
      }
    }

    // Max attempts reached
    const lastAttempt = this.attempts[this.attempts.length - 1];
    return {
      success: false,
      attempts: this.attempts,
      finalErrors: lastAttempt.errors,
      escalated: true,
      escalationReason: `Max attempts (${this.config.maxAttempts}) reached`,
    };
  }

  /**
   * Execute a single build attempt
   */
  private async build(attemptNumber: number): Promise<BuildAttempt> {
    const startTime = Date.now();
    let output = '';
    let success = false;

    try {
      output = execSync(this.config.command, {
        cwd: this.config.workingDir,
        encoding: 'utf-8',
        timeout: this.config.timeoutMs,
        stdio: ['pipe', 'pipe', 'pipe'],
      });
      success = true;
    } catch (error: any) {
      output = error.stdout?.toString() || '';
      output += error.stderr?.toString() || '';
      success = false;
    }

    const errors = success ? [] : this.parseErrors(output);

    return {
      attemptNumber,
      command: this.config.command,
      success,
      errors,
      durationMs: Date.now() - startTime,
    };
  }

  /**
   * Parse build output for errors
   */
  private parseErrors(output: string): BuildError[] {
    const errors: BuildError[] = [];

    // TypeScript error pattern: src/file.ts(10,5): error TS2345: ...
    // Or: src/file.ts:10:5 - error TS2345: ...
    const tsPattern = /([^\s]+\.tsx?)[:\(](\d+)[,:](\d+)\)?[:\s-]+error\s+(TS\d+):\s*(.+)/g;
    let match;
    while ((match = tsPattern.exec(output)) !== null) {
      errors.push({
        file: match[1],
        line: parseInt(match[2], 10),
        column: parseInt(match[3], 10),
        code: match[4],
        message: match[5].trim(),
        raw: match[0],
      });
    }

    // Rust error pattern: error[E0425]: cannot find value `x`
    //   --> src/main.rs:10:5
    const rustErrorPattern = /error\[([^\]]+)\]:\s*(.+)\n\s*-->\s*([^:]+):(\d+):(\d+)/g;
    while ((match = rustErrorPattern.exec(output)) !== null) {
      errors.push({
        file: match[3],
        line: parseInt(match[4], 10),
        column: parseInt(match[5], 10),
        code: match[1],
        message: match[2].trim(),
        raw: match[0],
      });
    }

    // Generic fallback: look for "error:" patterns
    if (errors.length === 0 && output.toLowerCase().includes('error')) {
      const lines = output.split('\n');
      for (const line of lines) {
        if (line.toLowerCase().includes('error') && !line.includes('0 errors')) {
          errors.push({
            file: 'unknown',
            line: 0,
            column: 0,
            message: line.trim(),
            raw: line,
          });
        }
      }
    }

    return errors;
  }

  /**
   * Determine if we should escalate (give up) based on error types
   */
  private shouldEscalate(errors: BuildError[]): boolean {
    for (const error of errors) {
      // Architectural issues - need human/full AI
      if (error.message.includes('circular dependency')) return true;
      if (error.message.includes('Cannot find module') && error.message.includes('@')) return true;

      // Too many errors - probably something fundamentally wrong
      if (errors.length > 10) return true;
    }
    return false;
  }

  /**
   * Get human-readable reason for escalation
   */
  private getEscalationReason(errors: BuildError[]): string {
    if (errors.length > 10) {
      return `Too many errors (${errors.length}) - likely architectural issue`;
    }
    for (const error of errors) {
      if (error.message.includes('circular dependency')) {
        return 'Circular dependency detected - needs architectural fix';
      }
      if (error.message.includes('Cannot find module')) {
        return `Missing module: ${error.message} - may need npm install or path fix`;
      }
    }
    return 'Error type not auto-fixable';
  }

  /**
   * Attempt to automatically fix errors
   * Returns description of fix applied, or null if couldn't fix
   *
   * When useLLM=true: LLM IS the brain - it analyzes, reasons, decides
   * When useLLM=false: Fall back to pattern-based fixes (fast but limited)
   */
  private async attemptFix(errors: BuildError[]): Promise<string | null> {
    // When LLM is enabled, it's the primary reasoning engine (like ClawdeBot)
    if (this.config.useLLM && this.invoker && this.modelConfig) {
      return this.llmReasonAndFix(errors);
    }

    // Without LLM, try pattern-based fixes (limited but fast)
    for (const error of errors) {
      const fix = await this.tryFixError(error);
      if (fix) return fix;
    }

    return null;
  }

  /**
   * LLM-powered reasoning and fixing (the agentic core)
   *
   * The LLM is the BRAIN that:
   * 1. Sees the build errors with full code context
   * 2. Reasons about what's wrong (root cause analysis)
   * 3. Decides the appropriate action
   * 4. Provides a precise fix
   */
  private async llmReasonAndFix(errors: BuildError[]): Promise<string | null> {
    if (!this.invoker || !this.modelConfig) return null;

    // Build rich context for the LLM brain
    const errorContext = errors.slice(0, 5).map(e => {
      let context = `ERROR: ${e.file}:${e.line}:${e.column}`;
      if (e.code) context += ` [${e.code}]`;
      context += `\n${e.message}`;

      // Give the LLM the surrounding code context to reason about
      try {
        const fullPath = path.resolve(this.config.workingDir, e.file);
        if (fs.existsSync(fullPath)) {
          const content = fs.readFileSync(fullPath, 'utf-8');
          const lines = content.split('\n');
          const start = Math.max(0, e.line - 5);
          const end = Math.min(lines.length, e.line + 3);
          const snippet = lines.slice(start, end).map((l, i) => {
            const lineNum = start + i + 1;
            const marker = lineNum === e.line ? '>>> ' : '    ';
            return `${marker}${lineNum.toString().padStart(4)}: ${l}`;
          }).join('\n');
          context += `\n\nCode:\n\`\`\`typescript\n${snippet}\n\`\`\``;
        }
      } catch {
        // Ignore file read errors
      }

      return context;
    }).join('\n\n');

    // Previous attempts context (so LLM doesn't repeat failed fixes)
    const previousFixes = this.attempts
      .filter(a => a.fixApplied)
      .map(a => `- ${a.fixApplied}`)
      .join('\n');

    const prompt = `You are an expert build error analyst. Your job is to understand WHY the build failed and fix it.

BUILD COMMAND: ${this.config.command}
ATTEMPT: ${this.attempts.length + 1}/${this.config.maxAttempts}

${previousFixes ? `PREVIOUS FIXES TRIED:\n${previousFixes}\n\n` : ''}BUILD ERRORS:
${errorContext}

THINK step by step:
1. What is the ROOT CAUSE of this error?
2. What specific change will fix it?
3. Are there any related issues that might appear after this fix?

Then respond with EXACTLY ONE action:

ACTION: EDIT
FILE: <filepath>
LINE: <line number>
SEARCH: <exact text to find on that line>
REPLACE: <replacement text>

ACTION: INSERT
FILE: <filepath>
AFTER_LINE: <line number to insert after, or 0 for top>
CONTENT: <line to insert>

ACTION: ESCALATE
REASON: <why this needs human intervention>

Be surgical. Fix only what's broken.`;

    try {
      this.report('fixing', this.attempts.length + 1, 'LLM analyzing errors...');
      const result = await this.invoker.generate(prompt, this.modelConfig);

      if (!result.success || !result.text) {
        this.report('fixing', this.attempts.length + 1, 'LLM returned no response');
        return null;
      }

      return this.applyLLMAction(result.text);
    } catch (error: any) {
      this.report('fixing', this.attempts.length + 1, `LLM error: ${error.message}`);
      return null;
    }
  }

  /**
   * Parse and apply the LLM's decided action
   */
  private applyLLMAction(response: string): string | null {
    const lines = response.split('\n');

    // Find the ACTION line
    const actionLine = lines.find(l => l.trim().startsWith('ACTION:'));
    if (!actionLine) {
      // Try to find old format for backwards compatibility
      return this.applyLLMFix(response, { file: 'unknown', line: 0, column: 0, message: '', raw: '' });
    }

    const action = actionLine.replace('ACTION:', '').trim().toUpperCase();

    if (action === 'ESCALATE') {
      const reasonLine = lines.find(l => l.trim().startsWith('REASON:'));
      const reason = reasonLine?.replace('REASON:', '').trim() || 'LLM decided to escalate';
      this.report('escalating', this.attempts.length + 1, `LLM: ${reason}`);
      return null;
    }

    if (action === 'EDIT') {
      const fileLine = lines.find(l => l.trim().startsWith('FILE:'));
      const lineLine = lines.find(l => l.trim().startsWith('LINE:'));
      const searchLine = lines.find(l => l.trim().startsWith('SEARCH:'));
      const replaceLine = lines.find(l => l.trim().startsWith('REPLACE:'));

      if (fileLine && searchLine && replaceLine) {
        const file = fileLine.replace('FILE:', '').trim();
        const lineNum = lineLine ? parseInt(lineLine.replace('LINE:', '').trim(), 10) : 0;
        const search = searchLine.replace('SEARCH:', '').trim();
        const replace = replaceLine.replace('REPLACE:', '').trim();

        try {
          const fullPath = path.resolve(this.config.workingDir, file);
          const content = fs.readFileSync(fullPath, 'utf-8');
          const fileLines = content.split('\n');

          // Find the line with the search text (prefer specified line number)
          let targetIdx = lineNum > 0 ? lineNum - 1 : -1;
          if (targetIdx >= 0 && targetIdx < fileLines.length && fileLines[targetIdx].includes(search)) {
            // Found at expected line
          } else {
            // Search nearby lines
            for (let i = 0; i < fileLines.length; i++) {
              if (fileLines[i].includes(search)) {
                targetIdx = i;
                break;
              }
            }
          }

          if (targetIdx >= 0 && targetIdx < fileLines.length) {
            const originalLine = fileLines[targetIdx];
            fileLines[targetIdx] = originalLine.replace(search, replace);
            fs.writeFileSync(fullPath, fileLines.join('\n'));
            return `EDIT ${file}:${targetIdx + 1}: "${search.slice(0, 25)}..." → "${replace.slice(0, 25)}..."`;
          }
        } catch (error: any) {
          this.report('fixing', this.attempts.length + 1, `Edit failed: ${error.message}`);
        }
      }
    }

    if (action === 'INSERT') {
      const fileLine = lines.find(l => l.trim().startsWith('FILE:'));
      const afterLine = lines.find(l => l.trim().startsWith('AFTER_LINE:'));
      const contentLine = lines.find(l => l.trim().startsWith('CONTENT:'));

      if (fileLine && contentLine) {
        const file = fileLine.replace('FILE:', '').trim();
        const afterLineNum = afterLine ? parseInt(afterLine.replace('AFTER_LINE:', '').trim(), 10) : 0;
        const insertContent = contentLine.replace('CONTENT:', '').trim();

        try {
          const fullPath = path.resolve(this.config.workingDir, file);
          const content = fs.readFileSync(fullPath, 'utf-8');
          const fileLines = content.split('\n');

          fileLines.splice(afterLineNum, 0, insertContent);
          fs.writeFileSync(fullPath, fileLines.join('\n'));
          return `INSERT ${file}:${afterLineNum + 1}: "${insertContent.slice(0, 40)}..."`;
        } catch (error: any) {
          this.report('fixing', this.attempts.length + 1, `Insert failed: ${error.message}`);
        }
      }
    }

    return null;
  }

  /**
   * Parse and apply LLM's suggested fix
   */
  private applyLLMFix(response: string, primaryError: BuildError): string | null {
    const lines = response.trim().split('\n');
    const firstLine = lines[0].trim();

    // CANNOT_FIX response
    if (firstLine.startsWith('CANNOT_FIX')) {
      this.report('fixing', this.attempts.length + 1, `LLM: ${firstLine}`);
      return null;
    }

    // EDIT response
    if (firstLine.startsWith('EDIT ')) {
      const file = firstLine.slice(5).trim();
      const lineMatch = response.match(/LINE\s+(\d+)/i);
      const oldMatch = response.match(/OLD:\s*(.+)/i);
      const newMatch = response.match(/NEW:\s*(.+)/i);

      if (lineMatch && oldMatch && newMatch) {
        const lineNum = parseInt(lineMatch[1], 10);
        const oldText = oldMatch[1].trim();
        const newText = newMatch[1].trim();

        try {
          const fullPath = path.resolve(this.config.workingDir, file);
          const content = fs.readFileSync(fullPath, 'utf-8');
          const fileLines = content.split('\n');

          // Find the line (may have shifted slightly)
          let targetLine = lineNum - 1;
          if (targetLine >= 0 && targetLine < fileLines.length) {
            // Check if oldText matches (loosely - trim whitespace)
            const currentLine = fileLines[targetLine].trim();
            if (currentLine.includes(oldText.trim()) || oldText.trim().includes(currentLine)) {
              // Preserve indentation
              const indent = fileLines[targetLine].match(/^(\s*)/)?.[1] || '';
              fileLines[targetLine] = indent + newText;
              fs.writeFileSync(fullPath, fileLines.join('\n'));
              return `LLM fix: ${file}:${lineNum} - replaced "${oldText.slice(0, 30)}..." with "${newText.slice(0, 30)}..."`;
            }
          }
        } catch (error: any) {
          this.report('fixing', this.attempts.length + 1, `LLM fix failed: ${error.message}`);
        }
      }
    }

    // IMPORT response
    if (firstLine.startsWith('IMPORT ')) {
      const file = firstLine.slice(7).trim();
      const importLine = lines.slice(1).find(l => l.trim().startsWith('import'));

      if (importLine) {
        try {
          const fullPath = path.resolve(this.config.workingDir, file);
          const content = fs.readFileSync(fullPath, 'utf-8');
          const fileLines = content.split('\n');

          // Check if import already exists
          if (fileLines.some(l => l.includes(importLine.trim()))) {
            return null;
          }

          // Find last import line
          let insertIndex = 0;
          for (let i = 0; i < fileLines.length; i++) {
            if (fileLines[i].trim().startsWith('import ')) {
              insertIndex = i + 1;
            }
          }

          fileLines.splice(insertIndex, 0, importLine.trim());
          fs.writeFileSync(fullPath, fileLines.join('\n'));
          return `LLM fix: Added import to ${file}: ${importLine.trim()}`;
        } catch (error: any) {
          this.report('fixing', this.attempts.length + 1, `LLM import fix failed: ${error.message}`);
        }
      }
    }

    return null;
  }

  /**
   * Try to fix a single error
   */
  private async tryFixError(error: BuildError): Promise<string | null> {
    // TypeScript: Property 'x' does not exist on type 'y'
    if (error.code === 'TS2339' && error.file !== 'unknown') {
      // This usually needs human judgment, but we can report it clearly
      return null;
    }

    // TypeScript: Cannot find name 'x' - might be missing import
    if (error.code === 'TS2304') {
      const match = error.message.match(/Cannot find name '(\w+)'/);
      if (match) {
        const missingName = match[1];
        // Common cases we can handle
        if (['fs', 'path', 'exec', 'execSync'].includes(missingName)) {
          return await this.addNodeImport(error.file, missingName);
        }
      }
    }

    // TypeScript: 'x' is declared but its value is never read
    if (error.code === 'TS6133' && error.file !== 'unknown') {
      // Could prefix with underscore, but risky - escalate
      return null;
    }

    // TypeScript: Type 'X' is not assignable to type 'Y'
    if (error.code === 'TS2322' && error.file !== 'unknown') {
      return await this.tryFixTypeAssignment(error);
    }

    return null;
  }

  /**
   * Try to fix TS2322: Type 'X' is not assignable to type 'Y'
   * Strategy: Read the line, identify the type annotation, fix it
   */
  private async tryFixTypeAssignment(error: BuildError): Promise<string | null> {
    const fullPath = path.resolve(this.config.workingDir, error.file);
    if (!fs.existsSync(fullPath)) return null;

    const content = fs.readFileSync(fullPath, 'utf-8');
    const lines = content.split('\n');
    const lineIndex = error.line - 1;

    if (lineIndex < 0 || lineIndex >= lines.length) return null;

    const line = lines[lineIndex];

    // Parse the error message: Type 'X' is not assignable to type 'Y'
    const typeMatch = error.message.match(/Type '([^']+)' is not assignable to type '([^']+)'/);
    if (!typeMatch) return null;

    const actualType = typeMatch[1];
    const expectedType = typeMatch[2];

    // Case 1: Variable declaration with wrong type annotation
    // e.g., "const fullPath: number = path.join(...)" should be "const fullPath: string = ..."
    const varDeclMatch = line.match(/^(\s*(?:const|let|var)\s+\w+):\s*(\w+)(\s*=.*)$/);
    if (varDeclMatch) {
      const [, prefix, currentType, suffix] = varDeclMatch;
      if (currentType === expectedType) {
        // The declaration type is correct, the value is wrong - need AI judgment
        return null;
      }
      // Fix the type annotation
      const newLine = `${prefix}: ${actualType}${suffix}`;
      lines[lineIndex] = newLine;
      fs.writeFileSync(fullPath, lines.join('\n'));
      return `Fixed type annotation: ${expectedType} → ${actualType} at ${error.file}:${error.line}`;
    }

    // Case 2: Function return type - check if we're at a return statement
    // Look for function declaration above to fix return type
    if (line.trim().startsWith('return ')) {
      // Find the function declaration above
      for (let i = lineIndex - 1; i >= 0; i--) {
        const funcMatch = lines[i].match(/^(\s*(?:async\s+)?function\s+\w+\s*\([^)]*\)):\s*(\w+)(\s*\{?)$/);
        if (funcMatch) {
          const [, funcSig, returnType, brace] = funcMatch;
          if (returnType === expectedType) {
            // Change return type to match actual
            lines[i] = `${funcSig}: ${actualType}${brace}`;
            fs.writeFileSync(fullPath, lines.join('\n'));
            return `Fixed return type: ${expectedType} → ${actualType} at ${error.file}:${i + 1}`;
          }
          break;
        }
      }
    }

    // Case 3: Object property shorthand in return statement
    // e.g., "return { id, name }" where id parameter has wrong type
    // Error appears on line with "id," - need to fix function parameter
    const propMatch = line.trim().match(/^(\w+),?\s*(\/\/.*)?$/);
    if (propMatch) {
      const propName = propMatch[1];
      // Find the function declaration above and fix the parameter type
      for (let i = lineIndex - 1; i >= 0; i--) {
        const funcLine = lines[i];
        // Match function with parameters
        const funcDeclMatch = funcLine.match(/^(\s*(?:async\s+)?function\s+\w+\s*\()([^)]+)(\).*)/);
        if (funcDeclMatch) {
          const [, prefix, params, suffix] = funcDeclMatch;
          // Parse parameters to find the one we need to fix
          const paramList = params.split(',').map(p => p.trim());
          let fixedParams = [];
          let fixed = false;

          for (const param of paramList) {
            // Match "name: type" pattern
            const paramMatch = param.match(/^(\w+):\s*(\w+)$/);
            if (paramMatch && paramMatch[1] === propName) {
              // This is the parameter we need to fix
              if (paramMatch[2] === actualType) {
                // Parameter type matches actual, interface expects different
                // Change parameter to match expected (interface is truth)
                fixedParams.push(`${propName}: ${expectedType}`);
                fixed = true;
              } else {
                fixedParams.push(param);
              }
            } else {
              fixedParams.push(param);
            }
          }

          if (fixed) {
            lines[i] = `${prefix}${fixedParams.join(', ')}${suffix}`;
            fs.writeFileSync(fullPath, lines.join('\n'));
            return `Fixed parameter type: ${propName}: ${actualType} → ${expectedType} at ${error.file}:${i + 1}`;
          }
          break;
        }
      }
    }

    return null;
  }

  /**
   * Add a Node.js import to a file
   */
  private async addNodeImport(file: string, name: string): Promise<string | null> {
    const fullPath = path.resolve(this.config.workingDir, file);
    if (!fs.existsSync(fullPath)) return null;

    const content = fs.readFileSync(fullPath, 'utf-8');

    // Determine the import to add
    let importStatement = '';
    if (name === 'fs') importStatement = "import * as fs from 'fs';\n";
    else if (name === 'path') importStatement = "import * as path from 'path';\n";
    else if (name === 'exec' || name === 'execSync') {
      importStatement = "import { exec, execSync } from 'child_process';\n";
    }

    if (!importStatement) return null;

    // Check if already imported (only check actual import lines, not comments)
    const lines = content.split('\n');
    const hasImport = lines.some(line => {
      const trimmed = line.trim();
      return !trimmed.startsWith('//') && trimmed.includes(importStatement.trim());
    });
    if (hasImport) return null;

    // Add at top of file (after any existing imports)
    let insertIndex = 0;
    for (let i = 0; i < lines.length; i++) {
      if (lines[i].startsWith('import ')) {
        insertIndex = i + 1;
      }
    }

    lines.splice(insertIndex, 0, importStatement.trim());
    fs.writeFileSync(fullPath, lines.join('\n'));

    return `Added import: ${importStatement.trim()} to ${file}`;
  }

  /**
   * Report progress
   */
  private report(phase: SentinelProgress['phase'], attempt: number, message: string, errors?: BuildError[]) {
    this.config.onProgress({
      phase,
      attempt,
      maxAttempts: this.config.maxAttempts,
      message,
      errors,
    });
  }
}

// Export for direct testing
export async function testBuildSentinel() {
  const sentinel = new BuildSentinel({
    command: 'npm run build:ts',
    workingDir: '/Volumes/FlashGordon/cambrian/continuum/src/debug/jtag',
    maxAttempts: 3,
    onProgress: (p) => {
      console.log(`[${p.phase.toUpperCase()}] (${p.attempt}/${p.maxAttempts}) ${p.message}`);
      if (p.errors && p.errors.length > 0) {
        p.errors.slice(0, 3).forEach((e) => {
          console.log(`  - ${e.file}:${e.line}: ${e.message}`);
        });
        if (p.errors.length > 3) {
          console.log(`  ... and ${p.errors.length - 3} more`);
        }
      }
    },
  });

  console.log('\n=== BuildSentinel Test ===\n');
  const result = await sentinel.run();

  console.log('\n=== Result ===');
  console.log(`Success: ${result.success}`);
  console.log(`Attempts: ${result.attempts.length}`);
  if (result.escalated) {
    console.log(`Escalated: ${result.escalationReason}`);
  }
  if (result.finalErrors && result.finalErrors.length > 0) {
    console.log(`Final errors: ${result.finalErrors.length}`);
  }

  return result;
}
