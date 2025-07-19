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
import type { CommandDefinition, CommandResult, ContinuumContext } from '../../core/base-command/BaseCommand';
import { normalizeCommandCategory } from '../../../types/shared/CommandTypes';
import { 
  ScreenshotFormat, 
  ScreenshotDestination, 
  ScreenshotAnimation,
  type ScreenshotClientRequest,
} from './shared/ScreenshotTypes';

// ✅ STRONGLY TYPED PARAMETERS - Eliminates 'any' types
// Unified interface covering both ScreenshotParams and ScreenshotClientRequest
interface ScreenshotParameters {
  selector?: string;
  filename?: string;
  format?: ScreenshotFormat;
  quality?: number;
  animation?: ScreenshotAnimation;
  destination?: ScreenshotDestination;
  subdirectory?: string;
  
  // AI-friendly features
  width?: number;
  height?: number;
  scale?: number;
  cropX?: number;
  cropY?: number;
  cropWidth?: number;
  cropHeight?: number;
  elementName?: string;
  querySelector?: string;
  maxFileSize?: number;
}

/**
 * Type guard for ScreenshotParameters
 */
function validateScreenshotParameters(params: unknown): params is ScreenshotParameters {
  if (typeof params !== 'object' || params === null) {
    return false;
  }
  
  const obj = params as Record<string, unknown>;
  
  // All parameters are optional, but validate types if present
  if (obj.selector !== undefined && typeof obj.selector !== 'string') {
    return false;
  }
  if (obj.filename !== undefined && typeof obj.filename !== 'string') {
    return false;
  }
  if (obj.format !== undefined && !Object.values(ScreenshotFormat).includes(obj.format as ScreenshotFormat)) {
    return false;
  }
  if (obj.quality !== undefined && (typeof obj.quality !== 'number' || obj.quality < 0 || obj.quality > 1)) {
    return false;
  }
  if (obj.animation !== undefined && !Object.values(ScreenshotAnimation).includes(obj.animation as ScreenshotAnimation)) {
    return false;
  }
  if (obj.destination !== undefined && !Object.values(ScreenshotDestination).includes(obj.destination as ScreenshotDestination)) {
    return false;
  }
  if (obj.subdirectory !== undefined && typeof obj.subdirectory !== 'string') {
    return false;
  }
  
  // Validate numeric AI-friendly features
  const numericFields = ['width', 'height', 'scale', 'cropX', 'cropY', 'cropWidth', 'cropHeight', 'maxFileSize'];
  for (const field of numericFields) {
    if (obj[field] !== undefined && typeof obj[field] !== 'number') {
      return false;
    }
  }
  
  // Validate string AI-friendly features
  if (obj.elementName !== undefined && typeof obj.elementName !== 'string') {
    return false;
  }
  if (obj.querySelector !== undefined && typeof obj.querySelector !== 'string') {
    return false;
  }
  
  return true;
}
import * as path from 'path';
import * as fs from 'fs';


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

  /**
   * ✅ JOEL'S CLEAN APPROACH - Single execute method with internal typing
   * Validation at the top, then typed params for rest of method
   */
  static async execute(parameters: unknown, context: ContinuumContext): Promise<CommandResult> {
    // === VALIDATION SECTION (top of method) ===
    if (typeof parameters !== 'object' || parameters === null) {
      return { success: false, error: 'Parameters must be a non-null object', timestamp: new Date().toISOString() };
    }
    
    const parsedParams = ScreenshotCommand.parseCliArguments(parameters as Record<string, unknown>);
    
    if (!validateScreenshotParameters(parsedParams)) {
      return { success: false, error: 'Invalid screenshot parameters. Check format, quality (0-1), and parameter types.', timestamp: new Date().toISOString() };
    }
    
    // === TYPED BUSINESS LOGIC SECTION (rest of method) ===
    // Now params is strongly typed for the entire rest of the method
    const params = parsedParams as ScreenshotParameters;
    
    const startTime = Date.now();
    console.log(`🚀 JTAG SCREENSHOT: Clean typed execution`);
    console.log(`📋 JTAG SCREENSHOT: Typed params:`, JSON.stringify(params, null, 2));
    console.log(`📋 JTAG SCREENSHOT: Context:`, JSON.stringify(context, null, 2));
    
    try {
      // Server-side execution - use html2canvas for actual screenshot
      console.log(`📤 JTAG SCREENSHOT: Server-side execution starting - html2canvas capture`);
      
      // Map querySelector to selector (querySelector takes precedence)
      const targetSelector = params.querySelector ?? params.selector ?? 'body';
      
      // Map elementName to querySelector for backward compatibility
      const elementQuery = params.elementName ?? params.querySelector;
      
      const normalizedParams: ScreenshotClientRequest = {
        selector: targetSelector,
        filename: params.filename ?? `screenshot-${Date.now()}.png`,
        format: params.format ?? ScreenshotFormat.PNG,
        quality: params.quality ?? 0.9,
        animation: params.animation ?? ScreenshotAnimation.NONE,
        destination: params.destination ?? ScreenshotDestination.FILE,
        
        // AI-friendly features - provide undefined explicitly for optional properties
        width: params.width ?? undefined,
        height: params.height ?? undefined,
        scale: params.scale ?? undefined,
        cropX: params.cropX ?? undefined,
        cropY: params.cropY ?? undefined,
        cropWidth: params.cropWidth ?? undefined,
        cropHeight: params.cropHeight ?? undefined,
        elementName: elementQuery ?? undefined,
        querySelector: elementQuery ?? undefined,
        maxFileSize: params.maxFileSize ?? undefined
      };
      
      // Session ID will be handled by FileWriteCommand through context
      const sessionId = context.sessionId ?? 'unknown-session';
      
      console.log(`📁 JTAG SCREENSHOT: Session ID: ${sessionId}`);
      console.log(`📁 JTAG SCREENSHOT: Context:`, JSON.stringify(context, null, 2));
      
      // Call the client-side screenshot function with AI-friendly features
      const screenshotScript = `
        (async () => {
          // Call the client screenshot function with AI-enhanced parameters
          return await window.clientScreenshot({
            selector: '${normalizedParams.selector}',
            filename: '${normalizedParams.filename}',
            format: '${normalizedParams.format}',
            quality: ${normalizedParams.quality},
            animation: '${normalizedParams.animation}',
            destination: '${normalizedParams.destination}',
            width: ${normalizedParams.width || 'undefined'},
            height: ${normalizedParams.height || 'undefined'},
            scale: ${normalizedParams.scale || 'undefined'},
            cropX: ${normalizedParams.cropX || 'undefined'},
            cropY: ${normalizedParams.cropY || 'undefined'},
            cropWidth: ${normalizedParams.cropWidth || 'undefined'},
            cropHeight: ${normalizedParams.cropHeight || 'undefined'},
            elementName: '${normalizedParams.elementName || ''}',
            querySelector: '${normalizedParams.querySelector || ''}',
            maxFileSize: ${normalizedParams.maxFileSize || 'undefined'}
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
   * Parse CLI arguments to extract typed parameters
   */
  private static parseCliArguments(params: Record<string, unknown>): Record<string, unknown> {
    // Handle CLI-style arguments if present in args array
    if (params.args && Array.isArray(params.args)) {
      const result: Record<string, unknown> = { ...params };
      const remainingArgs: string[] = [];
      
      // Parse CLI-style args: --selector=body --filename=test.png
      for (const arg of params.args) {
        if (typeof arg === 'string' && arg.startsWith('--')) {
          const [key, value] = arg.split('=', 2);
          const cleanKey = key.replace('--', '');
          
          // Convert known numeric parameters
          if (['quality', 'width', 'height', 'scale', 'cropX', 'cropY', 'cropWidth', 'cropHeight', 'maxFileSize'].includes(cleanKey)) {
            result[cleanKey] = value ? parseFloat(value) : true;
          } else {
            result[cleanKey] = value || true;
          }
        } else {
          remainingArgs.push(arg);
        }
      }
      
      result.args = remainingArgs;
      return result;
    }
    
    // Return as-is if no CLI args to parse
    return params;
  }
}

export default ScreenshotCommand;