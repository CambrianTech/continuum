/**
 * Screenshot Command - TypeScript Implementation
 * Elegant screenshot capture with advanced targeting and orchestration
 */

import { RemoteCommand, RemoteExecutionRequest, RemoteExecutionResponse } from '../../core/remote-command/RemoteCommand.js';
import { CommandDefinition, CommandContext, CommandResult } from '../../core/base-command/BaseCommand.js';
import * as fs from 'fs';
import * as path from 'path';

interface ScreenshotParams {
  selector?: string;
  filename?: string;
  subdirectory?: string;
  name_prefix?: string;
  scale?: number;
  manual?: boolean;
  source?: string;
  format?: 'png' | 'jpg' | 'jpeg' | 'webp';
  destination?: 'file' | 'bytes';
  animation?: 'visible' | 'animated' | 'none';
  roi?: boolean;
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
      const readme = fs.readFileSync(readmePath, 'utf8');
      const definition = this.parseReadmeDefinition(readme);
      
      return {
        name: definition.name || 'screenshot',
        category: definition.category || 'Browser',
        icon: definition.icon || '📸',
        description: definition.description || 'Capture browser screenshot with advanced targeting',
        parameters: definition.parameters,
        examples: [
          { description: 'Basic screenshot', command: '{"filename": "homepage.png"}' },
          { description: 'Element screenshot', command: '{"selector": ".main-content", "filename": "content.png"}' },
          { description: 'Organized screenshot', command: '{"subdirectory": "ui-tests", "filename": "test-result.png"}' }
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
          { description: 'Basic screenshot', command: '{"filename": "homepage.png"}' },
          { description: 'Element screenshot', command: '{"selector": ".main-content", "filename": "content.png"}' }
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
      manual = false,
      source = 'unknown',
      format = 'png',
      destination = 'file',
      animation = 'visible',
      roi = true
    } = options;
    
    console.log(`📸 SCREENSHOT Command: ${source} requesting ${selector} -> ${destination} (${name_prefix}) [${animation}]`);
    
    // Generate filename/id for tracking
    const timestamp = Date.now();
    const generatedFilename = this.generateFilename(filename, subdirectory, name_prefix, format, destination, timestamp);
    const requestId = `${source}_${timestamp}`;
    
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
   * Process client response and save screenshot to file system
   */
  protected static async processClientResponse(response: RemoteExecutionResponse, originalParams: any): Promise<CommandResult> {
    if (!response.success) {
      return this.createErrorResult(`Screenshot capture failed: ${response.error}`);
    }

    const { imageData, filename, selector, format, width, height } = response.data;
    const options = this.parseParams<ScreenshotParams>(originalParams);
    
    // Save to file if filename provided
    if (filename && options.destination === 'file') {
      try {
        const base64Data = imageData.replace(/^data:image\/[a-z]+;base64,/, '');
        const buffer = Buffer.from(base64Data, 'base64');
        
        // Ensure directory exists
        const fullPath = path.resolve(filename);
        const dir = path.dirname(fullPath);
        if (!fs.existsSync(dir)) {
          fs.mkdirSync(dir, { recursive: true });
        }
        
        // Write file
        fs.writeFileSync(fullPath, buffer);
        
        return this.createSuccessResult(
          `Screenshot saved successfully`,
          {
            filename: fullPath,
            selector,
            dimensions: { width, height },
            format,
            size: buffer.length,
            client: response.clientMetadata
          }
        );
      } catch (error) {
        return this.createErrorResult(`Failed to save screenshot: ${error instanceof Error ? error.message : String(error)}`);
      }
    } else {
      // Return bytes only
      return this.createSuccessResult(
        `Screenshot captured successfully`,
        {
          imageData,
          selector,
          dimensions: { width, height },
          format,
          client: response.clientMetadata
        }
      );
    }
  }

  /**
   * Generate filename based on parameters and destination
   */
  private static generateFilename(
    filename: string | undefined,
    subdirectory: string | undefined,
    name_prefix: string,
    format: string,
    destination: string,
    timestamp: number
  ): string | null {
    if (destination === 'file') {
      const baseName = filename || `${name_prefix}_${timestamp}.${format}`;
      return subdirectory ? `${subdirectory}/${baseName}` : baseName;
    }
    return null;
  }
}

export default ScreenshotCommand;