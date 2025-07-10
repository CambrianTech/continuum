/**
 * JTAG Command - Server-side AI Debugging Portal
 * =============================================
 * Integrates with existing browser JTAG API for remote widget analysis
 */

import { BaseCommand, CommandResult } from '../../core/base-command/BaseCommand';
import { 
  JTAGProbeResponse, 
  JTAGProbeMethod, 
  WidgetAnalysisData,
  ShadowDOMAnalysisData,
  HealthAnalysisData,
  NetworkAnalysisData,
  PerformanceAnalysisData,
  ExecutionResult
} from '../../../shared/types/JTAGSharedTypes';

export interface JtagCommandParams {
  readonly method: JTAGProbeMethod;
  readonly options?: {
    readonly selector?: string;
    readonly code?: string;
    readonly autoLog?: boolean;
    readonly screenshot?: boolean;
  };
  readonly sessionId?: string;
}

export class JtagCommand extends BaseCommand {
  static getDefinition() {
    return {
      name: 'jtag',
      category: 'development',
      description: 'AI autonomous debugging - probe browser widget states from server',
      parameters: {
        method: {
          type: 'string' as const,
          description: 'JTAG probe method: widgets, shadowDOM, health, network, performance, execute',
          required: true
        },
        options: {
          type: 'object' as const,
          description: 'Probe options: selector, code, autoLog, screenshot',
          required: false
        },
        sessionId: {
          type: 'string' as const,
          description: 'Target session ID for probing',
          required: false
        }
      },
      examples: [
        {
          description: 'Analyze widget rendering states',
          command: 'jtag widgets'
        },
        {
          description: 'Check system health',
          command: 'jtag health'
        },
        {
          description: 'Analyze shadow DOM structure',
          command: 'jtag shadowDOM'
        }
      ]
    };
  }

  static async execute(params: JtagCommandParams): Promise<CommandResult> {
    try {
      const { method, options = {}, sessionId } = params;
      
      console.log(`🔍 JTAG Probe: ${method} (session: ${sessionId || 'current'})`);
      
      // Execute probe via browser WebSocket using existing continuum API
      const probeCode = `
        // Use existing browser JTAG API
        if (!window.jtag) {
          throw new Error('JTAG browser API not available');
        }
        
        const result = window.jtag.${method}(${JSON.stringify(options)});
        return result;
      `;
      
      // Execute JavaScript in browser via existing command system
      const executeResult = await JtagCommand.executeBrowserCode(probeCode, sessionId);
      
      if (!executeResult.success) {
        return {
          success: false,
          error: `JTAG probe failed: ${executeResult.error}`,
          data: { method, sessionId }
        };
      }
      
      const probeResult = executeResult.data as JTAGProbeResponse;
      
      // Format and log results based on probe type
      const summary = JtagCommand.formatProbeResults(method, probeResult);
      
      // Take screenshot if requested
      if (options.screenshot) {
        await JtagCommand.captureScreenshot(sessionId);
      }
      
      return {
        success: true,
        data: {
          method,
          sessionId,
          result: probeResult,
          summary,
          timestamp: Date.now()
        }
      };
      
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error),
        data: { method: params.method }
      };
    }
  }
  
  /**
   * Execute JavaScript code in browser via WebSocket
   */
  private static async executeBrowserCode(code: string, sessionId?: string): Promise<CommandResult> {
    try {
      // For now, return a mock result - this needs integration with the WebSocket system
      console.log(`🔍 Would execute in browser (session ${sessionId}):`, code);
      
      // TODO: Integrate with actual WebSocket command execution
      return {
        success: true,
        data: { 
          success: true,
          data: { widgets: [], summary: { total: 0, rendered: 0, broken: 0, empty: 0 } },
          timestamp: Date.now(),
          category: 'jtag-probe',
          executionTime: 0
        }
      };
      
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      };
    }
  }
  
  /**
   * Capture screenshot for visual debugging
   */
  private static async captureScreenshot(sessionId?: string): Promise<void> {
    try {
      console.log(`📸 Would capture screenshot for session: ${sessionId}`);
      // TODO: Integrate with screenshot command
    } catch (error) {
      console.log(`⚠️ Screenshot capture failed: ${error instanceof Error ? error.message : String(error)}`);
    }
  }
  
  /**
   * Format probe results for different analysis types
   */
  private static formatProbeResults(method: JTAGProbeMethod, result: JTAGProbeResponse): string {
    if (!result.success) {
      return `❌ ${method} probe failed: ${result.error?.message || 'Unknown error'}`;
    }
    
    switch (method) {
      case 'widgets':
        return JtagCommand.formatWidgetAnalysis(result.data as WidgetAnalysisData);
        
      case 'shadowDOM':
        return JtagCommand.formatShadowDOMAnalysis(result.data as ShadowDOMAnalysisData);
        
      case 'health':
        return JtagCommand.formatHealthAnalysis(result.data as HealthAnalysisData);
        
      case 'network':
        return JtagCommand.formatNetworkAnalysis(result.data as NetworkAnalysisData);
        
      case 'performance':
        return JtagCommand.formatPerformanceAnalysis(result.data as PerformanceAnalysisData);
        
      case 'execute':
        return JtagCommand.formatExecutionResult(result.data as ExecutionResult);
        
      default:
        return `✅ ${method} probe completed (${result.executionTime}ms)`;
    }
  }
  
  private static formatWidgetAnalysis(data: WidgetAnalysisData): string {
    const { summary, issues } = data;
    
    let output = `📊 Widget Analysis:\n`;
    output += `   Total: ${summary.total}\n`;
    output += `   ✅ Rendered: ${summary.rendered}\n`;
    output += `   ❌ Broken: ${summary.broken}\n`;
    output += `   ⚪ Empty: ${summary.empty}\n`;
    output += `   🎯 Performance: ${summary.performance}\n\n`;
    
    if (issues.length > 0) {
      output += `🚨 Issues Found:\n`;
      issues.forEach(issue => {
        const icon = issue.severity === 'critical' ? '🔥' : 
                   issue.severity === 'error' ? '❌' : 
                   issue.severity === 'warn' ? '⚠️' : 'ℹ️';
        output += `   ${icon} ${issue.widget}: ${issue.message}\n`;
        if (issue.suggestion) {
          output += `      💡 ${issue.suggestion}\n`;
        }
      });
    }
    
    return output.trim();
  }
  
  private static formatShadowDOMAnalysis(data: ShadowDOMAnalysisData): string {
    const { summary, elements } = data;
    
    let output = `🌐 Shadow DOM Analysis:\n`;
    output += `   Elements: ${summary.totalElements}\n`;
    output += `   With Shadow Root: ${summary.withShadowRoot}\n`;
    output += `   With Content: ${summary.withContent}\n`;
    output += `   Total Styles: ${summary.totalStyles}\n\n`;
    
    elements.forEach(element => {
      const status = element.hasContent ? '✅' : '❌';
      const shadowInfo = element.hasShadowRoot ? 
        `(${element.shadowLength} chars, ${element.styles.length} styles)` : 
        '(no shadow root)';
      output += `   ${status} ${element.tagName} ${shadowInfo}\n`;
    });
    
    return output.trim();
  }
  
  private static formatHealthAnalysis(data: HealthAnalysisData): string {
    const { overall, score, summary, issues, recommendations } = data;
    
    let output = `🏥 Health Check - Score: ${score}/100 (${overall})\n\n`;
    
    output += `📊 Component Health:\n`;
    output += `   Widgets: ${summary.widgets.score}/100 (${summary.widgets.status})\n`;
    output += `   Performance: ${summary.performance.score}/100 (${summary.performance.status})\n`;
    output += `   Network: ${summary.network.score}/100 (${summary.network.status})\n`;
    output += `   Memory: ${summary.memory.score}/100 (${summary.memory.status})\n`;
    
    if (issues.length > 0) {
      output += `\n🚨 Critical Issues:\n`;
      issues.forEach(issue => {
        const icon = issue.severity === 'critical' ? '🔥' : '⚠️';
        output += `   ${icon} ${issue.component}: ${issue.message}\n`;
      });
    }
    
    if (recommendations.length > 0) {
      output += `\n💡 Recommendations:\n`;
      recommendations.forEach(rec => {
        output += `   • ${rec}\n`;
      });
    }
    
    return output.trim();
  }
  
  private static formatNetworkAnalysis(data: NetworkAnalysisData): string {
    const { continuum, websocket } = data;
    
    let output = `🌐 Network Analysis:\n`;
    output += `   Online: ${data.online ? '✅' : '❌'}\n`;
    output += `   Continuum API: ${continuum.available ? '✅' : '❌'}\n`;
    
    if (continuum.available) {
      output += `      Session: ${continuum.sessionId || 'none'}\n`;
      output += `      Methods: ${continuum.methods.length}\n`;
    }
    
    output += `   WebSocket: ${websocket.connected ? '✅' : '❌'}\n`;
    if (websocket.supported) {
      output += `      Ready State: ${websocket.readyState}\n`;
      output += `      Messages: ${websocket.messagesReceived} received\n`;
    }
    
    return output.trim();
  }
  
  private static formatPerformanceAnalysis(data: PerformanceAnalysisData): string {
    const { memory, widgets, overall } = data;
    
    let output = `⚡ Performance Analysis - Grade: ${overall.grade} (${overall.score}/100)\n`;
    
    if (memory) {
      output += `   Memory: ${memory.used}MB / ${memory.total}MB (${memory.percentage}%)\n`;
    }
    
    output += `   Widgets: ${widgets.renderedWidgets}/${widgets.totalWidgets} rendered\n`;
    output += `   Avg Render Time: ${widgets.averageRenderTime}ms\n`;
    
    if (overall.issues.length > 0) {
      output += `\n⚠️ Performance Issues:\n`;
      overall.issues.forEach(issue => {
        output += `   • ${issue}\n`;
      });
    }
    
    return output.trim();
  }
  
  private static formatExecutionResult(data: ExecutionResult): string {
    if (!data.success) {
      return `❌ Execution failed: ${data.error}\n   Code: ${data.code}`;
    }
    
    let output = `✅ Execution successful (${data.executionTime}ms)\n`;
    output += `   Code: ${data.code}\n`;
    
    if (data.result !== undefined) {
      output += `   Result: ${JSON.stringify(data.result, null, 2)}\n`;
    }
    
    if (data.memoryBefore && data.memoryAfter) {
      const memoryDelta = data.memoryAfter - data.memoryBefore;
      output += `   Memory: ${data.memoryBefore}MB → ${data.memoryAfter}MB (${memoryDelta >= 0 ? '+' : ''}${memoryDelta}MB)\n`;
    }
    
    return output.trim();
  }
}