/**
 * ExecCommand Browser - Simple JavaScript execution in browser
 */

import { CommandBase, type ICommandDaemon } from '../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import type { ExecCommandParams, ExecCommandResult } from '../shared/ExecTypes';
import { createExecErrorResult, createExecSuccessResult, DEFAULT_EXEC_TIMEOUT } from '../shared/ExecTypes';

/**
 * Execution safety tracker - prevents infinite loops
 */
interface ExecutionTracker {
  timestamps: number[];
  lastCode: string;
}

export class ExecBrowserCommand extends CommandBase<ExecCommandParams, ExecCommandResult> {
  // Execution safety guards
  private static readonly MAX_EXECUTIONS_PER_SECOND = 10;
  private static readonly EXECUTION_TIMEOUT = 5000; // 5 seconds
  private static readonly RATE_LIMIT_WINDOW = 1000; // 1 second
  private static executionTracker: Map<string, ExecutionTracker> = new Map();

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('exec', context, subpath, commander);
  }
  
  /**
   * Check if execution should be allowed (rate limiting)
   */
  private static checkExecutionRate(sourceCode: string): { allowed: boolean; reason?: string } {
    const now = Date.now();
    const codeHash = this.hashCode(sourceCode);

    if (!this.executionTracker.has(codeHash)) {
      this.executionTracker.set(codeHash, { timestamps: [], lastCode: sourceCode });
    }

    const tracker = this.executionTracker.get(codeHash)!;

    // Remove old timestamps outside the rate limit window
    tracker.timestamps = tracker.timestamps.filter(t => now - t < this.RATE_LIMIT_WINDOW);

    // Check rate limit
    if (tracker.timestamps.length >= this.MAX_EXECUTIONS_PER_SECOND) {
      return {
        allowed: false,
        reason: `Execution rate limit exceeded: ${tracker.timestamps.length} executions in ${this.RATE_LIMIT_WINDOW}ms. Possible infinite loop detected.`
      };
    }

    // Record this execution
    tracker.timestamps.push(now);

    // Cleanup old trackers (prevent memory leak)
    if (this.executionTracker.size > 100) {
      const oldest = Array.from(this.executionTracker.entries())
        .sort(([, a], [, b]) => Math.max(...a.timestamps) - Math.max(...b.timestamps))[0];
      if (oldest && now - Math.max(...oldest[1].timestamps) > 60000) { // 1 minute old
        this.executionTracker.delete(oldest[0]);
      }
    }

    return { allowed: true };
  }

  /**
   * Simple hash function for code deduplication
   */
  private static hashCode(str: string): string {
    let hash = 0;
    for (let i = 0; i < str.length; i++) {
      const char = str.charCodeAt(i);
      hash = ((hash << 5) - hash) + char;
      hash = hash & hash; // Convert to 32bit integer
    }
    return hash.toString(36);
  }

  /**
   * Browser exec: Execute JavaScript directly in browser context
   * Simple execution with DOM access preserved
   */
  async execute(params: ExecCommandParams): Promise<ExecCommandResult> {
    console.log(`🎯 BROWSER EXEC: Starting execution`);

    if (!params.code) {
      return createExecErrorResult('validation', 'Missing required code parameter', 'browser', params);
    }

    if (params.code.type === 'inline' && params.code.language === 'javascript') {
      const sourceCode = params.code.source;

      // SAFETY CHECK: Rate limiting
      const rateCheck = ExecBrowserCommand.checkExecutionRate(sourceCode);
      if (!rateCheck.allowed) {
        console.error(`🚫 BROWSER EXEC: Rate limit exceeded - ${rateCheck.reason}`);
        return createExecErrorResult('runtime', rateCheck.reason!, 'browser', params, 0);
      }

      console.log(`🎯 BROWSER EXEC: Executing JavaScript in browser`);

      try {
        // SAFETY CHECK: Execution timeout
        const timeout = params.timeout || ExecBrowserCommand.EXECUTION_TIMEOUT;
        const executionPromise = new Promise<any>((resolve, reject) => {
          try {
            // Simple execution with DOM access - wrap in async IIFE to support await
            const func = new Function(`
              return (async function() {
                ${sourceCode}
              })();
            `);

            const result = func();

            // Handle promises
            if (result && typeof result.then === 'function') {
              result.then(resolve).catch(reject);
            } else {
              resolve(result);
            }
          } catch (error) {
            reject(error);
          }
        });

        const timeoutPromise = new Promise<never>((_, reject) => {
          setTimeout(() => reject(new Error(`Execution timeout after ${timeout}ms`)), timeout);
        });

        const result = await Promise.race([executionPromise, timeoutPromise]);

        console.log(`✅ BROWSER EXEC: Success - result:`, result);

        // If called from server, send result back via remoteExecute pattern
        if (params.context.environment === 'server') {
          return await this.remoteExecute({
            ...params,
            result,
            executedAt: Date.now(),
            executedIn: 'browser'
          }) as ExecCommandResult;
        }

        // Direct browser call
        return createExecSuccessResult(result, 'browser', params, Date.now());

      } catch (error) {
        console.error(`❌ BROWSER EXEC: JavaScript execution failed:`, error);

        const errorMessage = error instanceof Error ? error.message : String(error);
        return createExecErrorResult('runtime', errorMessage, 'browser', params, Date.now());
      }
    } else {
      return createExecErrorResult('validation', 'Browser exec: unsupported code type', 'browser', params, Date.now());
    }
  }
}