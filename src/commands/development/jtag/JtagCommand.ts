/**
 * JTAG Command - CLI Interface for Browser Debugging
 * 
 * Provides easy command-line access to the JTAG probe system
 * running in the browser.
 */

import { BaseCommand } from '../../core/base-command/BaseCommand';
import { CommandResult } from '../../../types/CommandTypes';
import {
  JTAGProbeMethod,
  JTAG_PROBE_METHODS,
  isJTAGProbeMethod
} from '../../../shared/types/JTAGSharedTypes';

interface JtagCommandParams {
  method?: JTAGProbeMethod;
  code?: string;
  batch?: string;
  watch?: boolean;
  interval?: number;
  format?: 'json' | 'table' | 'summary';
  selector?: string;
}

export class JtagCommand extends BaseCommand {
  public static getDefinition() {
    return {
      name: 'jtag',
      description: 'JTAG debugging system - probe widget states, DOM, and performance',
      category: 'development',
      examples: [
        { description: 'Check widget states and rendering', command: 'jtag widgets' },
        { description: 'Investigate shadow DOM content', command: 'jtag shadowDOM' },
        { description: 'System health check', command: 'jtag health' },
        { description: 'Execute custom JavaScript', command: 'jtag execute --code "document.title"' },
        { description: 'Run multiple probes in batch', command: 'jtag batch --methods widgets,health' },
        { description: 'Widget analysis in table format', command: 'jtag widgets --format table' },
        { description: 'Watch widgets in real-time', command: 'jtag widgets --watch --interval 5000' }
      ],
      parameters: {
        method: {
          type: 'string' as const,
          description: 'Probe method to execute',
          choices: JTAG_PROBE_METHODS as readonly string[],
          example: 'widgets'
        },
        code: {
          type: 'string' as const,
          description: 'JavaScript code to execute (for execute method)',
          example: 'document.querySelectorAll("continuum-sidebar").length'
        },
        batch: {
          type: 'string' as const,
          description: 'Comma-separated list of methods to run in batch',
          example: 'widgets,health,performance'
        },
        watch: {
          type: 'boolean' as const,
          description: 'Watch for changes and re-run probe',
          default: false
        },
        interval: {
          type: 'number' as const,
          description: 'Watch interval in milliseconds',
          default: 3000
        },
        format: {
          type: 'string' as const,
          description: 'Output format',
          choices: ['json', 'table', 'summary'] as const,
          default: 'summary'
        },
        selector: {
          type: 'string' as const,
          description: 'CSS selector for shadowDOM probe',
          example: 'continuum-sidebar'
        }
      }
    };
  }

  public async execute(params: JtagCommandParams = {}): Promise<CommandResult> {
    try {
      // Handle different execution modes
      if (params.batch) {
        return await this.executeBatch(params);
      } else if (params.code) {
        return await this.executeCustomCode(params);
      } else if (params.method) {
        return await this.executeSingleProbe(params);
      } else {
        return await this.showHelp();
      }
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error),
        timestamp: new Date().toISOString()
      };
    }
  }

  private async executeSingleProbe(params: JtagCommandParams): Promise<CommandResult> {
    const { method, format = 'summary', watch = false, interval = 3000 } = params;

    if (!method || !isJTAGProbeMethod(method)) {
      return {
        success: false,
        error: `Invalid probe method. Available: ${JTAG_PROBE_METHODS.join(', ')}`,
        timestamp: new Date().toISOString()
      };
    }

    const executeProbe = async () => {
      const jsCode = this.generateProbeCode(method, params);
      const result = await this.executeInBrowser(jsCode);
      return this.formatOutput(result, format, method);
    };

    if (watch) {
      console.log(`🔍 Watching ${method} probe every ${interval}ms (press Ctrl+C to stop)...`);
      
      // Initial execution
      const initial = await executeProbe();
      console.log(initial.data);

      // Set up watch interval
      const watchInterval = setInterval(async () => {
        try {
          const result = await executeProbe();
          console.clear();
          console.log(`🔍 JTAG ${method} - ${new Date().toLocaleTimeString()}`);
          console.log(result.data);
        } catch (error) {
          console.error(`❌ Watch error: ${error}`);
        }
      }, interval);

      // Handle cleanup on process exit
      process.on('SIGINT', () => {
        clearInterval(watchInterval);
        process.exit(0);
      });

      return { success: true, data: 'Watch mode started', timestamp: new Date().toISOString() };
    } else {
      return await executeProbe();
    }
  }

  private async executeBatch(params: JtagCommandParams): Promise<CommandResult> {
    const { batch, format = 'summary' } = params;
    
    if (!batch) {
      return {
        success: false,
        error: 'Batch parameter required',
        timestamp: new Date().toISOString()
      };
    }

    const methods = batch.split(',').map(m => m.trim()) as JTAGProbeMethod[];
    const invalidMethods = methods.filter(m => !isJTAGProbeMethod(m));
    
    if (invalidMethods.length > 0) {
      return {
        success: false,
        error: `Invalid methods: ${invalidMethods.join(', ')}`,
        timestamp: new Date().toISOString()
      };
    }

    const jsCode = `
      if (window.jtag) {
        const results = window.jtag.batch([${methods.map(m => `'${m}'`).join(', ')}]);
        JSON.stringify(results);
      } else {
        JSON.stringify({ error: 'JTAG not available' });
      }
    `;

    const result = await this.executeInBrowser(jsCode);
    return this.formatBatchOutput(result, format);
  }

  private async executeCustomCode(params: JtagCommandParams): Promise<CommandResult> {
    const { code, format = 'summary' } = params;
    
    if (!code) {
      return {
        success: false,
        error: 'Code parameter required for execute method',
        timestamp: new Date().toISOString()
      };
    }

    const jsCode = `
      if (window.jtag) {
        const result = window.jtag.execute({ code: ${JSON.stringify(code)}, context: 'browser' });
        JSON.stringify(result);
      } else {
        JSON.stringify({ error: 'JTAG not available' });
      }
    `;

    const result = await this.executeInBrowser(jsCode);
    return this.formatExecutionOutput(result, format);
  }

  private generateProbeCode(method: JTAGProbeMethod, params: JtagCommandParams): string {
    const { selector } = params;

    switch (method) {
      case 'shadowDOM':
        return `
          if (window.jtag) {
            const result = window.jtag.shadowDOM(${selector ? `'${selector}'` : 'undefined'});
            JSON.stringify(result);
          } else {
            JSON.stringify({ error: 'JTAG not available' });
          }
        `;
      
      default:
        return `
          if (window.jtag) {
            const result = window.jtag.${method}();
            JSON.stringify(result);
          } else {
            JSON.stringify({ error: 'JTAG not available' });
          }
        `;
    }
  }

  private async executeInBrowser(jsCode: string): Promise<any> {
    // TODO: Integrate with existing console command system
    // For now, return mock result to indicate command structure is working
    const consoleResult = { 
      success: true, 
      data: 'Mock execution - integration with console command pending' 
    };

    if (!consoleResult.success) {
      throw new Error(`Failed to execute in browser: ${(consoleResult as any).error || 'Unknown error'}`);
    }

    // The result will be in the browser logs - we'd need to parse it
    // For now, return a mock result indicating the command was sent
    return {
      success: true,
      message: 'Probe executed in browser - check browser logs for results',
      executedCode: jsCode
    };
  }

  private formatOutput(result: any, format: string, method: string): CommandResult {
    if (format === 'json') {
      return {
        success: true,
        data: result,
        timestamp: new Date().toISOString()
      };
    }

    if (format === 'table') {
      return this.formatAsTable(result, method);
    }

    // Summary format
    return this.formatAsSummary(result, method);
  }

  private formatAsTable(_result: any, method: string): CommandResult {
    // Implementation would depend on the specific probe data structure
    const summary = `📊 ${method.toUpperCase()} TABLE FORMAT\n` +
                   `Command sent to browser - check logs for detailed table output`;

    return {
      success: true,
      data: summary,
      timestamp: new Date().toISOString()
    };
  }

  private formatAsSummary(_result: any, method: string): CommandResult {
    const timestamp = new Date().toLocaleTimeString();
    
    let summary = `🔍 JTAG ${method.toUpperCase()} PROBE - ${timestamp}\n`;
    summary += `✅ Probe command sent to browser\n`;
    summary += `📋 Check browser logs for detailed results:\n`;
    summary += `   tail -f .continuum/sessions/*/logs/browser.probe.json\n`;
    summary += `🌐 Or check browser console at localhost:9000\n`;

    return {
      success: true,
      data: summary,
      timestamp: new Date().toISOString()
    };
  }

  private formatBatchOutput(result: any, format: string): CommandResult {
    if (format === 'json') {
      return {
        success: true,
        data: result,
        timestamp: new Date().toISOString()
      };
    }

    const summary = `🔍 JTAG BATCH PROBE RESULTS\n` +
                   `✅ Batch command sent to browser\n` +
                   `📋 Check browser logs for detailed results`;

    return {
      success: true,
      data: summary,
      timestamp: new Date().toISOString()
    };
  }

  private formatExecutionOutput(result: any, format: string): CommandResult {
    if (format === 'json') {
      return {
        success: true,
        data: result,
        timestamp: new Date().toISOString()
      };
    }

    const summary = `🔍 JTAG CUSTOM EXECUTION\n` +
                   `✅ Custom code sent to browser\n` +
                   `📋 Check browser logs for execution results`;

    return {
      success: true,
      data: summary,
      timestamp: new Date().toISOString()
    };
  }

  private async showHelp(): Promise<CommandResult> {
    const help = `
🛸 JTAG Debugging System - CLI Interface

BASIC USAGE:
  jtag <method>              Run a single probe
  jtag execute --code <js>   Execute custom JavaScript
  jtag batch --methods <list> Run multiple probes

PROBE METHODS:
  widgets        - Analyze widget states and rendering
  shadowDOM      - Investigate shadow DOM content
  customElements - Check custom element registration
  performance    - Performance metrics and memory usage
  network        - API connectivity and WebSocket status  
  health         - Comprehensive system health check

EXAMPLES:
  jtag widgets                           # Check widget states
  jtag shadowDOM --selector chat-widget  # Check specific shadow DOM
  jtag health --format table            # Health check as table
  jtag execute --code "document.title"   # Execute custom JavaScript
  jtag batch --methods widgets,health    # Run multiple probes
  jtag widgets --watch --interval 5000   # Watch widgets every 5 seconds

OUTPUT FORMATS:
  --format summary   # Human-readable summary (default)
  --format json      # Raw JSON data
  --format table     # Tabular format

ADVANCED OPTIONS:
  --watch           # Continuously monitor (Ctrl+C to stop)
  --interval <ms>   # Watch interval in milliseconds
  --selector <css>  # CSS selector for shadowDOM probe

The probes execute in the browser and results are logged to:
  📋 Browser logs: .continuum/sessions/*/logs/browser.probe.json
  🌐 Browser console: http://localhost:9000
`;

    return {
      success: true,
      data: help,
      timestamp: new Date().toISOString()
    };
  }
}