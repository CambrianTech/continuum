/**
 * @fileoverview Continuum Memory System
 * @description AI memory and strategy storage for intelligent coordination
 */

export interface StrategyData {
  id: string;
  context: string;
  successRate: number;
  lastUsed: number;
}

export interface MemoryItem {
  id: string;
  data: any;
  timestamp: number;
  tags: string[];
}

export class ContinuumMemory {
  private strategies = new Map<string, StrategyData>();
  private memories = new Map<string, MemoryItem>();
  
  constructor(private projectRoot: string) {}
  
  storeStrategy(strategy: StrategyData): void {
    this.strategies.set(strategy.id, strategy);
  }
  
  getStrategy(id: string): StrategyData | undefined {
    return this.strategies.get(id);
  }
  
  store(id: string, data: any, tags: string[] = []): void {
    this.memories.set(id, {
      id,
      data,
      timestamp: Date.now(),
      tags
    });
  }
  
  retrieve(id: string): MemoryItem | undefined {
    return this.memories.get(id);
  }
  
  findByTag(tag: string): MemoryItem[] {
    return Array.from(this.memories.values())
      .filter(item => item.tags.includes(tag));
  }
}

export default ContinuumMemory;
