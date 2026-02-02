/**
 * Code Verify Command - Server Implementation
 *
 * Runs TypeScript compilation checks and optionally executes tests
 * via ExecutionSandbox (process-isolated, timeout-enforced).
 *
 * Workspace resolution:
 * - If `cwd` param is provided, use it directly
 * - Otherwise, resolve from userId: {jtagRoot}/.continuum/personas/{userId}/workspace/
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { CodeVerifyParams, CodeVerifyResult, TypeScriptError, TestResult } from '../shared/CodeVerifyTypes';
import { createCodeVerifyResultFromParams } from '../shared/CodeVerifyTypes';
import { ExecutionSandbox } from '@system/code/server/ExecutionSandbox';
import type { SandboxResult } from '@system/code/server/ExecutionSandbox';
import * as path from 'path';
import * as fs from 'fs';

/** TypeScript error regex: file(line,col): error TSxxxx: message */
const TS_ERROR_REGEX = /^(.+?)\((\d+),(\d+)\):\s*error\s+(TS\d+):\s*(.+)$/gm;

export class CodeVerifyServerCommand extends CommandBase<CodeVerifyParams, CodeVerifyResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('code/verify', context, subpath, commander);
  }

  async execute(params: CodeVerifyParams): Promise<CodeVerifyResult> {
    const startTime = Date.now();

    if (!params.userId) {
      throw new ValidationError('userId', 'Verification requires a userId (auto-injected for persona tool calls).');
    }

    const workspaceDir = this.resolveWorkspaceDir(params);
    const sandbox = new ExecutionSandbox();
    const doTypeCheck = params.typeCheck !== false;
    const doTests = params.testFiles && params.testFiles.length > 0;

    let typeCheckResult: CodeVerifyResult['typeCheck'] | undefined;
    let testsResult: TestResult | undefined;
    let output = '';
    let allPassed = true;

    // Phase 1: TypeScript compilation check
    if (doTypeCheck) {
      const tscResult = await this.runTypeCheck(sandbox, workspaceDir, params.userId);
      const errors = this.parseTypeScriptErrors(tscResult.stdout + tscResult.stderr);

      typeCheckResult = {
        passed: tscResult.success,
        errorCount: errors.length,
        errors,
      };

      output += tscResult.stdout + tscResult.stderr;
      if (!tscResult.success) allPassed = false;
    }

    // Phase 2: Test execution (optional)
    if (doTests && params.testFiles) {
      const testRunResult = await this.runTests(sandbox, workspaceDir, params.testFiles, params.userId);
      testsResult = this.parseTestResult(testRunResult);

      output += '\n' + testRunResult.stdout + testRunResult.stderr;
      if (!testsResult.passed) allPassed = false;
    }

    const durationMs = Date.now() - startTime;

    return createCodeVerifyResultFromParams(params, {
      success: allPassed,
      typeCheck: typeCheckResult,
      tests: testsResult,
      durationMs,
      output,
    });
  }

  /**
   * Resolve the workspace directory from params.
   * Uses explicit cwd if provided, otherwise resolves from userId convention.
   */
  private resolveWorkspaceDir(params: CodeVerifyParams): string {
    if (params.cwd && params.cwd.trim()) {
      return params.cwd;
    }

    const jtagRoot = process.cwd();
    const personaId = params.userId!;

    // Standard persona workspace path
    const workspaceDir = path.join(jtagRoot, '.continuum', 'personas', personaId, 'workspace');

    if (fs.existsSync(workspaceDir)) {
      return workspaceDir;
    }

    // Fallback: check if userId is a challenge workspace handle (challenge-{id}-{personaId})
    if (personaId.startsWith('challenge-')) {
      const parts = personaId.split('-');
      // Handle: challenge-{challengeId}-{personaId}
      // The challengeId and personaId are UUIDs, so we need the full pattern
      const challengeIdStart = 'challenge-'.length;
      // Find the persona ID (last UUID in the handle)
      const uuidLen = 36; // Standard UUID length
      if (personaId.length > challengeIdStart + uuidLen + 1) {
        const actualPersonaId = personaId.slice(-(uuidLen));
        const challengeId = personaId.slice(challengeIdStart, personaId.length - uuidLen - 1);
        const challengeDir = path.join(jtagRoot, '.continuum', 'personas', actualPersonaId, 'challenges', challengeId);
        if (fs.existsSync(challengeDir)) {
          return challengeDir;
        }
      }
    }

    // Last resort: use the standard workspace path even if it doesn't exist yet
    return workspaceDir;
  }

  /**
   * Run TypeScript compilation check via ExecutionSandbox.
   */
  private async runTypeCheck(sandbox: ExecutionSandbox, workspaceDir: string, personaId: string): Promise<SandboxResult> {
    // Check if workspace has a tsconfig.json — if so, tsc uses it automatically
    const hasTsConfig = fs.existsSync(path.join(workspaceDir, 'tsconfig.json'));

    const args = hasTsConfig
      ? ['tsc', '--noEmit']
      : ['tsc', '--noEmit', '--strict', ...this.findTypeScriptFiles(workspaceDir)];

    return sandbox.execute({
      command: 'npx',
      args,
      cwd: workspaceDir,
      timeoutMs: 120_000,
      maxOutputBytes: 102_400,
      personaId: personaId as any,
    });
  }

  /**
   * Run test files via vitest in sandbox.
   */
  private async runTests(
    sandbox: ExecutionSandbox,
    workspaceDir: string,
    testFiles: string[],
    personaId: string,
  ): Promise<SandboxResult> {
    return sandbox.execute({
      command: 'npx',
      args: ['vitest', 'run', ...testFiles, '--reporter=json'],
      cwd: workspaceDir,
      timeoutMs: 120_000,
      maxOutputBytes: 102_400,
      personaId: personaId as any,
    });
  }

  /**
   * Find .ts files in workspace for compilation without tsconfig.
   */
  private findTypeScriptFiles(workspaceDir: string): string[] {
    const files: string[] = [];
    try {
      const entries = fs.readdirSync(workspaceDir, { withFileTypes: true });
      for (const entry of entries) {
        if (entry.isFile() && entry.name.endsWith('.ts') && !entry.name.endsWith('.d.ts')) {
          files.push(entry.name);
        }
      }
    } catch {
      // Directory doesn't exist or isn't readable
    }
    return files;
  }

  /**
   * Parse TypeScript compiler output into structured errors.
   * Format: file(line,col): error TSxxxx: message
   */
  private parseTypeScriptErrors(output: string): TypeScriptError[] {
    const errors: TypeScriptError[] = [];
    let match;

    // Reset regex state
    TS_ERROR_REGEX.lastIndex = 0;

    while ((match = TS_ERROR_REGEX.exec(output)) !== null) {
      errors.push({
        file: match[1],
        line: parseInt(match[2], 10),
        column: parseInt(match[3], 10),
        code: match[4],
        message: match[5],
      });
    }

    return errors;
  }

  /**
   * Parse vitest JSON output into a TestResult.
   */
  private parseTestResult(sandboxResult: SandboxResult): TestResult {
    if (sandboxResult.timedOut) {
      return {
        passed: false,
        total: 0,
        passedCount: 0,
        failedCount: 0,
        failures: ['Test execution timed out'],
      };
    }

    try {
      // vitest --reporter=json outputs JSON to stdout
      const json = JSON.parse(sandboxResult.stdout);
      const numPassed = json.numPassedTests ?? 0;
      const numFailed = json.numFailedTests ?? 0;
      const total = json.numTotalTests ?? (numPassed + numFailed);
      const failures = (json.testResults ?? [])
        .flatMap((suite: any) => (suite.assertionResults ?? [])
          .filter((t: any) => t.status === 'failed')
          .map((t: any) => `${t.ancestorTitles?.join(' > ')} > ${t.title}: ${t.failureMessages?.[0] ?? 'Failed'}`)
        );

      return {
        passed: numFailed === 0,
        total,
        passedCount: numPassed,
        failedCount: numFailed,
        failures,
      };
    } catch {
      // Non-JSON output — treat as failure
      return {
        passed: sandboxResult.success,
        total: 0,
        passedCount: 0,
        failedCount: sandboxResult.success ? 0 : 1,
        failures: sandboxResult.success ? [] : [sandboxResult.stderr || sandboxResult.stdout || 'Unknown test failure'],
      };
    }
  }
}
