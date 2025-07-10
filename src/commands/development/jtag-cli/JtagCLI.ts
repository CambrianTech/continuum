/**
 * JTAG CLI Module - Clean AI Debugging Tool
 * =========================================
 * Modular implementation with proper delegation to Continuum's command system
 */

import { spawn } from 'child_process';

// Strong types for JTAG CLI operations
export interface JtagCommand {
  (): void | Promise<void>;
}

export interface JtagConfig {
  continuumBinary?: string;
  sessionsPath?: string;
}

// Strong types for command parameters (JSON format)
export interface ScreenshotParams {
  selector: string;
  scale: number;
  filename: string;
  destination: 'file' | 'bytes' | 'both';
  name_prefix?: string;
  source?: string;
}

export interface JSExecuteParams {
  script: string;
  generateUUID?: boolean;
  waitForResult?: boolean;
  timeout?: number;
  logExecution?: boolean;
}

// Strong types for command results
export interface JtagCommandResult {
  success: boolean;
  output: string;
  data?: any;
  error?: string | undefined;
}

export class JtagCLI {
  private config: JtagConfig;
  
  constructor(config: JtagConfig = {}) {
    this.config = {
      continuumBinary: './continuum',
      sessionsPath: '.continuum/sessions',
      ...config
    };
  }

  /**
   * Run any command dynamically - the ONLY method you need
   */
  async run(command: string, params?: any): Promise<JtagCommandResult> {
    console.log(`🌐 ${command}${params ? ' ' + JSON.stringify(params).substring(0, 50) + '...' : ''}`);
    
    const args = params ? [command, JSON.stringify(params)] : [command];
    const result = await this.spawnContinuum(args);
    
    const success = result.stdout.includes('"success": true') || !result.stdout.includes('Command execution failed');
    
    // Extract JSON from stdout (skip console log lines)
    let data = null;
    if (success && result.stdout) {
      try {
        // Find JSON in the output (starts with { or [)
        const jsonMatch = result.stdout.match(/[{\[].*/);
        if (jsonMatch) {
          data = JSON.parse(jsonMatch[0]);
        }
      } catch {
        // If JSON parsing fails, return raw output
        data = { rawOutput: result.stdout };
      }
    }
    
    return {
      success,
      output: result.stdout,
      data,
      error: success ? undefined : 'Command execution failed'
    };
  }

  // Convenience methods for common operations (just sugar over run())
  screenshot = (selector = 'body', scale = 2.0, filename?: string) => 
    this.run('screenshot', {
      selector, scale, 
      filename: filename || `jtag-${Date.now()}.png`,
      destination: 'file'
    });

  probe = (method = 'widgets') => 
    this.run('js-execute', {
      script: `window.jtag?.${method}() || {error: 'JTAG not available'}`,
      timeout: 5000
    });

  /**
   * Comprehensive widget inspection - delegates to widget-inspect command
   */
  inspectWidgets = (selector?: string, options?: any) => {
    const params = {
      selector: selector || 'continuum-sidebar, chat-widget, [class*="widget"], [class*="Widget"]',
      ...options
    };
    
    return this.run('widget-inspect', params);
  };

  health = () => this.run('health');
  logs = () => this.run('logs');
  errors = () => this.run('errors');
  warnings = () => this.run('warnings');
  session = () => this.run('session');
  hotreload = () => this.run('hotreload');


  /**
   * Show help information
   */
  help(): void {
    console.log(`
🛸 JTAG - AI Debugging Tool

Usage: ./jtag <command> [args]

🔍 Core Commands:
  screenshot [selector] [scale]  Capture browser screenshot (default: body 2.0)
  probe [method]                 Analyze widgets, shadowDOM, health, performance
  inspectWidgets [selector]      Comprehensive widget inspection with health, content, and state
  logs                           Show recent browser logs
  errors                         Show recent browser errors  
  warnings                       Show recent browser warnings
  health                         Check system health
  session                        Show current session info
  hotreload                      Rebuild and reload browser (preserves session)
  help                           Show this help

🚀 Examples:
  ./jtag screenshot                    # Full page, 2x scale
  ./jtag screenshot "saved-personas"   # Just personas widget
  ./jtag screenshot body 4.0           # High resolution
  ./jtag probe widgets                 # Widget analysis
  ./jtag inspectWidgets                # Full widget health report
  ./jtag inspectWidgets ".sidebar"     # Inspect specific widgets
  ./jtag logs                          # Recent activity

💡 All commands delegate to Continuum's command API
`);
  }

  /**
   * Private helper to spawn continuum commands
   */
  private spawnContinuum(args: string[]): Promise<{ code: number; stdout: string; stderr: string }> {
    return this.spawnCommand(this.config.continuumBinary!, args);
  }

  /**
   * Private helper to spawn any command
   */
  private spawnCommand(command: string, args: string[]): Promise<{ code: number; stdout: string; stderr: string }> {
    return new Promise((resolve, reject) => {
      const process = spawn(command, args, {
        stdio: 'pipe'
      });
      
      let stdout = '';
      let stderr = '';
      
      process.stdout.on('data', (data) => {
        const output = data.toString();
        stdout += output;
        console.log(output.trim());
      });
      
      process.stderr.on('data', (data) => {
        const output = data.toString();
        stderr += output;
        console.error(output.trim());
      });
      
      process.on('close', (code) => {
        resolve({ code: code || 0, stdout, stderr });
      });
      
      process.on('error', reject);
    });
  }
}