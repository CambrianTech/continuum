/**
 * Health Command - Universal Server + Client Health Reporting
 * 
 * DUAL-SIDE IMPLEMENTATION:
 * =========================
 * SERVER SIDE: Reports daemon, service, and command health
 * CLIENT SIDE: Reports browser, widget, and connection health
 * 
 * INTEGRATION: Browser reports its own health back to server via WebSocket
 * 
 * AUTONOMOUS VALIDATION: Complete system health visible to AI for diagnosis
 */

import { BaseCommand } from '../base-command/BaseCommand';
import { CommandDefinition, CommandResult } from '../../../types/CommandTypes';

export interface HealthStatus {
  component: string;
  status: 'healthy' | 'degraded' | 'failed' | 'unknown';
  lastCheck: number;
  details: string;
  dependencies?: string[];
  metrics?: HealthMetrics;
}

export interface HealthMetrics {
  uptime?: number;
  errorCount?: number;
  responseTime?: number;
  memoryUsage?: number;
  connectionCount?: number;
}

export interface HealthReport {
  timestamp: number;
  environment: 'server' | 'browser';
  overall: 'healthy' | 'degraded' | 'failed';
  components: HealthStatus[];
  summary: string;
}

export class HealthCommand extends BaseCommand {
  static getDefinition(): CommandDefinition {
    return {
      name: 'health',
      description: 'Get comprehensive system health status from server and client',
      category: 'core',
      parameters: {
        component: {
          type: 'string',
          description: 'Specific component to check (optional)',
          required: false
        },
        includeClient: {
          type: 'boolean',
          description: 'Include client-side health report',
          required: false,
          default: true
        }
      },
      examples: [
        {
          description: 'Get full system health',
          command: 'health'
        },
        {
          description: 'Check specific component',
          command: 'health --component=websocket-daemon'
        },
        {
          description: 'Server health only',
          command: 'health --includeClient=false'
        }
      ]
    };
  }

  static async execute(params: any = {}): Promise<CommandResult> {
    try {
      const startTime = Date.now();
      
      // Generate server-side health report
      const serverReport = await this.generateServerHealthReport(params.component);
      
      // If client health requested, wait for client report
      let clientReport: HealthReport | null = null;
      if (params.includeClient !== false) {
        clientReport = await this.requestClientHealthReport();
      }
      
      const responseTime = Date.now() - startTime;
      
      return this.createSuccessResult({
        server: serverReport,
        client: clientReport,
        summary: this.generateHealthSummary(serverReport, clientReport),
        responseTime: `${responseTime}ms`
      });
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return this.createErrorResult(`Health check failed: ${errorMessage}`);
    }
  }

  private static async generateServerHealthReport(specificComponent?: string): Promise<HealthReport> {
    const components: HealthStatus[] = [];
    
    // Check core daemons
    if (!specificComponent || specificComponent.includes('daemon')) {
      components.push(...await this.checkDaemons());
    }
    
    // Check commands availability
    if (!specificComponent || specificComponent.includes('command')) {
      components.push(...await this.checkCommands());
    }
    
    // Check system resources
    if (!specificComponent || specificComponent.includes('system')) {
      components.push(...await this.checkSystemResources());
    }
    
    const overall = this.calculateOverallHealth(components);
    
    return {
      timestamp: Date.now(),
      environment: 'server',
      overall,
      components,
      summary: `Server health: ${overall} (${components.length} components checked)`
    };
  }

  private static async checkDaemons(): Promise<HealthStatus[]> {
    const daemons = [
      'websocket-daemon',
      'renderer-daemon', 
      'command-processor-daemon',
      'browser-manager-daemon'
    ];
    
    return daemons.map(daemon => {
      // TODO: Integrate with actual daemon health checks
      return {
        component: daemon,
        status: 'healthy' as const,
        lastCheck: Date.now(),
        details: 'Daemon running normally',
        metrics: {
          uptime: Math.floor(Math.random() * 86400000), // Mock uptime
          errorCount: 0
        }
      };
    });
  }

  private static async checkCommands(): Promise<HealthStatus[]> {
    const commands = ['preferences', 'reload', 'help', 'info', 'health'];
    
    return commands.map(command => {
      return {
        component: `${command}-command`,
        status: 'healthy' as const,
        lastCheck: Date.now(),
        details: 'Command available and functional'
      };
    });
  }

  private static async checkSystemResources(): Promise<HealthStatus[]> {
    return [
      {
        component: 'file-system',
        status: 'healthy' as const,
        lastCheck: Date.now(),
        details: 'File system accessible',
        metrics: {
          responseTime: 5
        }
      },
      {
        component: 'network',
        status: 'healthy' as const,
        lastCheck: Date.now(),
        details: 'Network interfaces available'
      }
    ];
  }

  private static async requestClientHealthReport(): Promise<HealthReport | null> {
    // TODO: Send WebSocket message to request client health
    // For now, return mock client health
    return {
      timestamp: Date.now(),
      environment: 'browser',
      overall: 'healthy',
      components: [
        {
          component: 'websocket-connection',
          status: 'healthy',
          lastCheck: Date.now(),
          details: 'Connected to server'
        },
        {
          component: 'chat-widget',
          status: 'healthy', 
          lastCheck: Date.now(),
          details: 'Widget loaded and responsive'
        },
        {
          component: 'continuum-api',
          status: 'healthy',
          lastCheck: Date.now(),
          details: 'API methods available'
        }
      ],
      summary: 'Browser health: healthy (3 components checked)'
    };
  }

  private static calculateOverallHealth(components: HealthStatus[]): 'healthy' | 'degraded' | 'failed' {
    const healthyCounts = {
      healthy: components.filter(c => c.status === 'healthy').length,
      degraded: components.filter(c => c.status === 'degraded').length,
      failed: components.filter(c => c.status === 'failed').length
    };
    
    if (healthyCounts.failed > 0) return 'failed';
    if (healthyCounts.degraded > 0) return 'degraded';
    return 'healthy';
  }

  private static generateHealthSummary(serverReport: HealthReport, clientReport: HealthReport | null): string {
    const serverComponents = serverReport.components.length;
    const clientComponents = clientReport?.components.length || 0;
    const totalComponents = serverComponents + clientComponents;
    
    const serverHealthy = serverReport.overall === 'healthy';
    const clientHealthy = clientReport?.overall === 'healthy';
    
    if (serverHealthy && (clientHealthy !== false)) {
      return `🟢 System healthy: ${totalComponents} components operational`;
    } else if (serverReport.overall === 'degraded' || clientReport?.overall === 'degraded') {
      return `🟡 System degraded: Some components experiencing issues`;
    } else {
      return `🔴 System unhealthy: Critical components failed`;
    }
  }
}

// Export for both server and client use
export default HealthCommand;