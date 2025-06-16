/**
 * Screenshot Command - Singular, elegant core functionality
 */

const BaseCommand = require('../../BaseCommand.cjs');

class ScreenshotCommand extends BaseCommand {
  static getDefinition() {
    return {
      name: 'screenshot',
      category: 'Core', 
      icon: '📸',
      description: 'Capture browser screenshot',
      parameters: {
        selector: {
          type: 'string',
          required: false,
          description: 'CSS selector for element to capture'
        },
        filename: {
          type: 'string',
          required: false,
          description: 'Custom filename for screenshot'
        },
        subdirectory: {
          type: 'string',
          required: false,
          description: 'Subdirectory within screenshots folder'
        },
        animation: {
          type: 'string',
          required: false,
          default: 'visible',
          description: 'Animation mode: visible, hidden, animated'
        },
        roi: {
          type: 'boolean',
          required: false,
          default: true,
          description: 'Show region of interest highlighting'
        }
      },
      examples: [
        'screenshot',
        'screenshot --selector .version-badge',
        'screenshot --filename test.png --subdirectory tests',
        'screenshot --animation animated --roi true',
        'screenshot --selector .chat-area --animation visible',
        'screenshot --animation hidden --roi false'
      ]
    };
  }
  
  static async execute(params, continuum) {
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
    
    console.log(`📸 Broadcasting screenshot command: ${generatedFilename}`);
    continuum.webSocketServer.broadcast(screenshotMessage);
    
    // For file destination, the promise completes when FileSave command finishes
    // The WebSocket server will call FileSave command when browser sends screenshot_data
    // This creates a proper command chain: Screenshot → Browser → WebSocket → FileSave
    
    if (destination === 'file') {
      // Screenshot command initiates the process, FileSave completes it
      return this.createSuccessResult({
        requestId,
        filename: generatedFilename,
        selector,
        scale,
        manual,
        source,
        timestamp,
        format,
        destination,
        message: `Screenshot initiated: ${generatedFilename} (will complete via FileSave command)`
      }, `Screenshot command chain started: ${generatedFilename}`);
    } else {
      // For bytes mode, return immediately 
      return this.createSuccessResult({
        requestId,
        selector,
        scale,
        manual,
        source,
        timestamp,
        format,
        destination,
        message: 'Screenshot bytes will be returned via WebSocket'
      }, `Screenshot bytes mode: ${requestId}`);
    }
  }
}

module.exports = ScreenshotCommand;