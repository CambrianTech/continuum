/**
 * Git Tool - Handles git operations
 * Manages its own concerns: git commands, status checking, commits
 */

import { ITool, ToolExecutionResult, ToolMetrics } from '../interfaces/agent.interface';
import { exec } from 'child_process';
import { promisify } from 'util';

const execAsync = promisify(exec);

export class GitTool implements ITool {
  public readonly name = 'Git';
  private metrics: ToolMetrics = {
    executions: 0,
    errors: 0,
    totalTime: 0,
    averageTime: 0,
    successRate: 0
  };

  public async processResponse(response: string): Promise<readonly ToolExecutionResult[]> {
    const results: ToolExecutionResult[] = [];
    
    // Process GIT_STATUS commands
    const statusMatches = response.match(/GIT_STATUS/gi);
    if (statusMatches) {
      for (const match of statusMatches) {
        const result = await this.execute('status');
        results.push(result);
      }
    }
    
    // Process GIT_COMMIT commands
    const commitMatches = response.match(/GIT_COMMIT:\s*"([^"]+)"/gi);
    if (commitMatches) {
      for (const match of commitMatches) {
        const commitMatch = match.match(/GIT_COMMIT:\s*"([^"]+)"/i);
        if (commitMatch) {
          const message = commitMatch[1];
          const result = await this.execute(`commit:${message}`);
          results.push(result);
        }
      }
    }
    
    return Object.freeze(results);
  }

  public async execute(command: string): Promise<ToolExecutionResult> {
    const startTime = Date.now();
    
    try {
      console.log(`📊 ${this.name} executing: ${command}`);
      
      let gitCommand: string;
      let operation: string;
      
      if (command === 'status') {
        gitCommand = 'git status --porcelain';
        operation = 'STATUS';
      } else if (command.startsWith('commit:')) {
        const message = command.substring(7);
        gitCommand = `git commit -m "${message}"`;
        operation = 'COMMIT';
      } else if (command === 'diff') {
        gitCommand = 'git diff --name-status';
        operation = 'DIFF';
      } else if (command === 'log') {
        gitCommand = 'git log --oneline -5';
        operation = 'LOG';
      } else {
        throw new Error(`Unknown git operation: ${command}`);
      }
      
      const { stdout, stderr } = await execAsync(gitCommand);
      const duration = Date.now() - startTime;
      
      this.updateMetrics({ executions: 1, totalTime: duration });
      
      const result = stdout || stderr || 'Command completed successfully';
      
      return {
        tool: this.name,
        command: `GIT_${operation}`,
        result: result.substring(0, 1000), // Limit output
        success: true,
        timestamp: new Date(),
        duration
      };
      
    } catch (error) {
      const duration = Date.now() - startTime;
      this.updateMetrics({ executions: 1, errors: 1, totalTime: duration });
      
      return {
        tool: this.name,
        command: `GIT_${command.toUpperCase()}`,
        result: `Error: ${error.message}`,
        success: false,
        timestamp: new Date(),
        duration
      };
    }
  }

  public getMetrics(): ToolMetrics {
    this.metrics.averageTime = this.metrics.executions > 0 
      ? this.metrics.totalTime / this.metrics.executions 
      : 0;
    this.metrics.successRate = this.metrics.executions > 0
      ? (this.metrics.executions - this.metrics.errors) / this.metrics.executions
      : 0;
    
    return { ...this.metrics };
  }

  private updateMetrics(updates: Partial<ToolMetrics>): void {
    Object.assign(this.metrics, updates);
  }
}