/**
 * CodingChallengeRunner - Execute coding challenges and capture results
 *
 * Runs a coding challenge against the code/task pipeline:
 * 1. Set up workspace with challenge files
 * 2. Execute code/task with the challenge description
 * 3. Collect result files from workspace
 * 4. Pass to CodingJudge for evaluation
 * 5. Record attempt on entity
 *
 * Each challenge gets a fresh workspace to prevent state leakage.
 */

import { Logger } from '../../core/logging/Logger';
import { CodeDaemon } from '../../../daemons/code-daemon/shared/CodeDaemon';
import { CodeAgentOrchestrator } from './CodeAgentOrchestrator';
import { CodingJudge } from './CodingJudge';
import type { CodingTask, ExecutionOptions } from '../shared/CodingTypes';
import type { CodingChallengeEntity, ChallengeAttempt, AttemptStatus } from '../../data/entities/CodingChallengeEntity';
import type { UUID } from '../../core/types/CrossPlatformUUID';
import { v4 as uuidv4 } from 'uuid';
import * as fs from 'fs';
import * as path from 'path';

const log = Logger.create('CodingChallengeRunner', 'code');

export interface ChallengeRunOptions {
  /** Which AI persona runs the challenge */
  personaId: UUID;
  /** Skip AI judge evaluation (just check execution success) */
  skipJudge?: boolean;
  /** Override security tier (default: write) */
  securityTier?: string;
}

export interface ChallengeRunResult {
  success: boolean;
  attempt: ChallengeAttempt;
  /** Raw code/task result */
  taskResult?: Record<string, unknown>;
}

export class CodingChallengeRunner {
  private readonly orchestrator: CodeAgentOrchestrator;
  private readonly judge: CodingJudge;

  constructor() {
    this.orchestrator = new CodeAgentOrchestrator();
    this.judge = new CodingJudge();
  }

  /**
   * Execute a coding challenge for a persona.
   *
   * Creates a fresh workspace, seeds it with challenge files,
   * runs the coding pipeline, evaluates results, and records the attempt.
   */
  async run(challenge: CodingChallengeEntity, options: ChallengeRunOptions): Promise<ChallengeRunResult> {
    const { personaId } = options;
    const startedAt = Date.now();

    log.info(`Running challenge "${challenge.name}" (${challenge.difficulty}) for persona ${personaId}`);

    try {
      // Phase 1: Set up challenge workspace with unique handle
      const workspaceHandle = `challenge-${(challenge.id ?? challenge.sequenceNumber)}-${personaId}`;
      const workspaceDir = await this.setupChallengeWorkspace(challenge, personaId, workspaceHandle);

      // Phase 2: Execute the coding task
      const task: CodingTask = {
        id: uuidv4() as UUID,
        personaId,
        description: challenge.description,
        taskType: 'generation',
        maxDurationMs: challenge.timeLimitMs,
        maxToolCalls: challenge.toolCallLimit,
        workspaceHandle,
        relevantFiles: Object.keys(challenge.setupFiles),
        createdAt: Date.now(),
      };

      const execOptions: ExecutionOptions = {
        dryRun: false,
        securityTier: (options.securityTier as any) ?? 'write',
      };

      const result = await this.orchestrator.execute(task, execOptions);

      // Phase 3: Collect result files from workspace
      const resultFiles = await this.collectResultFiles(workspaceDir, challenge);

      // Phase 4: Judge evaluation
      const completedAt = Date.now();
      let score = 0;
      let feedback = '';
      let status: AttemptStatus;

      if (result.status === 'completed' || result.status === 'partial') {
        if (options.skipJudge) {
          score = result.status === 'completed' ? 70 : 40;
          feedback = `Pipeline ${result.status}. ${result.stepResults.filter(s => s.status === 'completed').length}/${result.stepResults.length} steps completed.`;
          status = result.status === 'completed' ? 'passed' : 'partial';
        } else {
          const evaluation = await this.judge.evaluate(challenge, resultFiles, result);
          score = evaluation.score;
          feedback = evaluation.feedback;
          status = evaluation.passed ? 'passed' : evaluation.score >= 40 ? 'partial' : 'failed';
        }
      } else if (result.status === 'budget_exceeded') {
        status = 'timeout';
        feedback = `Budget exceeded: ${result.errors.join('; ')}`;
      } else {
        status = 'failed';
        feedback = `Execution failed: ${result.errors.join('; ')}`;
      }

      const attempt: ChallengeAttempt = {
        personaId,
        planId: task.id,
        startedAt,
        completedAt,
        status,
        score,
        feedback,
        filesModified: result.filesModified,
        filesCreated: result.filesCreated,
        errors: result.errors,
        toolCallsUsed: result.totalToolCalls,
        durationMs: result.totalDurationMs,
        resultFiles,
      };

      // Phase 5: Record attempt on entity
      challenge.recordAttempt(attempt);

      log.info(`Challenge "${challenge.name}" ${status}: score=${score}, duration=${result.totalDurationMs}ms`);

      return {
        success: status === 'passed',
        attempt,
        taskResult: result as unknown as Record<string, unknown>,
      };

    } catch (error) {
      const completedAt = Date.now();
      const message = error instanceof Error ? error.message : String(error);
      log.error(`Challenge "${challenge.name}" error: ${message}`);

      const attempt: ChallengeAttempt = {
        personaId,
        startedAt,
        completedAt,
        status: 'error',
        score: 0,
        feedback: `Runner error: ${message}`,
        filesModified: [],
        filesCreated: [],
        errors: [message],
        toolCallsUsed: 0,
        durationMs: completedAt - startedAt,
      };

      challenge.recordAttempt(attempt);

      return { success: false, attempt };
    }
  }

  /**
   * Set up a fresh workspace with challenge files.
   * Creates the workspace directory and writes all setup files.
   */
  private async setupChallengeWorkspace(
    challenge: CodingChallengeEntity,
    personaId: UUID,
    workspaceHandle: string,
  ): Promise<string> {
    const jtagRoot = process.cwd();
    const challengeWorkspace = path.join(
      jtagRoot, '.continuum', 'personas', personaId as string,
      'challenges', challenge.id as string,
    );

    // Create fresh workspace
    if (fs.existsSync(challengeWorkspace)) {
      fs.rmSync(challengeWorkspace, { recursive: true });
    }
    fs.mkdirSync(challengeWorkspace, { recursive: true });

    // Write setup files
    for (const [filePath, content] of Object.entries(challenge.setupFiles)) {
      const fullPath = path.join(challengeWorkspace, filePath);
      const dir = path.dirname(fullPath);
      if (!fs.existsSync(dir)) {
        fs.mkdirSync(dir, { recursive: true });
      }
      fs.writeFileSync(fullPath, content, 'utf-8');
    }

    // Register workspace in Rust backend using unique handle (writable, no read roots)
    await CodeDaemon.createWorkspace(workspaceHandle, challengeWorkspace);

    log.debug(`Challenge workspace set up at ${challengeWorkspace} with ${Object.keys(challenge.setupFiles).length} files`);

    return challengeWorkspace;
  }

  /**
   * Collect result files from workspace after execution.
   * Reads all files that were part of the challenge setup, plus any new files.
   */
  private async collectResultFiles(
    workspaceDir: string,
    challenge: CodingChallengeEntity,
  ): Promise<Record<string, string>> {
    const resultFiles: Record<string, string> = {};

    const collectDir = (dir: string, prefix: string = ''): void => {
      if (!fs.existsSync(dir)) return;
      const entries = fs.readdirSync(dir, { withFileTypes: true });
      for (const entry of entries) {
        const relativePath = prefix ? `${prefix}/${entry.name}` : entry.name;
        const fullPath = path.join(dir, entry.name);
        if (entry.isDirectory()) {
          collectDir(fullPath, relativePath);
        } else if (entry.isFile()) {
          try {
            resultFiles[relativePath] = fs.readFileSync(fullPath, 'utf-8');
          } catch {
            // Skip unreadable files
          }
        }
      }
    };

    collectDir(workspaceDir);
    return resultFiles;
  }
}
