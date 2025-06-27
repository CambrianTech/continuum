/**
 * Screenshot Command - TypeScript Implementation
 * Elegant screenshot capture with advanced targeting and orchestration
 */

import { BaseCommand, CommandDefinition, CommandContext, CommandResult } from '../../core/BaseCommand';
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

interface ScreenshotResult {
  filename?: string;
  selector: string;
  timestamp: number;
  destination: string;
  requestId: string;
  workflow: string;
  orchestration: boolean;
}

interface ContinuonAnimation {
  enabled: boolean;
  type: string;
  showROI: boolean;
  fromRing: boolean;
}

interface ScreenshotCallback {
  command: string;
  params: {
    type: string;
    filename?: string;
    requestId: string;
    source: string;
  };
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
    try {
      const readmePath = path.join(__dirname, 'README.md');
      const readme = fs.readFileSync(readmePath, 'utf8');
      const definition = this.parseReadmeDefinition(readme);
      
      return {
        name: definition.name || 'screenshot',
        category: definition.category || 'Browser',
        icon: definition.icon || '📸',
        description: definition.description || 'Capture browser screenshot with advanced targeting',
        params: JSON.stringify(definition.parameters),
        examples: [
          '{"filename": "homepage.png"}',
          '{"selector": ".main-content", "filename": "content.png"}',
          '{"subdirectory": "ui-tests", "filename": "test-result.png"}'
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
        params: '{"selector?": "string", "filename?": "string", "subdirectory?": "string"}',
        examples: [
          '{"filename": "homepage.png"}',
          '{"selector": ".main-content", "filename": "content.png"}'
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

  static async execute(params: any, context?: CommandContext): Promise<CommandResult<ScreenshotResult>> {
    this.logExecution('Screenshot', params, context);
    
    console.log(`🔬 PROBE: ScreenshotCommand.execute called`);
    console.log('🔥 SCREENSHOT_COMMAND: Execute called with params:', params);
    
    const continuum = context?.continuum;
    if (!continuum?.webSocketServer) {
      return this.createErrorResult('WebSocket server not available for screenshot capture');
    }

    console.log('🔥 SCREENSHOT_COMMAND: Continuum object keys:', Object.keys(continuum || {}));
    
    const options = this.parseParams<ScreenshotParams>(params);
    
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
    
    // Continuon animation setup
    this.setupContinuonAnimation(animation, roi);
    
    // Generate filename/id for tracking
    const timestamp = Date.now();
    const generatedFilename = this.generateFilename(filename, subdirectory, name_prefix, format, destination, timestamp);
    const requestId = `${source}_${timestamp}`;
    
    // Create screenshot orchestration message
    const screenshotMessage = this.createScreenshotMessage(
      selector, scale, generatedFilename, manual, source, timestamp,
      format, destination, requestId, animation, roi
    );
    
    console.log(`📸 Orchestrating screenshot: browser capture → WSTransfer → ${destination === 'file' ? 'FileSave' : 'bytes only'}`);
    
    // Send enhanced message with WSTransfer callback
    const enhancedMessage = this.addWSTransferCallback(screenshotMessage, generatedFilename, requestId, destination);
    
    continuum.webSocketServer.broadcast(enhancedMessage);
    
    // Return orchestration result
    const result: ScreenshotResult = {
      filename: generatedFilename,
      selector,
      timestamp,
      destination,
      requestId,
      workflow: `html2canvas → WSTransfer → ${destination === 'file' ? 'FileSave' : 'bytes only'}`,
      orchestration: true
    };
    
    return this.createSuccessResult(result, `Screenshot workflow initiated (${destination} mode)`);
  }

  /**
   * Setup Continuon animation logging based on animation type
   */
  private static setupContinuonAnimation(animation: string, roi: boolean): void {
    if (animation === 'animated' && roi) {
      console.log('🟢 Continuon will animate ROI highlighting for screenshot');
    } else if (animation === 'visible' && roi) {
      console.log('🟢 Continuon will show ROI highlighting without animation');
    } else {
      console.log('🟢 Continuon will capture screenshot without ROI highlighting');
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

  /**
   * Create the base screenshot message for browser communication
   */
  private static createScreenshotMessage(
    selector: string,
    scale: number,
    filename: string | null,
    manual: boolean,
    source: string,
    timestamp: number,
    format: string,
    destination: string,
    requestId: string,
    animation: string,
    roi: boolean
  ) {
    const continuonAnimation: ContinuonAnimation = {
      enabled: animation === 'animated' || animation === 'visible',
      type: animation,
      showROI: roi,
      fromRing: true  // Continuon comes from the ring in top-left
    };

    return {
      type: 'command',
      command: 'screenshot',
      params: {
        selector,
        scale,
        filename,
        manual,
        source,
        timestamp,
        format,
        destination,
        requestId,
        animation,
        roi,
        continuonAnimation
      }
    };
  }

  /**
   * Add WSTransfer callback configuration to screenshot message
   */
  private static addWSTransferCallback(
    screenshotMessage: any,
    filename: string | null,
    requestId: string,
    destination: string
  ) {
    const callback: ScreenshotCallback = {
      command: 'wstransfer',
      params: {
        type: 'image',
        filename: destination === 'file' ? filename || undefined : undefined,
        requestId,
        source: 'screenshot'
      }
    };

    return {
      ...screenshotMessage,
      params: {
        ...screenshotMessage.params,
        callback
      }
    };
  }
}

export default ScreenshotCommand;