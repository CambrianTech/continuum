/**
 * NASA-Grade Continuum Memory System
 * Mission-critical AI memory and strategy storage
 */

import * as fs from 'fs';
import * as path from 'path';

export interface MemoryItem {
  id: string;
  data: any;
  timestamp: number;
  tags: string[];
}

export interface StrategyData {
  id: string;
  projectType: string;
  strategy: {
    taskDelegation: Record<string, string[]>;
    costOptimization: string[];
    successfulPatterns: string[];
    failurePatterns: string[];
  };
  performance: {
    totalCost: number;
    successRate: number;
    completionTime: number;
    userSatisfaction: number;
  };
  timestamp: number;
  sessionId: string;
  aiAgentsUsed: string[];
  tags: string[];
}

export interface MemoryAnalytics {
  totalStrategies: number;
  averageSuccessRate: number;
  totalCost: number;
  averageCompletionTime: number;
  mostUsedAgents: string[];
  commonPatterns: string[];
}

export class ContinuumMemory {
  private memories = new Map<string, MemoryItem>();
  private strategies = new Map<string, StrategyData>();
  private memoryDir: string;
  
  constructor(private projectRoot: string) {
    this.memoryDir = path.join(projectRoot, '.continuum');
    this.ensureMemoryDirectory();
    this.loadExistingData();
  }
  
  private ensureMemoryDirectory(): void {
    if (!fs.existsSync(this.memoryDir)) {
      fs.mkdirSync(this.memoryDir, { recursive: true });
    }
  }
  
  private loadExistingData(): void {
    try {
      const strategiesFile = path.join(this.memoryDir, 'strategies.json');
      if (fs.existsSync(strategiesFile)) {
        const data = JSON.parse(fs.readFileSync(strategiesFile, 'utf-8'));
        data.forEach((strategy: StrategyData) => {
          this.strategies.set(strategy.id, strategy);
        });
      }
    } catch (error) {
      // Start with fresh memory if loading fails
    }
  }
  
  store(id: string, data: any, tags: string[] = []): void {
    if (typeof id !== 'string' || !id.trim()) {
      throw new Error('Invalid memory ID: must be non-empty string');
    }
    
    this.memories.set(id, {
      id,
      data,
      timestamp: Date.now(),
      tags: Array.isArray(tags) ? tags : []
    });
  }
  
  retrieve(id: string): MemoryItem | undefined {
    if (typeof id !== 'string') {
      return undefined;
    }
    return this.memories.get(id);
  }
  
  findByTag(tag: string): MemoryItem[] {
    if (typeof tag !== 'string') {
      return [];
    }
    return Array.from(this.memories.values())
      .filter(item => item.tags.includes(tag));
  }
  
  async storeStrategy(strategy: StrategyData): Promise<void> {
    if (!strategy || typeof strategy.id !== 'string') {
      throw new Error('Invalid strategy: must have valid ID');
    }
    
    this.strategies.set(strategy.id, strategy);
    await this.persistStrategies();
  }
  
  private async persistStrategies(): Promise<void> {
    try {
      const strategiesFile = path.join(this.memoryDir, 'strategies.json');
      const strategies = Array.from(this.strategies.values());
      fs.writeFileSync(strategiesFile, JSON.stringify(strategies, null, 2));
    } catch (error) {
      // Handle persistence errors gracefully
      console.warn('Failed to persist strategies:', (error as Error).message);
    }
  }
  
  getStrategy(id: string): StrategyData | undefined {
    if (typeof id !== 'string') {
      return undefined;
    }
    return this.strategies.get(id);
  }
  
  async getMemoryAnalytics(): Promise<MemoryAnalytics> {
    const strategies = Array.from(this.strategies.values());
    
    return {
      totalStrategies: strategies.length,
      averageSuccessRate: strategies.length > 0 ? 
        strategies.reduce((sum, s) => sum + s.performance.successRate, 0) / strategies.length : 0,
      totalCost: strategies.reduce((sum, s) => sum + s.performance.totalCost, 0),
      averageCompletionTime: strategies.length > 0 ?
        strategies.reduce((sum, s) => sum + s.performance.completionTime, 0) / strategies.length : 0,
      mostUsedAgents: this.getMostUsedAgents(strategies),
      commonPatterns: this.getCommonPatterns(strategies)
    };
  }
  
  private getMostUsedAgents(strategies: StrategyData[]): string[] {
    const agentCounts = new Map<string, number>();
    
    strategies.forEach(strategy => {
      strategy.aiAgentsUsed.forEach(agent => {
        agentCounts.set(agent, (agentCounts.get(agent) || 0) + 1);
      });
    });
    
    return Array.from(agentCounts.entries())
      .sort((a, b) => b[1] - a[1])
      .slice(0, 5)
      .map(entry => entry[0]);
  }
  
  private getCommonPatterns(strategies: StrategyData[]): string[] {
    const patterns = new Map<string, number>();
    
    strategies.forEach(strategy => {
      strategy.strategy.successfulPatterns.forEach(pattern => {
        patterns.set(pattern, (patterns.get(pattern) || 0) + 1);
      });
    });
    
    return Array.from(patterns.entries())
      .sort((a, b) => b[1] - a[1])
      .slice(0, 10)
      .map(entry => entry[0]);
  }
  
  async askDatabaseAI(query: string): Promise<string> {
    if (typeof query !== 'string') {
      return 'Invalid query';
    }
    
    const strategies = Array.from(this.strategies.values());
    
    if (query.toLowerCase().includes('similar') && strategies.length > 0) {
      const recentFailures = strategies
        .filter(s => s.strategy.failurePatterns.length > 0)
        .slice(-3);
      
      if (recentFailures.length > 0) {
        return 'Found ' + recentFailures.length + ' similar attempts with issues: ' + 
               recentFailures.map(s => s.strategy.failurePatterns.join(', ')).join('; ');
      }
    }
    
    return 'No relevant data found in memory.';
  }
}

export class DatabaseAI {
  constructor(private memory: ContinuumMemory) {
    if (!memory) {
      throw new Error('DatabaseAI requires valid ContinuumMemory instance');
    }
  }
  
  async query(query: string): Promise<string> {
    return this.memory.askDatabaseAI(query);
  }
}

export default ContinuumMemory;
