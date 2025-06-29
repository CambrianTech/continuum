/**
 * FileSystem Tool - Handles file operations
 * Manages its own concerns: file reading, writing, path validation
 */

import { ITool, ToolExecutionResult, ToolMetrics } from '../interfaces/agent.interface';
import * as fs from 'fs';
import * as path from 'path';

export class FileSystemTool implements ITool {
  public readonly name = 'FileSystem';
  private metrics: ToolMetrics = {
    executions: 0,
    errors: 0,
    totalTime: 0,
    averageTime: 0,
    successRate: 0
  };

  public async processResponse(response: string): Promise<readonly ToolExecutionResult[]> {
    const results: ToolExecutionResult[] = [];
    
    // Process FILE_READ commands
    const readMatches = response.match(/FILE_READ:\s*([^\s\n]+)/gi);
    if (readMatches) {
      for (const match of readMatches) {
        const fileMatch = match.match(/FILE_READ:\s*([^\s\n]+)/i);
        if (fileMatch) {
          const filePath = fileMatch[1];
          const result = await this.readFile(filePath);
          results.push(result);
        }
      }
    }
    
    // Process FILE_WRITE commands
    const writeMatches = response.match(/FILE_WRITE:\s*([^\s\n]+)\s+(.+)/gi);
    if (writeMatches) {
      for (const match of writeMatches) {
        const writeMatch = match.match(/FILE_WRITE:\s*([^\s\n]+)\s+(.+)/i);
        if (writeMatch) {
          const filePath = writeMatch[1];
          const content = writeMatch[2];
          const result = await this.writeFile(filePath, content);
          results.push(result);
        }
      }
    }
    
    return Object.freeze(results);
  }

  public async execute(command: string): Promise<ToolExecutionResult> {
    // Parse command to determine operation
    if (command.startsWith('READ:')) {
      const filePath = command.substring(5).trim();
      return this.readFile(filePath);
    } else if (command.startsWith('WRITE:')) {
      const parts = command.substring(6).split(' ', 2);
      const filePath = parts[0];
      const content = parts[1] || '';
      return this.writeFile(filePath, content);
    } else {
      throw new Error(`Unknown filesystem command: ${command}`);
    }
  }

  private async readFile(filePath: string): Promise<ToolExecutionResult> {
    const startTime = Date.now();
    
    try {
      console.log(`📖 ${this.name} reading: ${filePath}`);
      
      // Security check - prevent reading outside project
      if (this.isUnsafePath(filePath)) {
        throw new Error('Access denied: unsafe file path');
      }
      
      const content = fs.readFileSync(filePath, 'utf-8');
      const duration = Date.now() - startTime;
      
      this.updateMetrics({ executions: 1, totalTime: duration });
      
      return {
        tool: this.name,
        command: `READ: ${filePath}`,
        result: content.substring(0, 1000), // Limit content for performance
        success: true,
        timestamp: new Date(),
        duration
      };
      
    } catch (error) {
      const duration = Date.now() - startTime;
      this.updateMetrics({ executions: 1, errors: 1, totalTime: duration });
      
      return {
        tool: this.name,
        command: `READ: ${filePath}`,
        result: `Error: ${error.message}`,
        success: false,
        timestamp: new Date(),
        duration
      };
    }
  }

  private async writeFile(filePath: string, content: string): Promise<ToolExecutionResult> {
    const startTime = Date.now();
    
    try {
      console.log(`✏️  ${this.name} writing: ${filePath}`);
      
      // Security check
      if (this.isUnsafePath(filePath)) {
        throw new Error('Access denied: unsafe file path');
      }
      
      // Ensure directory exists
      const dir = path.dirname(filePath);
      if (!fs.existsSync(dir)) {
        fs.mkdirSync(dir, { recursive: true });
      }
      
      fs.writeFileSync(filePath, content, 'utf-8');
      const duration = Date.now() - startTime;
      
      this.updateMetrics({ executions: 1, totalTime: duration });
      
      return {
        tool: this.name,
        command: `WRITE: ${filePath}`,
        result: `File written successfully (${content.length} bytes)`,
        success: true,
        timestamp: new Date(),
        duration
      };
      
    } catch (error) {
      const duration = Date.now() - startTime;
      this.updateMetrics({ executions: 1, errors: 1, totalTime: duration });
      
      return {
        tool: this.name,
        command: `WRITE: ${filePath}`,
        result: `Error: ${error.message}`,
        success: false,
        timestamp: new Date(),
        duration
      };
    }
  }

  private isUnsafePath(filePath: string): boolean {
    const normalized = path.normalize(filePath);
    const projectRoot = process.cwd();
    const resolved = path.resolve(projectRoot, normalized);
    
    // Check if path is within project directory
    return !resolved.startsWith(projectRoot) || 
           normalized.includes('..') ||
           normalized.includes('/etc/') ||
           normalized.includes('/usr/') ||
           normalized.includes('/var/');
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