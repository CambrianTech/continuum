/**
 * System Migrator - Gradually migrate from legacy to elegant OS architecture
 * 
 * MIGRATION STRATEGY:
 * 1. Keep old system 100% functional
 * 2. Add new OS layer as optional overlay
 * 3. Migrate components one by one when ready
 * 4. Gradual cutover with rollback capability
 * 5. Zero downtime, zero breakage
 */

import { ContinuumOS } from '../ContinuumOS.js';
import { LegacyBridgeService } from './LegacyBridgeService.js';

export interface MigrationComponent {
  name: string;
  legacy: {
    module: string;
    entryPoint: string;
    dependencies: string[];
  };
  modern: {
    daemon: string;
    osIntegration: boolean;
    aiReady: boolean;
  };
  migrationState: 'legacy' | 'bridged' | 'migrating' | 'modern' | 'failed';
  rollbackPlan: RollbackPlan;
}

export interface RollbackPlan {
  triggers: string[];
  steps: RollbackStep[];
  timeoutMs: number;
}

export interface RollbackStep {
  action: 'stop-modern' | 'start-legacy' | 'redirect-traffic' | 'restore-data';
  target: string;
  parameters: any;
}

/**
 * System Migrator - Orchestrates gradual migration
 */
export class SystemMigrator {
  private components = new Map<string, MigrationComponent>();
  private legacySystem: any; // The old messy system
  private modernOS: ContinuumOS;
  private bridge: LegacyBridgeService;
  private migrationHistory: MigrationEvent[] = [];

  constructor() {
    this.modernOS = new ContinuumOS();
    this.bridge = new LegacyBridgeService();
    this.setupMigrationComponents();
  }

  /**
   * Start migration process - old system keeps running
   */
  async startMigration(): Promise<void> {
    console.log('🔄 System Migration - Starting gradual migration to Continuum OS');
    
    // 1. Start modern OS in parallel (doesn't interfere with legacy)
    await this.modernOS.boot();
    
    // 2. Start bridge service (translates between old/new)
    await this.bridge.start();
    
    // 3. Begin component-by-component migration
    await this.beginComponentMigration();
    
    console.log('✅ Migration framework active - old system still running');
  }

  /**
   * Migrate specific component when ready
   */
  async migrateComponent(componentName: string): Promise<MigrationResult> {
    const component = this.components.get(componentName);
    if (!component) {
      return { success: false, error: `Component ${componentName} not found` };
    }

    console.log(`🔄 Migrating component: ${componentName}`);
    
    try {
      // Step 1: Start modern version alongside legacy
      await this.startModernComponent(component);
      component.migrationState = 'bridged';
      
      // Step 2: Setup traffic bridging (both systems handle requests)
      await this.setupTrafficBridge(component);
      
      // Step 3: Gradual traffic shift (10% -> 50% -> 90% -> 100%)
      const success = await this.gradualTrafficShift(component);
      
      if (success) {
        // Step 4: Stop legacy version
        await this.stopLegacyComponent(component);
        component.migrationState = 'modern';
        
        this.logMigrationEvent('success', componentName);
        return { success: true, component: componentName };
      } else {
        // Rollback if migration failed
        await this.rollbackComponent(component);
        return { success: false, error: 'Migration failed, rolled back' };
      }
      
    } catch (error) {
      // Auto-rollback on any error
      await this.rollbackComponent(component);
      return { success: false, error: `Migration failed: ${error}` };
    }
  }

  /**
   * Setup migration components - define what can be migrated
   */
  private setupMigrationComponents(): void {
    // Browser coordination - perfect first candidate
    this.components.set('browser-coordinator', {
      name: 'Browser Coordination',
      legacy: {
        module: 'src/core/DevToolsSessionCoordinator.cjs',
        entryPoint: 'getDevToolsCoordinator',
        dependencies: []
      },
      modern: {
        daemon: 'browser-manager',
        osIntegration: true,
        aiReady: true
      },
      migrationState: 'legacy',
      rollbackPlan: {
        triggers: ['error-rate-high', 'performance-degraded'],
        steps: [
          { action: 'redirect-traffic', target: 'legacy', parameters: {} },
          { action: 'stop-modern', target: 'browser-manager', parameters: {} }
        ],
        timeoutMs: 30000
      }
    });

    // Git hook verification
    this.components.set('git-verification', {
      name: 'Git Hook Verification',
      legacy: {
        module: 'quick_commit_check.py',
        entryPoint: 'run_verification',
        dependencies: ['browser-coordinator']
      },
      modern: {
        daemon: 'verification-service',
        osIntegration: true,
        aiReady: false // Not AI ready yet
      },
      migrationState: 'legacy',
      rollbackPlan: {
        triggers: ['verification-failed', 'timeout'],
        steps: [
          { action: 'start-legacy', target: 'quick_commit_check.py', parameters: {} },
          { action: 'stop-modern', target: 'verification-service', parameters: {} }
        ],
        timeoutMs: 60000
      }
    });

    // Portal session management
    this.components.set('portal-sessions', {
      name: 'Portal Session Management',
      legacy: {
        module: 'python-client/ai-portal.py',
        entryPoint: 'session_manager',
        dependencies: ['browser-coordinator']
      },
      modern: {
        daemon: 'session-manager',
        osIntegration: true,
        aiReady: true
      },
      migrationState: 'legacy',
      rollbackPlan: {
        triggers: ['portal-unresponsive', 'session-failures'],
        steps: [
          { action: 'redirect-traffic', target: 'legacy', parameters: {} },
          { action: 'restore-data', target: 'session-state', parameters: {} }
        ],
        timeoutMs: 15000
      }
    });
  }

  /**
   * Gradual traffic shift with monitoring
   */
  private async gradualTrafficShift(component: MigrationComponent): Promise<boolean> {
    const shiftPhases = [10, 25, 50, 75, 90, 100];
    
    for (const percentage of shiftPhases) {
      console.log(`📊 Shifting ${percentage}% traffic to modern ${component.name}`);
      
      // Update traffic routing
      await this.bridge.setTrafficSplit(component.name, percentage);
      
      // Monitor for 30 seconds
      const phaseSuccess = await this.monitorPhase(component, percentage);
      
      if (!phaseSuccess) {
        console.log(`❌ Phase ${percentage}% failed, rolling back`);
        return false;
      }
      
      // Wait between phases
      await this.sleep(10000);
    }
    
    return true;
  }

  /**
   * Monitor migration phase for success/failure
   */
  private async monitorPhase(component: MigrationComponent, percentage: number): Promise<boolean> {
    const startTime = Date.now();
    const monitorDuration = 30000; // 30 seconds
    
    while (Date.now() - startTime < monitorDuration) {
      // Check error rates
      const errorRate = await this.bridge.getErrorRate(component.name);
      if (errorRate > 0.05) { // More than 5% errors
        return false;
      }
      
      // Check performance
      const responseTime = await this.bridge.getAverageResponseTime(component.name);
      if (responseTime > 5000) { // More than 5 seconds
        return false;
      }
      
      // Check for rollback triggers
      const triggers = await this.checkRollbackTriggers(component);
      if (triggers.length > 0) {
        console.log(`🚨 Rollback triggered: ${triggers.join(', ')}`);
        return false;
      }
      
      await this.sleep(1000);
    }
    
    return true;
  }

  /**
   * Rollback component to legacy system
   */
  private async rollbackComponent(component: MigrationComponent): Promise<void> {
    console.log(`🔄 Rolling back ${component.name} to legacy system`);
    
    component.migrationState = 'failed';
    
    for (const step of component.rollbackPlan.steps) {
      try {
        await this.executeRollbackStep(step);
      } catch (error) {
        console.error(`❌ Rollback step failed: ${error}`);
      }
    }
    
    component.migrationState = 'legacy';
    this.logMigrationEvent('rollback', component.name);
  }

  /**
   * Get migration status
   */
  getMigrationStatus(): MigrationStatus {
    const total = this.components.size;
    const migrated = Array.from(this.components.values())
      .filter(c => c.migrationState === 'modern').length;
    const inProgress = Array.from(this.components.values())
      .filter(c => c.migrationState === 'migrating').length;
    const failed = Array.from(this.components.values())
      .filter(c => c.migrationState === 'failed').length;
    
    return {
      total,
      migrated,
      inProgress,
      failed,
      percentage: Math.round((migrated / total) * 100),
      components: Array.from(this.components.values())
    };
  }

  /**
   * Migration command interface
   */
  async executeMigrationCommand(command: string, target?: string): Promise<any> {
    switch (command) {
      case 'status':
        return this.getMigrationStatus();
      
      case 'migrate':
        if (!target) throw new Error('Target component required');
        return await this.migrateComponent(target);
      
      case 'rollback':
        if (!target) throw new Error('Target component required');
        const component = this.components.get(target);
        if (component) await this.rollbackComponent(component);
        return { success: true };
      
      case 'list':
        return Array.from(this.components.keys());
      
      case 'auto':
        return await this.autoMigrate();
      
      default:
        throw new Error(`Unknown migration command: ${command}`);
    }
  }

  /**
   * Auto-migrate ready components
   */
  private async autoMigrate(): Promise<void> {
    // Start with safest components first
    const migrationOrder = ['browser-coordinator', 'portal-sessions', 'git-verification'];
    
    for (const componentName of migrationOrder) {
      const component = this.components.get(componentName);
      if (component && component.migrationState === 'legacy') {
        console.log(`🤖 Auto-migrating ${componentName}`);
        await this.migrateComponent(componentName);
        
        // Wait between migrations
        await this.sleep(60000);
      }
    }
  }

  // Helper methods
  private async startModernComponent(component: MigrationComponent): Promise<void> {
    await this.modernOS.handleRequest({
      type: 'spawn-process',
      payload: { daemon: component.modern.daemon },
      priority: 'normal'
    });
  }

  private async stopLegacyComponent(component: MigrationComponent): Promise<void> {
    // Gracefully stop legacy component
  }

  private async setupTrafficBridge(component: MigrationComponent): Promise<void> {
    await this.bridge.setupComponentBridge(component);
  }

  private async checkRollbackTriggers(component: MigrationComponent): Promise<string[]> {
    return []; // Implementation would check actual triggers
  }

  private async executeRollbackStep(step: RollbackStep): Promise<void> {
    // Implementation would execute the specific rollback step
  }

  private logMigrationEvent(type: string, component: string): void {
    this.migrationHistory.push({
      type,
      component,
      timestamp: new Date()
    });
  }

  private sleep(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
  }
}

// Supporting types
interface MigrationResult {
  success: boolean;
  component?: string;
  error?: string;
}

interface MigrationStatus {
  total: number;
  migrated: number;
  inProgress: number;
  failed: number;
  percentage: number;
  components: MigrationComponent[];
}

interface MigrationEvent {
  type: string;
  component: string;
  timestamp: Date;
}