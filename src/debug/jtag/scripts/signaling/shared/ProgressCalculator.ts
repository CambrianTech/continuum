/**
 * Progress Calculator
 * 
 * Handles progress calculation and progress bar visualization for system readiness milestones.
 * Supports configurable milestone sets and flexible display options.
 */

import { SystemReadySignal, MilestoneConfig, SystemMilestone, ProgressInfo } from './SystemSignalingTypes';

export class ProgressCalculator {
  constructor(private config: MilestoneConfig) {}

  // Calculate progress across core milestones only (for display consistency with original)
  calculateProgress(signal: SystemReadySignal): ProgressInfo {
    // Only use core milestones for progress calculation to match original behavior
    const coreMilestones = this.config.core || [];
    const milestonesWithStatus = coreMilestones.map(milestone => ({
      milestone,
      ready: milestone.checkFn(signal)
    }));

    const completed = milestonesWithStatus.filter(m => m.ready).length;
    const requiredMilestones = milestonesWithStatus.filter(m => m.milestone.required);
    const requiredCompleted = requiredMilestones.filter(m => m.ready).length;

    const details = milestonesWithStatus.map(({ milestone, ready }) => 
      `${milestone.name}${ready ? '✅' : '⏳'}`
    );

    return {
      completed,
      total: coreMilestones.length,
      requiredCompleted,
      requiredTotal: requiredMilestones.length,
      details,
      milestones: milestonesWithStatus
    };
  }

  // Create visual progress bar
  createProgressBar(completed: number, total: number, width: number = 20): string {
    const filledWidth = Math.round((completed / total) * width);
    const emptyWidth = width - filledWidth;
    
    const filled = '█'.repeat(filledWidth);
    const empty = '░'.repeat(emptyWidth);
    
    return `[${filled}${empty}]`;
  }

  // Generate detailed progress display
  formatProgressDisplay(signal: SystemReadySignal, options?: {
    showRequired?: boolean;
    showDetailed?: boolean;
  }): string {
    const progress = this.calculateProgress(signal);
    const progressBar = this.createProgressBar(progress.completed, progress.total);
    const percentage = Math.round((progress.completed / progress.total) * 100);

    let output = `🔄 Progress: ${progressBar} ${percentage}%\n`;
    output += `   ${progress.completed}/${progress.total} systems ready`;

    if (options?.showRequired && progress.requiredTotal > 0) {
      const requiredPercentage = Math.round((progress.requiredCompleted / progress.requiredTotal) * 100);
      output += ` (${progress.requiredCompleted}/${progress.requiredTotal} required: ${requiredPercentage}%)`;
    }

    if (options?.showDetailed) {
      output += `\n   ${progress.details.join(', ')}`;
    }

    return output;
  }

  // Check if system meets readiness criteria (matches original logic)
  isSystemReady(signal: SystemReadySignal): {
    fullyHealthy: boolean;
    functionallyReady: boolean;
    hasErrors: boolean;
  } {
    // Match original logic exactly for compatibility
    const fullyHealthy = signal.systemHealth === 'healthy' && 
                         signal.bootstrapComplete && 
                         signal.commandCount > 0 &&
                         signal.browserReady;
    
    const functionallyReady = signal.systemHealth === 'degraded' && 
                              signal.bootstrapComplete && 
                              signal.commandCount > 0 &&
                              (signal.portsActive?.length || 0) >= 2;
                              
    const hasErrors = signal.systemHealth === 'error';

    return { fullyHealthy, functionallyReady, hasErrors };
  }

  // Get all milestones from config
  private getAllMilestones(): SystemMilestone[] {
    return [
      ...(this.config.core || []),
      ...(this.config.performance || []),
      ...(this.config.integration || []),
      ...(this.config.custom || [])
    ];
  }

  // Get milestones by category
  getMilestonesByCategory(category: keyof MilestoneConfig): SystemMilestone[] {
    return this.config[category] || [];
  }

  // Update configuration (for dynamic milestone addition)
  updateConfig(newConfig: MilestoneConfig): void {
    this.config = newConfig;
  }
}