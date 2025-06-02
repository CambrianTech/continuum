/**
 * WebFetch Tool - Handles web content fetching
 * Manages its own concerns: URL parsing, HTTP requests, content processing
 */

import { ITool, ToolExecutionResult, ToolMetrics } from '../interfaces/agent.interface';

export class WebFetchTool implements ITool {
  public readonly name = 'WebFetch';
  private metrics: ToolMetrics = {
    executions: 0,
    errors: 0,
    totalTime: 0,
    averageTime: 0,
    successRate: 0
  };

  public async processResponse(response: string): Promise<readonly ToolExecutionResult[]> {
    const results: ToolExecutionResult[] = [];
    const matches = response.match(/WEBFETCH:\s*(https?:\/\/[^\s\n]+)/gi);
    
    if (matches) {
      for (const match of matches) {
        const urlMatch = match.match(/WEBFETCH:\s*(https?:\/\/[^\s\n]+)/i);
        if (urlMatch) {
          const url = urlMatch[1];
          const result = await this.execute(url);
          results.push(result);
        }
      }
    }
    
    return Object.freeze(results);
  }

  public async execute(url: string): Promise<ToolExecutionResult> {
    const startTime = Date.now();
    
    try {
      console.log(`🌐 ${this.name} fetching: ${url}`);
      
      const content = await this.fetchPage(url);
      const duration = Date.now() - startTime;
      
      this.updateMetrics({ executions: 1, totalTime: duration });
      
      return {
        tool: this.name,
        command: url,
        result: content,
        success: true,
        timestamp: new Date(),
        duration
      };
      
    } catch (error) {
      const duration = Date.now() - startTime;
      this.updateMetrics({ executions: 1, errors: 1, totalTime: duration });
      
      return {
        tool: this.name,
        command: url,
        result: `Error: ${error.message}`,
        success: false,
        timestamp: new Date(),
        duration
      };
    }
  }

  private async fetchPage(url: string): Promise<string> {
    // Dynamic import for ES module compatibility
    const { default: fetch } = await import('node-fetch');
    
    const response = await fetch(url);
    if (!response.ok) {
      throw new Error(`HTTP ${response.status}: ${response.statusText}`);
    }
    
    const text = await response.text();
    
    // Simple HTML to text conversion
    const plainText = text
      .replace(/<script[^>]*>[\s\S]*?<\/script>/gi, '')
      .replace(/<style[^>]*>[\s\S]*?<\/style>/gi, '')
      .replace(/<[^>]+>/g, ' ')
      .replace(/\s+/g, ' ')
      .trim();
    
    return plainText.substring(0, 2000); // Limit to 2KB
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