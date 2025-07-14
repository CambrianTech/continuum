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

import { BaseCommand, CommandDefinition, CommandResult, CommandContext } from '../../core/base-command/BaseCommand';
import { normalizeCommandCategory } from '../../../types/shared/CommandTypes';
import * as path from 'path';

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
      // TODO: Use file/read command instead of direct fs access
      const fs = require('fs');
      const readme = fs.readFileSync(readmePath, 'utf8');
      const definition = this.parseReadmeDefinition(readme);
      
      console.log(`📖 JTAG: README definition parsed - name: ${definition.name}, params: ${Object.keys(definition.parameters).join(', ')}`);
      
      const finalDefinition = {
        name: definition.name || 'screenshot',
        category: normalizeCommandCategory(definition.category || 'browser'),
        icon: definition.icon || '📸',
        description: definition.description || 'Capture browser screenshot with advanced targeting',
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
      definition.description = (definition.description || '') + ` (⚠️ ${todos.length} TODOs pending)`;
    }
    
    return definition;
  }

  static async execute(params: ScreenshotParams | ScreenshotClientRequest, context?: CommandContext): Promise<CommandResult | any> {
    const startTime = Date.now();
    
    // Check if we're running in browser context (client-side)
    if (typeof window !== 'undefined' && typeof document !== 'undefined' && (window as any).html2canvas) {
      console.log(`📸 JTAG: Running in browser context - executing client-side`);
      return this.executeClient(params as ScreenshotClientRequest);
    }
    
    // Server-side execution
    console.log(`🚀 JTAG: Starting ScreenshotCommand execution (server side)`);
    console.log(`📋 JTAG: Parameters received:`, JSON.stringify(params, null, 2));
    
    try {
      // Prepare screenshot parameters
      const safeParams = params && typeof params === 'object' ? params : {};
      const filename = safeParams.filename || `screenshot-${Date.now()}.png`;
      const selector = safeParams.selector || 'body';
      const format = safeParams.format || this.inferFormatFromFilename(filename);
      
      const clientParams = {
        selector,
        filename,
        format,
        quality: safeParams.quality || 0.9,
        animation: safeParams.animation || ScreenshotAnimation.NONE,
        destination: safeParams.destination || ScreenshotDestination.FILE
      };
      
      console.log(`📤 JTAG: Using continuum.executeJS() to get screenshot bytes`);
      
      // Import and use JSExecuteCommand for server-side execution
      const { JSExecuteCommand } = await import('../../browser/js-execute/JSExecuteCommand');
      
      const executeResult = await JSExecuteCommand.execute({
        script: `
          // Call continuum.screenshot() on client side
          console.log('🔬 JTAG BROWSER: Calling continuum.screenshot()');
          
          if (!window.continuum || !window.continuum.screenshot) {
            throw new Error('window.continuum.screenshot not available');
          }
          
          const params = ${JSON.stringify(clientParams)};
          return await window.continuum.screenshot(params);
        `,
        returnResult: true
      }, context);
      
      console.log(`📨 JTAG: Got execute result:`, JSON.stringify(executeResult, null, 2));
      
      if (!executeResult.success) {
        throw new Error(executeResult.error || 'executeJS failed');
      }
      
      const clientResult = executeResult.data?.result;
      const executionTime = Date.now() - startTime;
      
      // Check if client-side already handled file saving
      if (clientResult?.saved) {
        console.log(`✅ JTAG: Screenshot completed in ${executionTime}ms - file saved by client`);
        return this.createSuccessResult({
          filename: clientResult.filename,
          filepath: clientResult.filepath,
          selector: clientResult.selector,
          format: clientResult.format,
          dimensions: clientResult.dimensions,
          size: clientResult.size,
          timestamp: clientResult.timestamp,
          artifactType: 'screenshot',
          savedBy: 'client'
        });
      }
      
      // Client didn't save file, check if we need to save on server
      if (filename && clientResult?.bytes) {
        console.log(`💾 JTAG: Client didn't save file, saving on server`);
        
        const { FileWriteCommand } = await import('../../file/write/FileWriteCommand');
        const fileResult = await FileWriteCommand.execute({
          content: clientResult.bytes,
          filename: filename,
          artifactType: 'screenshot',
          ...(context?.sessionId && { sessionId: context.sessionId })
        }, context);
        
        if (!fileResult.success) {
          throw new Error(`File save failed: ${fileResult.error}`);
        }
        
        console.log(`✅ JTAG: Screenshot completed in ${executionTime}ms - file saved by server`);
        return this.createSuccessResult({
          filename,
          filepath: fileResult.data?.filepath || filename,
          selector: clientResult.selector,
          format: clientResult.format,
          dimensions: clientResult.dimensions,
          size: clientResult.size,
          timestamp: clientResult.timestamp,
          artifactType: 'screenshot',
          savedBy: 'server'
        });
      }
      
      // No file saving requested, just return bytes info
      console.log(`✅ JTAG: Screenshot completed in ${executionTime}ms - bytes only`);
      return this.createSuccessResult({
        selector: clientResult.selector,
        format: clientResult.format,
        dimensions: clientResult.dimensions,
        size: clientResult.size,
        timestamp: clientResult.timestamp,
        artifactType: 'screenshot',
        savedBy: 'none'
      });
      
    } catch (error) {
      const executionTime = Date.now() - startTime;
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.error(`❌ JTAG: Screenshot execution failed after ${executionTime}ms: ${errorMessage}`);
      return this.createErrorResult(`Screenshot failed: ${errorMessage}`);
    }
  }

  /**
   * Browser-side execution with html2canvas - smart file saving
   */
  static async executeClient(params: ScreenshotClientRequest): Promise<any> {
    const startTime = Date.now();
    
    try {
      const { selector, format, quality, filename, destination } = params;
      
      console.log(`📸 JTAG BROWSER: Starting screenshot capture (client side) - selector: ${selector}, format: ${format}, filename: ${filename}`);
      console.log(`📋 JTAG BROWSER: Destination: ${destination}`);
      
      // Find target element
      const targetElement = selector === 'body' ? document.body : document.querySelector(selector);
      if (!targetElement) {
        throw new Error(`Element not found: ${selector}`);
      }
      
      // Import html2canvas (should be loaded in browser context)
      const html2canvas = (window as any).html2canvas;
      if (!html2canvas) {
        throw new Error('html2canvas not loaded - include it in your page');
      }
      
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
      const bytes = Uint8Array.from(atob(base64Data), c => c.charCodeAt(0));
      
      const executionTime = Date.now() - startTime;
      console.log(`✅ JTAG BROWSER: Screenshot bytes ready in ${executionTime}ms - size: ${bytes.length} bytes`);
      
      const result = {
        bytes,
        selector,
        format,
        dimensions: { width: canvas.width, height: canvas.height },
        size: bytes.length,
        timestamp: new Date().toISOString(),
        executionTime
      };
      
      // Smart file saving: if filename specified, save to server via continuum.fileSave()
      if (filename && (destination === ScreenshotDestination.FILE || destination === ScreenshotDestination.BOTH)) {
        console.log(`💾 JTAG BROWSER: Filename specified, calling continuum.fileSave() to save on server`);
        
        if (!(window as any).continuum || !(window as any).continuum.fileSave) {
          console.warn(`⚠️ JTAG BROWSER: continuum.fileSave() not available - returning bytes only`);
          return result;
        }
        
        try {
          const fileResult = await (window as any).continuum.fileSave({
            content: bytes,
            filename: filename,
            artifactType: 'screenshot'
          });
          
          console.log(`📁 JTAG BROWSER: File saved successfully - filepath: ${fileResult.filepath}`);
          
          return {
            ...result,
            filename,
            filepath: fileResult.filepath,
            saved: true
          };
        } catch (fileError) {
          console.error(`❌ JTAG BROWSER: File save failed:`, fileError);
          return {
            ...result,
            filename,
            saved: false,
            saveError: fileError instanceof Error ? fileError.message : String(fileError)
          };
        }
      }
      
      // Just return bytes (no file saving requested)
      console.log(`📤 JTAG BROWSER: Returning screenshot bytes only (no file saving)`);
      return result;
      
    } catch (error) {
      const executionTime = Date.now() - startTime;
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.error(`❌ JTAG BROWSER: Screenshot capture failed after ${executionTime}ms - ${errorMessage}`);
      throw error;
    }
  }

  /**
   * Infer image format from filename extension
   */
  private static inferFormatFromFilename(filename: string): ScreenshotFormat {
    const ext = filename.split('.').pop()?.toLowerCase();
    switch (ext) {
      case 'jpg':
      case 'jpeg':
        return ScreenshotFormat.JPEG;
      case 'webp':
        return ScreenshotFormat.WEBP;
      case 'png':
      default:
        return ScreenshotFormat.PNG;
    }
  }

}

export default ScreenshotCommand;