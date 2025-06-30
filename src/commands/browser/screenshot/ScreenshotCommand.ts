/**
 * Screenshot Command - TypeScript Implementation
 * Elegant screenshot capture with advanced targeting and orchestration
 */

import { RemoteCommand, RemoteExecutionRequest, RemoteExecutionResponse } from '../../core/remote-command/RemoteCommand.js';
import { CommandDefinition, CommandResult } from '../../core/base-command/BaseCommand.js';
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

interface ScreenshotParams {
  selector?: string;
  filename?: string;
  subdirectory?: string;
  name_prefix?: string;
  scale?: number;
  manual?: boolean;
  source?: string;
  destination?: ScreenshotDestination;
  animation?: ScreenshotAnimation;
  roi?: boolean;
  // format is inferred from filename extension
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
    try {
      const readmePath = path.join(__dirname, 'README.md');
      // TODO: Use file/read command instead of direct fs access
      const fs = require('fs');
      const readme = fs.readFileSync(readmePath, 'utf8');
      const definition = this.parseReadmeDefinition(readme);
      
      return {
        name: definition.name || 'screenshot',
        category: definition.category || 'Browser',
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
    } catch (error) {
      // Fallback definition if README.md not found
      return {
        name: 'screenshot',
        category: 'Browser',
        icon: '📸',
        description: 'Capture browser screenshot with advanced targeting',
        parameters: { selector: 'string', filename: 'string', subdirectory: 'string' },
        examples: [
          { description: 'Save to file', command: `{"filename": "homepage.png", "destination": "${ScreenshotDestination.FILE}"}` },
          { description: 'Return bytes only', command: `{"selector": ".main-content", "destination": "${ScreenshotDestination.BYTES}"}` }
        ],
        usage: 'Capture screenshots with optional element targeting and custom naming'
      };
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

  protected static async executeOnClient(request: RemoteExecutionRequest): Promise<RemoteExecutionResponse> {
    console.log(`🖼️ SCREENSHOT: Executing on client with params:`, request.params);
    
    const options = this.parseParams<ScreenshotParams>(request.params);
    
    // Default parameters for elegant API
    const {
      selector = 'body',
      filename,
      subdirectory,
      name_prefix = 'screenshot',
      scale = 2.0,
      source = 'unknown',
      destination = ScreenshotDestination.FILE
    } = options;
    
    // Infer format from filename extension
    const format = this.getFormatFromFilename(filename, name_prefix);
    
    console.log(`📸 SCREENSHOT Command: ${source} requesting ${selector} -> ${destination} (${name_prefix})`);
    
    // Generate filename/id for tracking
    const timestamp = Date.now();
    const generatedFilename = this.generateFilename(filename, subdirectory, name_prefix, format, destination, timestamp);
    
    // Browser-side screenshot execution using html2canvas
    try {
      // This runs in the browser context
      const element = document.querySelector(selector) as HTMLElement;
      if (!element) {
        return {
          success: false,
          error: `Element not found: ${selector}`,
          clientMetadata: {
            userAgent: navigator.userAgent,
            timestamp: Date.now(),
            executionTime: 0
          }
        };
      }

      // Use html2canvas for screenshot capture
      const startTime = Date.now();
      const canvas = await (window as any).html2canvas(element, {
        scale: scale,
        useCORS: true,
        allowTaint: true
      });
      
      const imageData = canvas.toDataURL(`image/${format}`);
      const executionTime = Date.now() - startTime;
      
      return {
        success: true,
        data: {
          imageData,
          selector,
          filename: generatedFilename,
          format,
          width: canvas.width,
          height: canvas.height
        },
        clientMetadata: {
          userAgent: navigator.userAgent,
          timestamp: Date.now(),
          executionTime
        }
      };
      
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error),
        clientMetadata: {
          userAgent: navigator.userAgent,
          timestamp: Date.now(),
          executionTime: 0
        }
      };
    }
  }

  /**
   * Process client response and orchestrate file saving through command chaining
   */
  protected static async processClientResponse(response: RemoteExecutionResponse, originalParams: any): Promise<CommandResult> {
    if (!response.success) {
      return this.createErrorResult(`Screenshot capture failed: ${response.error}`);
    }

    const { imageData, filename, selector, format, width, height } = response.data;
    const options = this.parseParams<ScreenshotParams>(originalParams);
    
    const base64Data = imageData.replace(/^data:image\/[a-z]+;base64,/, '');
    const buffer = Buffer.from(base64Data, 'base64');
    
    // Handle different destination modes using strongly typed enums
    switch (options.destination) {
      case ScreenshotDestination.FILE:
        if (!filename) {
          return this.createErrorResult('Filename required when destination is FILE');
        }
        return this.createSuccessResult(
          `Screenshot captured - ready for file save command`,
          {
            filename: path.resolve(filename),
            selector,
            dimensions: { width, height },
            format,
            size: buffer.length,
            client: response.clientMetadata,
            nextCommand: {
              command: 'file_write',
              params: {
                filename: path.resolve(filename),
                content: base64Data,
                encoding: 'base64',
                ensureDirectory: true
              }
            }
          }
        );

      case ScreenshotDestination.BYTES:
        return this.createSuccessResult(
          `Screenshot captured - returning bytes`,
          {
            imageData: base64Data,
            selector,
            dimensions: { width, height },
            format,
            size: buffer.length,
            client: response.clientMetadata
          }
        );

      case ScreenshotDestination.BOTH:
        if (!filename) {
          return this.createErrorResult('Filename required when destination is BOTH');
        }
        return this.createSuccessResult(
          `Screenshot captured - file and bytes available`,
          {
            imageData: base64Data,
            filename: path.resolve(filename),
            selector,
            dimensions: { width, height },
            format,
            size: buffer.length,
            client: response.clientMetadata,
            nextCommand: {
              command: 'file_write',
              params: {
                filename: path.resolve(filename),
                content: base64Data,
                encoding: 'base64',
                ensureDirectory: true
              }
            }
          }
        );

      default:
        // Default to bytes-only for backward compatibility
        return this.createSuccessResult(
          `Screenshot captured successfully`,
          {
            imageData: base64Data,
            selector,
            dimensions: { width, height },
            format,
            client: response.clientMetadata
          }
        );
    }
  }

  /**
   * Extract format from filename extension or default to PNG
   */
  private static getFormatFromFilename(filename?: string, name_prefix?: string): ScreenshotFormat {
    const targetFilename = filename || `${name_prefix || 'screenshot'}.png`;
    const extension = path.extname(targetFilename).toLowerCase().replace('.', '');
    
    switch (extension) {
      case 'jpg':
      case 'jpeg': return ScreenshotFormat.JPG;
      case 'webp': return ScreenshotFormat.WEBP;
      case 'png':
      default: return ScreenshotFormat.PNG;
    }
  }

  /**
   * Generate filename based on parameters and destination
   */
  private static generateFilename(
    filename: string | undefined,
    subdirectory: string | undefined,
    name_prefix: string,
    format: ScreenshotFormat,
    destination: ScreenshotDestination,
    timestamp: number
  ): string | null {
    if (destination === ScreenshotDestination.FILE || destination === ScreenshotDestination.BOTH) {
      const baseName = filename || `${name_prefix}_${timestamp}.${format}`;
      return subdirectory ? `${subdirectory}/${baseName}` : baseName;
    }
    return null;
  }
}

export default ScreenshotCommand;