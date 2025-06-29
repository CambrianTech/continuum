/**
 * Legacy Bridge Service - Traffic routing and translation between old and new systems
 * 
 * BRIDGE RESPONSIBILITIES:
 * 1. Route traffic between legacy and modern systems during migration
 * 2. Translate API calls from old format to new daemon messages
 * 3. Monitor error rates and performance during migration phases
 * 4. Provide rollback capabilities when migration fails
 * 5. Enable zero-downtime component migration
 */

import { EventEmitter } from 'events';
import { DaemonMessage, DaemonResponse } from '../../daemons/base/DaemonProtocol.js';
import { MigrationComponent } from './SystemMigrator.js';

export interface TrafficSplit {
  componentName: string;
  legacyPercentage: number;
  modernPercentage: number;
  startTime: Date;
  metrics: SplitMetrics;
}

export interface SplitMetrics {
  legacyRequests: number;
  modernRequests: number;
  legacyErrors: number;
  modernErrors: number;
  legacyAvgResponseTime: number;
  modernAvgResponseTime: number;
  lastUpdated: Date;
}

export interface LegacyRequest {
  id: string;
  component: string;
  method: string;
  parameters: any;
  timestamp: Date;
  source: 'git-hook' | 'portal' | 'api' | 'unknown';
}

export interface ModernRequest {
  id: string;
  component: string;
  daemon: string;
  message: DaemonMessage;
  timestamp: Date;
}

/**
 * Legacy Bridge Service - Seamless migration traffic routing
 */
export class LegacyBridgeService extends EventEmitter {
  private trafficSplits = new Map<string, TrafficSplit>();
  private componentBridges = new Map<string, ComponentBridge>();
  private metricsHistory: SplitMetrics[] = [];
  private monitoringActive = false;

  constructor() {
    super();
  }

  /**
   * Start the bridge service
   */
  async start(): Promise<void> {
    this.log('🌉 Legacy Bridge Service - Starting traffic routing');
    
    // Initialize default traffic splits (100% legacy initially)
    await this.initializeDefaultSplits();
    
    // Start metrics monitoring
    this.startMetricsMonitoring();
    
    this.monitoringActive = true;
    this.emit('bridge-started');
    
    this.log('✅ Legacy Bridge Service - Ready for traffic routing');
  }

  /**
   * Stop the bridge service
   */
  async stop(): Promise<void> {
    this.monitoringActive = false;
    this.log('🌉 Legacy Bridge Service - Stopping');
    this.emit('bridge-stopped');
  }

  /**
   * Setup component bridge for migration
   */
  async setupComponentBridge(component: MigrationComponent): Promise<void> {
    this.log(`🔗 Setting up bridge for component: ${component.name}`);
    
    const bridge = new ComponentBridge(component, this);
    this.componentBridges.set(component.name, bridge);
    
    // Initialize traffic split (start with 100% legacy)
    await this.setTrafficSplit(component.name, 0);
    
    this.log(`✅ Bridge setup complete for ${component.name}`);
  }

  /**
   * Set traffic split percentage for a component
   */
  async setTrafficSplit(componentName: string, modernPercentage: number): Promise<void> {
    const legacyPercentage = 100 - modernPercentage;
    
    this.log(`📊 Setting traffic split for ${componentName}: ${legacyPercentage}% legacy, ${modernPercentage}% modern`);
    
    const split: TrafficSplit = {
      componentName,
      legacyPercentage,
      modernPercentage,
      startTime: new Date(),
      metrics: {
        legacyRequests: 0,
        modernRequests: 0,
        legacyErrors: 0,
        modernErrors: 0,
        legacyAvgResponseTime: 0,
        modernAvgResponseTime: 0,
        lastUpdated: new Date()
      }
    };
    
    this.trafficSplits.set(componentName, split);
    this.emit('traffic-split-updated', split);
  }

  /**
   * Route request based on traffic split configuration
   */
  async routeRequest(request: LegacyRequest): Promise<any> {
    const split = this.trafficSplits.get(request.component);
    if (!split) {
      // No split configured, use legacy by default
      return await this.routeToLegacy(request);
    }

    // Determine routing based on percentage
    const useModern = Math.random() * 100 < split.modernPercentage;
    
    if (useModern) {
      // Route to modern system
      split.metrics.modernRequests++;
      return await this.routeToModern(request);
    } else {
      // Route to legacy system
      split.metrics.legacyRequests++;
      return await this.routeToLegacy(request);
    }
  }

  /**
   * Get error rate for a component during migration
   */
  async getErrorRate(componentName: string): Promise<number> {
    const split = this.trafficSplits.get(componentName);
    if (!split) return 0;

    const { metrics } = split;
    const totalRequests = metrics.legacyRequests + metrics.modernRequests;
    const totalErrors = metrics.legacyErrors + metrics.modernErrors;
    
    return totalRequests > 0 ? totalErrors / totalRequests : 0;
  }

  /**
   * Get average response time for a component
   */
  async getAverageResponseTime(componentName: string): Promise<number> {
    const split = this.trafficSplits.get(componentName);
    if (!split) return 0;

    const { metrics } = split;
    const legacyWeight = metrics.legacyRequests / (metrics.legacyRequests + metrics.modernRequests);
    const modernWeight = 1 - legacyWeight;
    
    return (metrics.legacyAvgResponseTime * legacyWeight) + 
           (metrics.modernAvgResponseTime * modernWeight);
  }

  /**
   * Route request to legacy system
   */
  private async routeToLegacy(request: LegacyRequest): Promise<any> {
    const startTime = Date.now();
    
    try {
      // Route based on component type
      let result;
      switch (request.component) {
        case 'browser-coordinator':
          result = await this.callLegacyBrowserCoordinator(request);
          break;
        
        case 'git-verification':
          result = await this.callLegacyGitVerification(request);
          break;
        
        case 'portal-sessions':
          result = await this.callLegacyPortalSessions(request);
          break;
        
        default:
          throw new Error(`Unknown legacy component: ${request.component}`);
      }
      
      // Update metrics
      const responseTime = Date.now() - startTime;
      this.updateLegacyMetrics(request.component, responseTime, false);
      
      return result;
      
    } catch (error) {
      // Update error metrics
      const responseTime = Date.now() - startTime;
      this.updateLegacyMetrics(request.component, responseTime, true);
      throw error;
    }
  }

  /**
   * Route request to modern system
   */
  private async routeToModern(request: LegacyRequest): Promise<any> {
    const startTime = Date.now();
    
    try {
      // Translate legacy request to modern daemon message
      const modernRequest = await this.translateToModern(request);
      
      // Send to appropriate daemon
      const result = await this.sendToDaemon(modernRequest);
      
      // Update metrics
      const responseTime = Date.now() - startTime;
      this.updateModernMetrics(request.component, responseTime, false);
      
      return result;
      
    } catch (error) {
      // Update error metrics
      const responseTime = Date.now() - startTime;
      this.updateModernMetrics(request.component, responseTime, true);
      throw error;
    }
  }

  /**
   * Translate legacy request to modern daemon message
   */
  private async translateToModern(request: LegacyRequest): Promise<ModernRequest> {
    const bridge = this.componentBridges.get(request.component);
    if (!bridge) {
      throw new Error(`No bridge configured for component: ${request.component}`);
    }

    return await bridge.translateRequest(request);
  }

  /**
   * Send request to modern daemon
   */
  private async sendToDaemon(modernRequest: ModernRequest): Promise<any> {
    // This would integrate with Continuum OS's daemon messaging system
    // For now, return a success response
    return {
      success: true,
      data: `Modern daemon response for ${modernRequest.component}`,
      source: 'modern-daemon'
    };
  }

  /**
   * Initialize default traffic splits for all known components
   */
  private async initializeDefaultSplits(): Promise<void> {
    const components = ['browser-coordinator', 'git-verification', 'portal-sessions'];
    
    for (const component of components) {
      await this.setTrafficSplit(component, 0); // Start with 100% legacy
    }
  }

  /**
   * Start metrics monitoring
   */
  private startMetricsMonitoring(): void {
    setInterval(() => {
      if (this.monitoringActive) {
        this.updateMetricsSnapshot();
        this.cleanupOldMetrics();
      }
    }, 30000); // Every 30 seconds
  }

  /**
   * Update metrics snapshot for historical analysis
   */
  private updateMetricsSnapshot(): void {
    for (const split of this.trafficSplits.values()) {
      split.metrics.lastUpdated = new Date();
      this.metricsHistory.push({ ...split.metrics });
    }
  }

  /**
   * Clean up old metrics to prevent memory growth
   */
  private cleanupOldMetrics(): void {
    // Keep only last 1000 metric snapshots
    if (this.metricsHistory.length > 1000) {
      this.metricsHistory = this.metricsHistory.slice(-500);
    }
  }

  /**
   * Update legacy system metrics
   */
  private updateLegacyMetrics(component: string, responseTime: number, isError: boolean): void {
    const split = this.trafficSplits.get(component);
    if (!split) return;

    if (isError) {
      split.metrics.legacyErrors++;
    }

    // Update average response time
    const totalRequests = split.metrics.legacyRequests;
    split.metrics.legacyAvgResponseTime = 
      (split.metrics.legacyAvgResponseTime * (totalRequests - 1) + responseTime) / totalRequests;
  }

  /**
   * Update modern system metrics
   */
  private updateModernMetrics(component: string, responseTime: number, isError: boolean): void {
    const split = this.trafficSplits.get(component);
    if (!split) return;

    if (isError) {
      split.metrics.modernErrors++;
    }

    // Update average response time
    const totalRequests = split.metrics.modernRequests;
    split.metrics.modernAvgResponseTime = 
      (split.metrics.modernAvgResponseTime * (totalRequests - 1) + responseTime) / totalRequests;
  }

  /**
   * Legacy system integration methods
   */
  private async callLegacyBrowserCoordinator(request: LegacyRequest): Promise<any> {
    // Integration with existing DevToolsSessionCoordinator.cjs
    return { success: true, source: 'legacy-browser-coordinator' };
  }

  private async callLegacyGitVerification(request: LegacyRequest): Promise<any> {
    // Integration with quick_commit_check.py
    return { success: true, source: 'legacy-git-verification' };
  }

  private async callLegacyPortalSessions(request: LegacyRequest): Promise<any> {
    // Integration with ai-portal.py
    return { success: true, source: 'legacy-portal-sessions' };
  }

  private log(message: string): void {
    console.log(`[${new Date().toISOString()}] [LegacyBridge] ${message}`);
  }
}

/**
 * Component Bridge - Handles translation between legacy and modern APIs
 */
class ComponentBridge {
  constructor(
    private component: MigrationComponent,
    private bridgeService: LegacyBridgeService
  ) {}

  /**
   * Translate legacy request to modern daemon message
   */
  async translateRequest(request: LegacyRequest): Promise<ModernRequest> {
    const daemonMessage: DaemonMessage = {
      id: `bridge-${request.id}`,
      from: 'legacy-bridge',
      to: this.component.modern.daemon,
      type: this.translateRequestType(request.method),
      data: this.translateParameters(request.parameters),
      timestamp: new Date()
    };

    return {
      id: request.id,
      component: request.component,
      daemon: this.component.modern.daemon,
      message: daemonMessage,
      timestamp: request.timestamp
    };
  }

  /**
   * Translate legacy method names to modern daemon message types
   */
  private translateRequestType(legacyMethod: string): string {
    // Translation mapping from legacy API to modern daemon messages
    const typeMap: Record<string, string> = {
      'createSession': 'create-session',
      'getSession': 'get-session',
      'closeSession': 'close-session',
      'takeScreenshot': 'screenshot',
      'executeJS': 'execute-script',
      'verifyCommit': 'verify-commit',
      'runTests': 'run-tests'
    };

    return typeMap[legacyMethod] || legacyMethod;
  }

  /**
   * Translate legacy parameters to modern format
   */
  private translateParameters(legacyParams: any): any {
    // Parameter translation logic would go here
    // For now, pass through as-is
    return legacyParams;
  }
}

// Supporting types
export interface BridgeStatus {
  active: boolean;
  componentsConfigured: number;
  totalTrafficSplits: number;
  overallErrorRate: number;
  uptime: number;
}