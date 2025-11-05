/**
 * Generic Priority Queue - Reusable for any message type
 */

import type { TimerHandle } from '../../../types/CrossPlatformTypes';

export enum Priority {
  CRITICAL = 0,
  HIGH = 1,
  NORMAL = 2,
  LOW = 3
}

export interface QueuedItem<T> {
  item: T;
  priority: Priority;
  timestamp: number;
  retryCount: number;
  maxRetries: number;
  id: string;
}

export interface QueueConfig {
  maxSize: number;
  maxRetries: number;
  flushInterval: number;
  persistenceKey?: string;
}

export class PriorityQueue<T> {
  private queue: QueuedItem<T>[] = [];
  private config: QueueConfig;
  private processing = false;
  private flushTimer?: TimerHandle;

  constructor(config: Partial<QueueConfig> = {}) {
    this.config = {
      maxSize: 1000,
      maxRetries: 3,
      flushInterval: 500,
      ...config
    };
  }

  /**
   * Add item to queue with priority ordering
   */
  enqueue(item: T, priority: Priority = Priority.NORMAL): string {
    const queuedItem: QueuedItem<T> = {
      item,
      priority,
      timestamp: Date.now(),
      retryCount: 0,
      maxRetries: this.config.maxRetries,
      id: `item_${Date.now()}_${Math.random().toString(36).substr(2, 6)}`
    };

    // Insert based on priority (lower number = higher priority)
    const insertIndex = this.queue.findIndex(queued => queued.priority > priority);
    if (insertIndex === -1) {
      this.queue.push(queuedItem);
    } else {
      this.queue.splice(insertIndex, 0, queuedItem);
    }

    // Maintain queue size
    while (this.queue.length > this.config.maxSize) {
      const lowestPriority = Math.max(...this.queue.map(q => q.priority));
      const removeIndex = this.queue.findIndex(q => q.priority === lowestPriority);
      if (removeIndex !== -1) {
        this.queue.splice(removeIndex, 1);
      }
    }

    return queuedItem.id;
  }

  /**
   * Start processing queue with flush handler
   */
  startProcessing(flushHandler: (items: QueuedItem<T>[]) => Promise<QueuedItem<T>[]>): void {
    if (this.processing) return;
    
    this.processing = true;
    
    this.flushTimer = setInterval(async () => {
      if (this.queue.length === 0) return;

      // FAIR PRIORITY SELECTION: Use percentages to prevent starvation
      const itemsToFlush = this.selectItemsForFairFlush();
      
      // Remove selected items from queue
      this.queue = this.queue.filter(item => !itemsToFlush.includes(item));

      try {
        const failedItems = await flushHandler(itemsToFlush);
        
        // Re-queue failed items with retry logic
        failedItems.forEach(failed => {
          failed.retryCount++;
          if (failed.retryCount <= failed.maxRetries) {
            const delay = Math.min(1000 * Math.pow(2, failed.retryCount), 30000);
            setTimeout(() => {
              this.queue.unshift(failed);
            }, delay);
          }
        });

      } catch (error) {
        // Re-queue all items on handler error
        this.queue = [...itemsToFlush, ...this.queue];
      }
    }, this.config.flushInterval);
  }

  /**
   * Select items for fair flush using stochastic scheduling with percentages and age-based promotion
   * Implements classic computer science fair scheduling algorithms with randomization
   */
  private selectItemsForFairFlush(): QueuedItem<T>[] {
    if (this.queue.length === 0) return [];
    
    const now = Date.now();
    const AGE_PROMOTION_THRESHOLD = 30000; // 30 seconds
    const MAX_FLUSH_SIZE = 20; // Process max 20 items per flush
    
    // Age-based promotion: boost priority of old messages
    this.queue.forEach(item => {
      const age = now - item.timestamp;
      if (age > AGE_PROMOTION_THRESHOLD && item.priority > Priority.CRITICAL) {
        console.log(`⏰ Queue: Promoting aged message (${age}ms old) from ${Priority[item.priority]} to HIGH`);
        item.priority = Priority.HIGH;
      }
    });
    
    // STOCHASTIC FAIR ALLOCATION with percentage guarantees
    const totalItems = Math.min(this.queue.length, MAX_FLUSH_SIZE);
    
    // Base allocation percentages (ensuring fairness guarantees)
    const allocations = {
      [Priority.CRITICAL]: Math.max(1, Math.floor(totalItems * 0.4)), // 40%
      [Priority.HIGH]: Math.max(1, Math.floor(totalItems * 0.4)),     // 40% 
      [Priority.NORMAL]: Math.max(1, Math.floor(totalItems * 0.15)),  // 15%
      [Priority.LOW]: Math.max(1, Math.floor(totalItems * 0.05))      // 5%
    };
    
    const selected: QueuedItem<T>[] = [];
    const priorityGroups = this.groupByPriority();
    
    // STOCHASTIC SELECTION: Randomize within each priority band
    for (const [priority, allocation] of Object.entries(allocations)) {
      const priorityNum = parseInt(priority) as Priority;
      const availableItems = priorityGroups[priorityNum] || [];
      
      if (availableItems.length === 0) continue;
      
      // Sort by age within priority band, then apply stochastic selection
      availableItems.sort((a, b) => a.timestamp - b.timestamp); // Older first
      
      const takeCount = Math.min(allocation, availableItems.length);
      
      if (takeCount >= availableItems.length) {
        // Take all if allocation >= available
        selected.push(...availableItems);
      } else {
        // STOCHASTIC SELECTION: Weighted random selection favoring older messages
        const stochasticSelection = this.selectStochastically(availableItems, takeCount);
        selected.push(...stochasticSelection);
      }
      
      if (takeCount > 0) {
        console.log(`🎲 Queue: Stochastically selected ${takeCount}/${availableItems.length} ${Priority[priorityNum]} priority items`);
      }
    }
    
    return selected.slice(0, totalItems);
  }
  
  /**
   * Stochastic selection with age-weighted probability that grows over time
   * Ensures eventual delivery through exponential age weighting
   */
  private selectStochastically(items: QueuedItem<T>[], count: number): QueuedItem<T>[] {
    if (count >= items.length) return items;
    
    const now = Date.now();
    const selected: QueuedItem<T>[] = [];
    const available = [...items]; // Copy to avoid mutation
    
    // Calculate age-weighted probabilities for each item
    const calculateSelectionProbability = (item: QueuedItem<T>): number => {
      const age = now - item.timestamp;
      const ageSeconds = age / 1000;
      
      // EXPONENTIAL AGE WEIGHTING: Probability grows exponentially with age
      // Formula: P(age) = base + (age_factor * (1 - e^(-growth_rate * age)))
      // This ensures very old messages eventually get probability near 1.0
      
      const baseProb = 0.1;           // Minimum 10% chance for new messages
      const ageFactor = 0.9;          // Maximum additional 90% from age weighting  
      const growthRate = 0.002;       // Growth rate per second (tunable)
      
      // Exponential growth: P approaches baseProb + ageFactor as age → ∞
      const ageWeight = ageFactor * (1 - Math.exp(-growthRate * ageSeconds));
      const probability = Math.min(1.0, baseProb + ageWeight);
      
      return probability;
    };
    
    // Select items stochastically using weighted random selection
    for (let i = 0; i < count && available.length > 0; i++) {
      // Calculate selection weights for all remaining items
      const weights = available.map(item => ({
        item,
        weight: calculateSelectionProbability(item)
      }));
      
      // Weighted random selection
      const totalWeight = weights.reduce((sum, w) => sum + w.weight, 0);
      let randomValue = Math.random() * totalWeight;
      
      let selectedIndex = 0;
      for (let j = 0; j < weights.length; j++) {
        randomValue -= weights[j].weight;
        if (randomValue <= 0) {
          selectedIndex = j;
          break;
        }
      }
      
      // Select the item and remove from available pool
      const selectedItem = available[selectedIndex];
      selected.push(selectedItem);
      available.splice(selectedIndex, 1);
      
      // Log selection for very old messages (debugging)
      const age = now - selectedItem.timestamp;
      if (age > 15000) { // Log if older than 15 seconds
        const probability = calculateSelectionProbability(selectedItem);
        console.log(`🎯 Queue: Stochastic selection - aged item (${Math.round(age/1000)}s) with P=${probability.toFixed(3)}`);
      }
    }
    
    return selected;
  }

  /**
   * Group queue items by priority for fair allocation
   */
  private groupByPriority(): Record<Priority, QueuedItem<T>[]> {
    const groups: Record<Priority, QueuedItem<T>[]> = {
      [Priority.CRITICAL]: [],
      [Priority.HIGH]: [],
      [Priority.NORMAL]: [],
      [Priority.LOW]: []
    };
    
    this.queue.forEach(item => {
      groups[item.priority].push(item);
    });
    
    return groups;
  }

  /**
   * Stop processing
   */
  stopProcessing(): void {
    if (this.flushTimer) {
      clearInterval(this.flushTimer);
      this.flushTimer = undefined;
    }
    this.processing = false;
  }

  /**
   * Get queue status
   */
  get status() {
    const priorityBreakdown = {} as Record<Priority, number>;
    Object.values(Priority).forEach(priority => {
      if (typeof priority === 'number') {
        priorityBreakdown[priority] = this.queue.filter(q => q.priority === priority).length;
      }
    });

    return {
      size: this.queue.length,
      processing: this.processing,
      priorityBreakdown
    };
  }

  /**
   * Clear queue
   */
  clear(): void {
    this.queue = [];
  }
}