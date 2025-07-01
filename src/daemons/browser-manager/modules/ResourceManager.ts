/**
 * Resource Manager - Focused module for browser resource monitoring and optimization
 * Handles memory/CPU tracking, performance optimization, and resource cleanup
 */

import { DaemonManagedBrowser } from '../types/index.js';

export interface ProcessStats {
  memory: number;
  cpu: number;
}

export interface ResourceMetrics {
  totalMemory: number;
  totalCpu: number;
  browserCount: number;
  tabCount: number;
  averageMemoryPerBrowser: number;
  averageCpuPerBrowser: number;
}

export interface OptimizationResult {
  browsersOptimized: number;
  memoryFreed: number;
  tabsClosed: number;
  actions: string[];
}

export class ResourceManager {
  private monitorInterval: NodeJS.Timeout | null = null;
  private browsers: Map<string, DaemonManagedBrowser>;

  constructor(browsers: Map<string, DaemonManagedBrowser>) {
    this.browsers = browsers;
  }

  /**
   * Start resource monitoring
   */
  startMonitoring(intervalMs: number = 30000): void {
    if (this.monitorInterval) {
      this.stopMonitoring();
    }

    this.monitorInterval = setInterval(async () => {
      await this.updateAllResourceUsage();
      await this.performAutomaticOptimizations();
    }, intervalMs);

    console.log(`🔍 Resource monitoring started (${intervalMs}ms interval)`);
  }

  /**
   * Stop resource monitoring
   */
  stopMonitoring(): void {
    if (this.monitorInterval) {
      clearInterval(this.monitorInterval);
      this.monitorInterval = null;
      console.log('🛑 Resource monitoring stopped');
    }
  }

  /**
   * Update resource usage for all browsers
   */
  async updateAllResourceUsage(): Promise<void> {
    const promises = Array.from(this.browsers.values()).map(browser => 
      this.updateBrowserResourceUsage(browser)
    );

    await Promise.allSettled(promises);
  }

  /**
   * Update resource usage for a specific browser
   */
  async updateBrowserResourceUsage(browser: DaemonManagedBrowser): Promise<void> {
    try {
      // Get process stats
      const stats = await this.getProcessStats(browser.pid);
      browser.resources.memory = stats.memory;
      browser.resources.cpu = stats.cpu;
      
      // Get tab count from DevTools
      const tabs = await this.getTabCount(browser.port);
      browser.resources.tabs = tabs;
      
      // Update last activity
      browser.lastActivity = new Date();
      
    } catch (error) {
      console.log(`Failed to update stats for browser ${browser.id}: ${error}`);
    }
  }

  /**
   * Get process statistics
   */
  async getProcessStats(pid: number): Promise<ProcessStats> {
    try {
      // For now, return mock data
      // TODO: Implement real process monitoring using ps or similar
      return {
        memory: Math.floor(Math.random() * 500) + 100, // MB
        cpu: Math.floor(Math.random() * 30) + 5 // %
      };
    } catch (error) {
      console.log(`Failed to get process stats for PID ${pid}: ${error}`);
      return { memory: 0, cpu: 0 };
    }
  }

  /**
   * Get tab count from DevTools
   */
  async getTabCount(port: number): Promise<number> {
    try {
      if (port === 0) return 1; // System default browser has 1 tab

      const response = await fetch(`http://localhost:${port}/json/list`, {
        signal: AbortSignal.timeout(2000)
      });
      
      if (response.ok) {
        const tabs = await response.json();
        return Array.isArray(tabs) ? tabs.length : 1;
      }
    } catch (error) {
      // DevTools not available, assume 1 tab
    }
    
    return 1;
  }

  /**
   * Get overall resource metrics
   */
  getResourceMetrics(): ResourceMetrics {
    const browsers = Array.from(this.browsers.values());
    
    const totalMemory = browsers.reduce((sum, b) => sum + b.resources.memory, 0);
    const totalCpu = browsers.reduce((sum, b) => sum + b.resources.cpu, 0);
    const tabCount = browsers.reduce((sum, b) => sum + b.resources.tabs, 0);

    return {
      totalMemory,
      totalCpu,
      browserCount: browsers.length,
      tabCount,
      averageMemoryPerBrowser: browsers.length > 0 ? totalMemory / browsers.length : 0,
      averageCpuPerBrowser: browsers.length > 0 ? totalCpu / browsers.length : 0
    };
  }

  /**
   * Perform automatic optimizations
   */
  async performAutomaticOptimizations(): Promise<OptimizationResult> {
    const actions: string[] = [];
    let browsersOptimized = 0;
    let memoryFreed = 0;
    let tabsClosed = 0;

    // Close idle browsers
    for (const browser of this.browsers.values()) {
      if (this.isIdle(browser) && this.canClose(browser)) {
        const memoryBefore = browser.resources.memory;
        await this.closeBrowser(browser.id);
        memoryFreed += memoryBefore;
        browsersOptimized++;
        actions.push(`Closed idle browser ${browser.id}`);
      }
    }

    // Consolidate underutilized browsers
    const consolidationResult = await this.consolidateResources();
    browsersOptimized += consolidationResult.browsersAffected;
    tabsClosed += consolidationResult.tabsClosed;
    actions.push(...consolidationResult.actions);

    if (actions.length > 0) {
      console.log(`🧹 Automatic optimization completed: ${actions.join(', ')}`);
    }

    return {
      browsersOptimized,
      memoryFreed,
      tabsClosed,
      actions
    };
  }

  /**
   * Consolidate underutilized browsers
   */
  async consolidateResources(): Promise<{ browsersAffected: number; tabsClosed: number; actions: string[] }> {
    // Placeholder for browser consolidation logic
    // TODO: Implement browser tab consolidation, memory optimization
    return {
      browsersAffected: 0,
      tabsClosed: 0,
      actions: []
    };
  }

  /**
   * Check if browser is idle
   */
  isIdle(browser: DaemonManagedBrowser): boolean {
    const idleThreshold = 5 * 60 * 1000; // 5 minutes
    return Date.now() - browser.lastActivity.getTime() > idleThreshold &&
           browser.sessions.size === 0;
  }

  /**
   * Check if browser can be closed
   */
  canClose(browser: DaemonManagedBrowser): boolean {
    return browser.config.requirements.persistence === 'ephemeral';
  }

  /**
   * Close browser (placeholder)
   */
  async closeBrowser(browserId: string): Promise<void> {
    const browser = this.browsers.get(browserId);
    if (browser) {
      // TODO: Implement graceful browser shutdown
      this.browsers.delete(browserId);
      console.log(`🔒 Browser ${browserId} closed`);
    }
  }

  /**
   * Get resource limits and thresholds
   */
  getResourceLimits() {
    return {
      maxBrowsers: 5,
      maxTabsPerBrowser: 10,
      memoryWarningThreshold: 1000, // MB
      cpuWarningThreshold: 80, // %
      idleTimeoutMs: 5 * 60 * 1000 // 5 minutes
    };
  }

  /**
   * Check if system is under resource pressure
   */
  isUnderResourcePressure(): boolean {
    const metrics = this.getResourceMetrics();
    const limits = this.getResourceLimits();
    
    return metrics.totalMemory > limits.memoryWarningThreshold ||
           metrics.totalCpu > limits.cpuWarningThreshold ||
           metrics.browserCount >= limits.maxBrowsers;
  }
}