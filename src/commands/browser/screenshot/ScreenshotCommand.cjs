/**
 * Screenshot Command - Singular, elegant core functionality
 */

const BaseCommand = require('../../BaseCommand.cjs');

class ScreenshotCommand extends BaseCommand {
  static getDefinition() {
    // README-driven: Read definition from README.md
    const fs = require('fs');
    const path = require('path');
    
    try {
      const readmePath = path.join(__dirname, 'README.md');
      const readme = fs.readFileSync(readmePath, 'utf8');
      return this.parseReadmeDefinition(readme);
    } catch (error) {
      // Fallback definition if README.md not found
      return {
        name: 'screenshot',
        description: 'Capture browser screenshot with advanced targeting',
        icon: '📸',
        parameters: {
          selector: { type: 'string', required: false, description: 'CSS selector for element to capture' },
          filename: { type: 'string', required: false, description: 'Custom filename for screenshot' },
          subdirectory: { type: 'string', required: false, description: 'Subdirectory within screenshots workspace' }
        }
      };
    }
  }
  
  static parseReadmeDefinition(readme) {
    // Parse README.md for command definition
    const lines = readme.split('\n');
    const definition = { parameters: {} };
    
    let inDefinition = false;
    let inParams = false;
    let inTodos = false;
    const todos = [];
    
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
      definition.description += ' (⚠️ ' + todos.length + ' TODOs pending)';
    }
    
    return definition;
  }
  
  static async execute(params, continuum) {
    console.log(`🔬 PROBE: ScreenshotCommand.execute called`);
    console.log('🔥 SCREENSHOT_COMMAND: Execute called with params:', params);
    console.log('🔥 SCREENSHOT_COMMAND: Continuum object keys:', Object.keys(continuum || {}));
    const options = this.parseParams(params);
    
    // Default parameters for elegant API
    const {
      selector = 'body',
      filename,
      subdirectory,
      name_prefix = 'screenshot',
      scale = 1.0,
      manual = false,
      source = 'unknown',
      format = 'png',
      destination = 'file',  // 'file' or 'bytes'
      animation = 'visible',
      roi = true
    } = options;
    
    console.log(`📸 SCREENSHOT Command: ${source} requesting ${selector} -> ${destination} (${name_prefix}) [${animation}]`);
    
    // Continuon animation setup
    if (animation === 'animated' && roi) {
      console.log('🟢 Continuon will animate ROI highlighting for screenshot');
    } else if (animation === 'visible' && roi) {
      console.log('🟢 Continuon will show ROI highlighting without animation');
    } else {
      console.log('🟢 Continuon will capture screenshot without ROI highlighting');
    }
    
    // Generate filename/id for tracking - include subdirectory in filename path
    const timestamp = Date.now();
    let generatedFilename;
    if (destination === 'file') {
      const baseName = filename || `${name_prefix}_${timestamp}.${format}`;
      generatedFilename = subdirectory ? `${subdirectory}/${baseName}` : baseName;
    } else {
      generatedFilename = null;
    }
    const requestId = `${source}_${timestamp}`;
    
    // Send to browser with rich parameters
    const screenshotMessage = {
      type: 'command',
      command: 'screenshot',
      params: {
        selector,
        scale,
        filename: generatedFilename,
        manual,
        source,
        timestamp,
        format,
        destination,
        requestId,
        animation,
        roi,
        continuonAnimation: {
          enabled: animation === 'animated' || animation === 'visible',
          type: animation,
          showROI: roi,
          fromRing: true  // Continuon comes from the ring in top-left
        }
      }
    };
    
    console.log(`📸 Orchestrating screenshot: browser capture → WSTransfer → ${destination === 'file' ? 'FileSave' : 'bytes only'}`);
    
    // Step 1: Send html2canvas command to browser with proper WSTransfer callback
    const enhancedMessage = {
      ...screenshotMessage,
      params: {
        ...screenshotMessage.params,
        // Add WSTransfer callback configuration
        callback: {
          command: 'wstransfer',
          params: {
            type: 'image',
            filename: destination === 'file' ? generatedFilename : null,
            requestId,
            source: 'screenshot'
          }
        }
      }
    };
    
    continuum.webSocketServer.broadcast(enhancedMessage);
    
    // Step 2: Browser will execute html2canvas and automatically call WSTransfer with base64 data
    // Step 3: WSTransfer will conditionally orchestrate to FileSave if filename provided
    // This creates proper command orchestration that EventBus can track
    
    return this.createSuccessResult({
      filename: generatedFilename,
      selector,
      timestamp,
      destination,
      requestId,
      workflow: `html2canvas → WSTransfer → ${destination === 'file' ? 'FileSave' : 'bytes only'}`,
      orchestration: true
    }, `Screenshot workflow initiated (${destination} mode)`);
  }
}

module.exports = ScreenshotCommand;