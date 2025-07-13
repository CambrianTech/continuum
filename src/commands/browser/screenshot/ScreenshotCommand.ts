/**
 * Screenshot Command - TypeScript Implementation
 * Elegant screenshot capture with advanced targeting and orchestration
 * Uses proper daemon bus architecture for browser communication
 */

import { RemoteCommand, RemoteExecutionResponse, RemoteExecutionRequest } from '../../core/remote-command/RemoteCommand';
import { CommandDefinition, CommandResult, CommandContext } from '../../core/base-command/BaseCommand';
import { normalizeCommandCategory } from '../../../types/shared/CommandTypes';
import { RemoteCommandType } from '../../../types/shared/CommandOperationTypes';
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
export class ScreenshotCommand extends RemoteCommand {
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

  protected static async processClientResponse(response: RemoteExecutionResponse, _originalParams: ScreenshotParams, context?: CommandContext): Promise<CommandResult> {
    console.log(`📨 JTAG: Processing screenshot response - success: ${response.success}`);
    
    if (!response.success) {
      console.error(`❌ JTAG: Screenshot capture failed: ${response.error}`);
      return this.createErrorResult(`Screenshot capture failed: ${response.error}`);
    }

    const responseData = response.data as ScreenshotClientResponse;
    const { imageData, filename, selector, format, width, height } = responseData;
    
    console.log(`📊 JTAG: Screenshot response data - selector: ${selector}, dimensions: ${width}x${height}, format: ${format}`);
    
    // Extract base64 data from data URL
    const base64Data = imageData.replace(/^data:image\/[a-z]+;base64,/, '');
    const buffer = Buffer.from(base64Data, 'base64');
    
    console.log(`💾 JTAG: Converting base64 to buffer - original data: ${imageData.length} chars, buffer: ${buffer.length} bytes`);
    console.log(`📂 JTAG: Saving screenshot file ${filename} (${buffer.length} bytes) to session directory`);
    
    try {
      // Pipe screenshot data through marshal → file write chain
      const screenshotData = {
        imageData,
        filename,
        selector,
        format,
        width,
        height,
        size: buffer.length,
        timestamp: new Date().toISOString(),
        artifactType: 'screenshot'
      };

      console.log(`📦 JTAG: Marshalling screenshot data with ${Object.keys(screenshotData).length} properties`);

      // 1. Marshal data (creates correlation ID and emits events for subscribers)
      const { DataMarshalCommand } = await import('../../core/data-marshal/DataMarshalCommand');
      const marshalResult = await DataMarshalCommand.execute({
        operation: 'encode',
        data: screenshotData,
        encoding: 'json',
        source: 'screenshot',
        destination: 'file-write'
      }, context);

      if (!marshalResult.success) {
        console.error(`❌ JTAG: Screenshot marshal failed: ${marshalResult.error}`);
        return this.createErrorResult(`Screenshot marshal failed: ${marshalResult.error}`);
      }

      console.log(`✅ JTAG: Data marshalled successfully with ID: ${marshalResult.data?.marshalId}`);

      // 2. Pipe marshalled data to file write (subscribers can listen to marshal events)
      const { FileWriteCommand } = await import('../../file/write/FileWriteCommand');
      const fileResult = await FileWriteCommand.execute({
        content: buffer,
        filename: filename,
        artifactType: 'screenshot',
        ...(context?.sessionId && { sessionId: context.sessionId }),
        // Include marshal correlation for tracking the pipe
        marshalId: marshalResult.data?.marshalId
      }, context);
      
      if (!fileResult.success) {
        console.error(`❌ JTAG: File save failed: ${fileResult.error}`);
        return this.createErrorResult(`File save failed: ${fileResult.error}`);
      }
      
      console.log(`✅ JTAG: Screenshot file saved successfully to: ${fileResult.data?.filepath}`);
      console.log(`📈 JTAG: Screenshot stats - ${width}x${height} ${format} image, ${buffer.length} bytes`);
      
      return this.createSuccessResult({
        filename: filename,
        filepath: fileResult.data?.filepath || filename,
        selector: selector,
        dimensions: { width, height },
        format: format,
        size: buffer.length,
        sessionId: context?.sessionId,
        timestamp: new Date().toISOString(),
        artifactType: 'screenshot'
      });
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.error(`❌ JTAG: Screenshot processing error: ${errorMessage}`);
      return this.createErrorResult(`Screenshot file save failed: ${errorMessage}`);
    }
  }

  /**
   * Server-side preparation before sending to browser
   */
  protected static async prepareForRemoteExecution(params: ScreenshotParams, context?: CommandContext): Promise<RemoteExecutionRequest> {
    const parsedParams = this.parseParams<ScreenshotParams>(params);
    
    // Infer format from filename if not provided
    const filename = parsedParams.filename || `screenshot-${Date.now()}.png`;
    const format = parsedParams.format || this.inferFormatFromFilename(filename);
    
    const clientRequest: ScreenshotClientRequest = {
      selector: parsedParams.selector || 'body',
      filename,
      format,
      quality: parsedParams.quality || 0.9,
      animation: parsedParams.animation || ScreenshotAnimation.NONE,
      destination: parsedParams.destination || ScreenshotDestination.FILE
    };
    
    return {
      command: RemoteCommandType.SCREENSHOT,
      params: clientRequest,
      sessionId: context?.sessionId,
      timeout: this.getRemoteTimeout()
    };
  }

  /**
   * Browser-side execution with html2canvas
   */
  protected static async executeOnClient(request: RemoteExecutionRequest): Promise<RemoteExecutionResponse> {
    const startTime = Date.now();
    
    try {
      // This code will run in the browser context
      const clientRequest = request.params as ScreenshotClientRequest;
      const { selector, format, quality, animation, filename } = clientRequest;
      
      console.log(`📸 JTAG: Starting screenshot capture - selector: ${selector}, format: ${format}, filename: ${filename}`);
      
      // Show animation if requested
      if (animation !== ScreenshotAnimation.NONE) {
        console.log(`✨ JTAG: Showing screenshot animation (${animation}) for selector: ${selector}`);
        this.showScreenshotAnimation(selector, animation);
      }
      
      // Find target element
      const targetElement = selector === 'body' ? document.body : document.querySelector(selector);
      if (!targetElement) {
        const error = `Element not found: ${selector}`;
        console.error(`❌ JTAG: ${error}`);
        throw new Error(error);
      }
      
      const elementRect = targetElement.getBoundingClientRect();
      console.log(`🎯 JTAG: Target element found - dimensions: ${elementRect.width}x${elementRect.height}`);
      
      // Import html2canvas dynamically (should be loaded in browser context)
      const html2canvas = (window as any).html2canvas;
      if (!html2canvas) {
        const error = 'html2canvas not loaded - include it in your page';
        console.error(`❌ JTAG: ${error}`);
        throw new Error(error);
      }
      
      console.log(`📦 JTAG: html2canvas available - starting capture with quality: ${quality}`);
      
      // Capture screenshot
      const canvas = await html2canvas(targetElement, {
        allowTaint: true,
        useCORS: true,
        scale: 1,
        logging: false,
        onclone: (clonedDoc: Document) => {
          // Clean up any animation artifacts in the cloned document
          const animationElements = clonedDoc.querySelectorAll('.screenshot-animation');
          animationElements.forEach(el => el.remove());
          console.log(`🧹 JTAG: Cleaned ${animationElements.length} animation elements from cloned document`);
        }
      });
      
      console.log(`🖼️ JTAG: Canvas created - dimensions: ${canvas.width}x${canvas.height}`);
      
      // Convert to desired format
      const imageData = format === ScreenshotFormat.PNG ? 
        canvas.toDataURL('image/png') : 
        canvas.toDataURL(`image/${format}`, quality);
      
      // Calculate image size
      const imageSize = Math.round((imageData.length * 3) / 4); // Rough base64 to bytes conversion
      console.log(`📏 JTAG: Image data generated - format: ${format}, size: ${imageSize} bytes (${imageData.length} chars)`);
      
      // Hide animation
      if (animation !== ScreenshotAnimation.NONE) {
        console.log(`✨ JTAG: Hiding screenshot animation`);
        this.hideScreenshotAnimation();
      }
      
      const executionTime = Date.now() - startTime;
      console.log(`⏱️ JTAG: Screenshot capture completed in ${executionTime}ms`);
      
      const responseData: ScreenshotClientResponse = {
        imageData,
        filename,
        selector,
        format,
        width: canvas.width,
        height: canvas.height
      };
      
      return {
        success: true,
        data: responseData,
        clientMetadata: {
          userAgent: navigator.userAgent,
          timestamp: Date.now(),
          executionTime
        }
      };
      
    } catch (error) {
      const executionTime = Date.now() - startTime;
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.error(`❌ JTAG: Screenshot capture failed after ${executionTime}ms - ${errorMessage}`);
      
      return {
        success: false,
        error: errorMessage,
        clientMetadata: {
          userAgent: navigator.userAgent,
          timestamp: Date.now(),
          executionTime
        }
      };
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

  /**
   * Show screenshot animation (runs in browser)
   */
  private static showScreenshotAnimation(selector: string, animation: ScreenshotAnimation): void {
    if (animation === ScreenshotAnimation.NONE) return;
    
    const targetElement = selector === 'body' ? document.body : document.querySelector(selector);
    if (!targetElement) return;
    
    // Create animation overlay
    const overlay = document.createElement('div');
    overlay.className = 'screenshot-animation';
    overlay.style.cssText = `
      position: absolute;
      top: 0;
      left: 0;
      right: 0;
      bottom: 0;
      border: 3px solid #007acc;
      pointer-events: none;
      z-index: 9999;
      ${animation === ScreenshotAnimation.ANIMATED ? 'animation: screenshot-pulse 0.5s ease-in-out;' : ''}
    `;
    
    // Add animation keyframes if not already present
    if (!document.querySelector('#screenshot-animation-styles')) {
      const style = document.createElement('style');
      style.id = 'screenshot-animation-styles';
      style.textContent = `
        @keyframes screenshot-pulse {
          0% { opacity: 0; transform: scale(1.05); }
          50% { opacity: 1; transform: scale(1); }
          100% { opacity: 0.7; transform: scale(1); }
        }
      `;
      document.head.appendChild(style);
    }
    
    // Position overlay relative to target
    const rect = targetElement.getBoundingClientRect();
    overlay.style.position = 'fixed';
    overlay.style.top = rect.top + 'px';
    overlay.style.left = rect.left + 'px';
    overlay.style.width = rect.width + 'px';
    overlay.style.height = rect.height + 'px';
    
    document.body.appendChild(overlay);
  }

  /**
   * Hide screenshot animation (runs in browser)
   */
  private static hideScreenshotAnimation(): void {
    const animations = document.querySelectorAll('.screenshot-animation');
    animations.forEach(el => el.remove());
  }
}

export default ScreenshotCommand;