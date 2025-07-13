/**
 * Screenshot Command - TypeScript Implementation
 * Elegant screenshot capture with advanced targeting and orchestration
 * Uses proper daemon bus architecture for browser communication
 */

import { RemoteCommand, RemoteExecutionResponse } from '../../core/remote-command/RemoteCommand';
import { CommandDefinition, CommandResult } from '../../core/base-command/BaseCommand';
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

// ScreenshotParams interface removed - parameters defined in command definition


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
    } catch (error) {
      // Fallback definition if README.md not found
      return {
        name: 'screenshot',
        category: 'browser',
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

  protected static async processClientResponse(response: RemoteExecutionResponse, _originalParams: any, context?: any): Promise<CommandResult> {
    if (!response.success) {
      return this.createErrorResult(`Screenshot capture failed: ${response.error}`);
    }

    const { imageData, filename, selector, format, width, height } = response.data;
    
    // Extract base64 data from data URL
    const base64Data = imageData.replace(/^data:image\/[a-z]+;base64,/, '');
    const buffer = Buffer.from(base64Data, 'base64');
    
    console.log(`💾 SCREENSHOT: Saving file ${filename} (${buffer.length} bytes)`);
    
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
        return this.createErrorResult(`Screenshot marshal failed: ${marshalResult.error}`);
      }

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
        return this.createErrorResult(`File save failed: ${fileResult.error}`);
      }
      
      console.log(`✅ SCREENSHOT: File saved successfully to session directory`);
      
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
      console.error(`❌ SCREENSHOT: File save error:`, errorMessage);
      return this.createErrorResult(`Screenshot file save failed: ${errorMessage}`);
    }
  }

  // Utility methods removed - using simplified approach with FileWriteCommand integration
}

export default ScreenshotCommand;