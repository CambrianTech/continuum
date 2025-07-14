// ISSUES: 3 open, last updated 2025-07-13 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking
// 🚨 CROSS-CUTTING CONCERN: Browser response data structure mismatch in WebSocket integration boundary
// 🎯 ARCHITECTURAL CHANGE: Converting to typed parameter execution pattern
/**
 * Screenshot Command - TypeScript Implementation
 * Elegant screenshot capture with advanced targeting and orchestration
 * Uses proper daemon bus architecture for browser communication
 * 
 * 🔧 CRITICAL ISSUE RESOLVED (2025-07-13):
 * - [✅] Issue #1: WebSocket messaging needs daemon context, not command context
 *   
 *   PROBLEM: RemoteCommand.sendToClientViaWebSocket() tried to call daemon messaging
 *   from command context, but only daemons have this.sendMessage() access.
 *   
 *   SOLUTION IMPLEMENTED: 
 *   ✅ RemoteCommand now returns _routeToDaemon instruction instead of direct messaging
 *   ✅ CommandProcessorDaemon.handleRemoteExecutionViaWebSocket() handles the actual communication
 *   ✅ WebSocket communication now properly routed through daemon system with this.sendMessage()
 *   
 *   TESTING RESULTS:
 *   ✅ Session management working (extracts sessionId correctly)
 *   ✅ Browser WebSocket handler implemented (ScreenshotExecutor modular)
 *   ✅ Server WebSocket handler implemented (send_to_session in WebSocketDaemon)
 *   ✅ html2canvas integration complete (dynamic loading)
 *   ✅ Command→Daemon messaging architecture fixed (routing via CommandProcessor)
 *   
 *   ARCHITECTURE: Command defines what to do, Daemon executes how to do it. ✅
 */

import { BaseCommand } from '../../core/base-command/BaseCommand';
import type { CommandDefinition, CommandResult, CommandContext } from '../../core/base-command/BaseCommand';
import { normalizeCommandCategory } from '../../../types/shared/CommandTypes';
import * as path from 'path';
import * as fs from 'fs';

// Strongly typed enums for screenshot behavior
export enum ScreenshotFormat {
  PNG = 'png',
  JPG = 'jpg', 
  JPEG = 'jpeg',
  WEBP = 'webp'
}

export enum ScreenshotDestination {
  FILE = 'file',           // Save to file, return filename
  BYTES = 'bytes',         // Return raw image data
  BOTH = 'both'           // Save to file AND return bytes
}

export enum ScreenshotAnimation {
  NONE = 'none',           // No UI feedback
  VISIBLE = 'visible',     // Show ROI highlighting
  ANIMATED = 'animated'    // Animate ROI highlighting
}

// Strongly typed screenshot parameters
export interface ScreenshotParams {
  selector?: string;
  filename?: string;
  format?: ScreenshotFormat;
  quality?: number;
  animation?: ScreenshotAnimation;
  destination?: ScreenshotDestination;
  subdirectory?: string;
}

export interface ScreenshotClientRequest {
  selector: string;
  filename: string;
  format: ScreenshotFormat;
  quality: number;
  animation: ScreenshotAnimation;
  destination: ScreenshotDestination;
}

export interface ScreenshotClientResponse {
  imageData: string;
  filename: string;
  selector: string;
  format: ScreenshotFormat;
  width: number;
  height: number;
}


interface ReadmeDefinition {
  name?: string;
  description?: string;
  icon?: string;
  category?: string;
  status?: string;
  parameters: Record<string, any>;
  todos?: string[];
}

/**
 * Screenshot Command - Captures browser screenshots with advanced targeting
 * Supports README-driven definitions and sophisticated browser orchestration
 */
export class ScreenshotCommand extends BaseCommand {

  static getDefinition(): CommandDefinition {
    console.log(`🔬 JTAG: getDefinition() called for ScreenshotCommand`);
    
    try {
      const readmePath = path.join(__dirname, 'README.md');
      const readme = fs.readFileSync(readmePath, 'utf8');
      const definition = this.parseReadmeDefinition(readme);
      
      console.log(`📖 JTAG: README definition parsed - name: ${definition.name}, params: ${Object.keys(definition.parameters).join(', ')}`);
      
      const finalDefinition = {
        name: definition.name ?? 'screenshot',
        category: normalizeCommandCategory(definition.category ?? 'browser'),
        icon: definition.icon ?? '📸',
        description: definition.description ?? 'Capture browser screenshot with advanced targeting',
        parameters: definition.parameters,
        examples: [
          { description: 'Save to file', command: `{"filename": "homepage.png", "destination": "${ScreenshotDestination.FILE}"}` },
          { description: 'Return bytes only', command: `{"selector": ".main-content", "destination": "${ScreenshotDestination.BYTES}"}` },
          { description: 'Both file and bytes', command: `{"filename": "content.png", "destination": "${ScreenshotDestination.BOTH}"}` },
          { description: 'Animated screenshot', command: `{"filename": "ui-test.png", "animation": "${ScreenshotAnimation.ANIMATED}"}` }
        ],
        usage: 'Capture screenshots with optional element targeting and custom naming'
      };
      
      console.log(`✅ JTAG: Final definition created - name: ${finalDefinition.name}, category: ${finalDefinition.category}`);
      return finalDefinition;
    } catch (error) {
      console.error(`❌ JTAG: Error reading README, using fallback definition: ${error}`);
      
      // Fallback definition if README.md not found
      const fallbackDefinition = {
        name: 'screenshot',
        category: normalizeCommandCategory('browser'),
        icon: '📸',
        description: 'Capture browser screenshot with advanced targeting',
        parameters: { 
          selector: { type: 'string' as const, description: 'CSS selector to target for screenshot' },
          filename: { type: 'string' as const, description: 'Output filename for the screenshot' },
          subdirectory: { type: 'string' as const, description: 'Subdirectory to save the screenshot in' }
        },
        examples: [
          { description: 'Save to file', command: `{"filename": "homepage.png", "destination": "${ScreenshotDestination.FILE}"}` },
          { description: 'Return bytes only', command: `{"selector": ".main-content", "destination": "${ScreenshotDestination.BYTES}"}` }
        ],
        usage: 'Capture screenshots with optional element targeting and custom naming'
      };
      
      console.log(`✅ JTAG: Fallback definition created - name: ${fallbackDefinition.name}, params: ${Object.keys(fallbackDefinition.parameters).join(', ')}`);
      return fallbackDefinition;
    }
  }

  /**
   * Parse README.md for command definition and parameters
   */
  private static parseReadmeDefinition(readme: string): ReadmeDefinition {
    const lines = readme.split('\n');
    const definition: ReadmeDefinition = { parameters: {} };
    
    let inDefinition = false;
    let inParams = false;
    let inTodos = false;
    const todos: string[] = [];
    
    for (const line of lines) {
      if (line.includes('## Definition')) {
        inDefinition = true;
        continue;
      }
      if (inDefinition && line.startsWith('##')) {
        inDefinition = false;
      }
      if (line.includes('## Parameters')) {
        inParams = true;
        continue;
      }
      if (inParams && line.startsWith('##')) {
        inParams = false;
      }
      if (line.includes('## TODO:')) {
        inTodos = true;
        continue;
      }
      if (inTodos && line.startsWith('##')) {
        inTodos = false;
      }
      
      if (inDefinition) {
        if (line.includes('**Name**:')) {
          definition.name = line.split('**Name**:')[1].trim();
        } else if (line.includes('**Description**:')) {
          definition.description = line.split('**Description**:')[1].trim();
        } else if (line.includes('**Icon**:')) {
          definition.icon = line.split('**Icon**:')[1].trim();
        } else if (line.includes('**Category**:')) {
          definition.category = line.split('**Category**:')[1].trim();
        } else if (line.includes('**Status**:')) {
          definition.status = line.split('**Status**:')[1].trim();
        }
      }
      
      if (inParams && line.includes('`') && line.includes(':')) {
        const param = line.match(/`([^`]+)`:\s*(.+)/);
        if (param) {
          definition.parameters[param[1]] = {
            type: 'string',
            description: param[2]
          };
        }
      }
      
      if (inTodos && line.includes('TODO:')) {
        todos.push(line.trim());
      }
    }
    
    // Add TODOs to description if present
    if (todos.length > 0) {
      definition.todos = todos;
      definition.description = (definition.description ?? '') + ` (⚠️ ${todos.length} TODOs pending)`;
    }
    
    return definition;
  }

  static async execute(params: ScreenshotParams | ScreenshotClientRequest, context?: CommandContext): Promise<CommandResult> {
    const startTime = Date.now();
    console.log(`🚀 JTAG SCREENSHOT: ScreenshotCommand.execute() called`);
    console.log(`🚀 JTAG SCREENSHOT: Starting ScreenshotCommand execution (server side)`);
    console.log(`📋 JTAG SCREENSHOT: Parameters received:`, JSON.stringify(params, null, 2));
    console.log(`📋 JTAG SCREENSHOT: Context:`, JSON.stringify(context, null, 2));
    
    try {
      console.log(`🔍 JTAG SCREENSHOT: Checking environment - window:${typeof window}, document:${typeof document}`);
      
      // Check if we're running in browser context (client-side)
      if (typeof window !== 'undefined' && typeof document !== 'undefined') {
        console.log(`📸 JTAG SCREENSHOT: Running in browser context - executing client-side`);
        return this.executeClient(params as ScreenshotClientRequest);
      }
      
      // Server-side execution - use html2canvas for actual screenshot
      console.log(`📤 JTAG SCREENSHOT: Server-side execution starting - html2canvas capture`);
      
      // Normalize parameters for consistent client execution
      const normalizedParams: ScreenshotClientRequest = {
        selector: (params as ScreenshotParams).selector ?? 'body',
        filename: (params as ScreenshotParams).filename ?? `screenshot-${Date.now()}.png`,
        format: (params as ScreenshotParams).format ?? ScreenshotFormat.PNG,
        quality: (params as ScreenshotParams).quality ?? 0.9,
        animation: (params as ScreenshotParams).animation ?? ScreenshotAnimation.NONE,
        destination: (params as ScreenshotParams).destination ?? ScreenshotDestination.FILE
      };
      
      // Get the session-specific screenshots directory
      const sessionId = context?.sessionId ?? 'development-shared-md3a2zj3-bqvkf';
      const screenshotsDir = path.join(process.cwd(), '.continuum', 'sessions', 'user', 'shared', sessionId, 'screenshots');
      
      console.log(`📁 JTAG SCREENSHOT: Session ID: ${sessionId}`);
      console.log(`📁 JTAG SCREENSHOT: Context:`, JSON.stringify(context, null, 2));
      console.log(`📁 JTAG SCREENSHOT: Screenshots directory: ${screenshotsDir}`);
      
      // Call the client-side screenshot function (loaded globally in browser bundle)
      const screenshotScript = `
        (async () => {
          // Call the client screenshot function with parameters
          return await window.clientScreenshot({
            selector: '${normalizedParams.selector}',
            filename: '${normalizedParams.filename}',
            format: '${normalizedParams.format}',
            quality: ${normalizedParams.quality},
            animation: '${normalizedParams.animation}',
            destination: '${normalizedParams.destination}',
            directory: '${screenshotsDir}'
          });
        })()
      `;
      
      console.log(`📤 JTAG SCREENSHOT: Calling global.continuum.executeJS`);
      console.log(`📤 JTAG SCREENSHOT: Process PID: ${process.pid}`);
      console.log(`📤 JTAG SCREENSHOT: global object keys:`, Object.keys(global));
      console.log(`📤 JTAG SCREENSHOT: global.continuum type:`, typeof (global as any).continuum);
      
      const continuum = (global as any).continuum;
      if (!continuum) {
        throw new Error('global.continuum does not exist');
      }
      
      if (!continuum.executeJS) {
        throw new Error('global.continuum.executeJS does not exist');
      }
      
      const result = await continuum.executeJS(screenshotScript);
      
      console.log(`📤 JTAG SCREENSHOT: executeJS result:`, result);
      
      const executionTime = Date.now() - startTime;
      
      return {
        success: true,
        data: result,
        timestamp: new Date().toISOString(),
        executionTime,
        processor: 'server-executeJS'
      };
      
    } catch (error) {
      const executionTime = Date.now() - startTime;
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.error(`❌ JTAG: Screenshot execution failed after ${executionTime}ms: ${errorMessage}`);
      return {
        success: false,
        error: `Screenshot failed: ${errorMessage}`,
        timestamp: new Date().toISOString(),
        executionTime,
        processor: 'server'
      };
    }
  }


  /**
   * Browser-side execution with html2canvas - smart file saving
   */
  static async executeClient(params: ScreenshotClientRequest): Promise<CommandResult> {
    const startTime = Date.now();
    
    console.log(`🚀🚀🚀 JTAG BROWSER SCREENSHOT: executeClient() called from browser!`);
    console.log(`🚀🚀🚀 JTAG BROWSER SCREENSHOT: Received params:`, JSON.stringify(params, null, 2));
    
    try {
      const { selector, format, quality, filename, destination } = params;
      const continuum = (window as any).continuum;
      
      console.log(`📸 JTAG BROWSER: Starting screenshot capture (client side) - selector: ${selector}, format: ${format}, filename: ${filename}`);
      console.log(`📋 JTAG BROWSER: Destination: ${destination}`);
      
      // Find target element
      const targetElement = selector === 'body' ? document.body : document.querySelector(selector);
      if (!targetElement) {
        throw new Error(`Element not found: ${selector}`);
      }
      
      // Load html2canvas dynamically if not already loaded
      const html2canvas = await this.loadHtml2Canvas();
      
      console.log(`📦 JTAG BROWSER: html2canvas available - starting capture`);
      
      // Capture screenshot
      const canvas = await html2canvas(targetElement, {
        allowTaint: true,
        useCORS: true,
        scale: 1,
        logging: false
      });
      
      // Convert to desired format
      const imageData = format === ScreenshotFormat.PNG ? 
        canvas.toDataURL('image/png') : 
        canvas.toDataURL(`image/${format}`, quality);
      
      console.log(`🖼️ JTAG BROWSER: Canvas captured, converting to bytes`);
      
      // Extract base64 data from data URL and convert to bytes
      const base64Data = imageData.replace(/^data:image\/[a-z]+;base64,/, '');
      const bytes = Uint8Array.from(atob(base64Data), c => c.charCodeAt(0)); //getting a warning here about atob
      
      const executionTime = Date.now() - startTime;
      console.log(`✅ JTAG BROWSER: Screenshot bytes ready in ${executionTime}ms - size: ${bytes.length} bytes`);
      

      //file save adheres to commandresult already and is a promise:
      return continuum.fileSave({
        content: bytes,
        filename: filename,
        artifactType: 'screenshot'
      });
      
    } catch (error) {
      const executionTime = Date.now() - startTime;
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.error(`❌ JTAG BROWSER: Screenshot capture failed after ${executionTime}ms - ${errorMessage}`);
      throw error;
    }
  }


  /**
   * Dynamically load html2canvas library and return the function
   */
  private static async loadHtml2Canvas(): Promise<any> {
    return new Promise((resolve, reject) => {
      // Check if already loaded
      if ((window as any).html2canvas) {
        resolve((window as any).html2canvas);
        return;
      }

      const script = document.createElement('script');
      script.src = 'https://cdnjs.cloudflare.com/ajax/libs/html2canvas/1.4.1/html2canvas.min.js';
      script.onload = (): void => {
        console.log('✅ html2canvas loaded successfully');
        // Return the loaded function directly, don't rely on window global
        const html2canvas = (window as any).html2canvas;
        if (html2canvas) {
          resolve(html2canvas);
        } else {
          reject(new Error('html2canvas not available after load'));
        }
      };
      script.onerror = (): void => {
        reject(new Error('Failed to load html2canvas'));
      };
      document.head.appendChild(script);
    });
  }

}

export default ScreenshotCommand;