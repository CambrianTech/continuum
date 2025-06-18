/**
 * WSTransfer Command - Marshals base64 data across WebSocket
 * Handles binary data transfer between browser and server with conditional file saving
 */

const BaseCommand = require('../../BaseCommand.cjs');

class WSTransferCommand extends BaseCommand {
  static getDefinition() {
    return {
      name: 'wstransfer',
      description: 'Marshal base64 data across WebSocket with conditional file saving',
      icon: '📡',
      category: 'core',
      parameters: {
        data: { type: 'string', required: true, description: 'Base64 encoded data' },
        type: { type: 'string', required: false, description: 'Data type (image, file, etc.)' },
        filename: { type: 'string', required: false, description: 'Filename for saving (if provided, will save to file)' },
        requestId: { type: 'string', required: false, description: 'Request ID for tracking' },
        source: { type: 'string', required: false, description: 'Source command that generated data' }
      }
    };
  }
  
  static async execute(params, continuum) {
    console.log(`🔬 PROBE: WSTransferCommand.execute called`);
    console.log('📡 WSTransfer: Marshalling base64 data across WebSocket');
    console.log(`🔍 WSTransfer: Raw params type: ${typeof params}`);
    console.log(`🔍 WSTransfer: Raw params:`, params);
    
    const options = this.parseParams(params);
    const { data, type = 'unknown', filename, requestId, source = 'unknown' } = options;
    
    if (!data) {
      return this.createErrorResult('Base64 data is required');
    }
    
    console.log(`📡 WSTransfer: Processing ${data.length} bytes from ${source}`);
    console.log(`📡 WSTransfer: Type: ${type}, Filename: ${filename || 'none'}, RequestId: ${requestId || 'none'}`);
    
    // Validate base64 format
    try {
      const buffer = Buffer.from(data, 'base64');
      console.log(`📡 WSTransfer: Validated ${buffer.length} bytes of base64 data`);
    } catch (error) {
      console.error(`📡 WSTransfer: Invalid base64 data: ${error.message}`);
      return this.createErrorResult(`Invalid base64 data: ${error.message}`);
    }
    
    const result = {
      data,
      type,
      size: data.length,
      timestamp: Date.now(),
      requestId,
      source
    };
    
    // If filename provided, orchestrate to FileSave command
    if (filename) {
      console.log(`📡 WSTransfer: Filename provided (${filename}), orchestrating to FileSave`);
      console.log(`🔍 WSTransfer: Continuum available: ${!!continuum}`);
      console.log(`🔍 WSTransfer: CommandProcessor available: ${!!continuum?.commandProcessor}`);
      
      try {
        // Execute FileSave command with the base64 data
        const fileSaveResult = await continuum.commandProcessor.executeCommand('FILESAVE', {
          filename,
          data,
          encoding: 'base64',
          type,
          source: `wstransfer-${requestId || 'unknown'}`
        });
        
        if (fileSaveResult.success) {
          console.log(`📡 WSTransfer: File saved successfully via FileSave orchestration`);
          result.saved = true;
          result.filepath = fileSaveResult.filepath;
          result.orchestration = 'wstransfer → filesave';
        } else {
          console.error(`📡 WSTransfer: FileSave failed: ${fileSaveResult.error}`);
          result.saved = false;
          result.saveError = fileSaveResult.error;
        }
      } catch (error) {
        console.error(`📡 WSTransfer: FileSave orchestration error: ${error.message}`);
        result.saved = false;
        result.saveError = error.message;
      }
    } else {
      console.log(`📡 WSTransfer: No filename provided, returning base64 bytes only`);
      result.saved = false;
      result.bytesOnly = true;
    }
    
    // Publish transfer event to EventBus
    if (continuum && continuum.eventBus) {
      continuum.eventBus.processMessage('data_transfer', {
        type: 'wstransfer',
        dataSize: result.size,
        saved: result.saved,
        filename: filename || null,
        source,
        requestId,
        timestamp: result.timestamp
      }, 'wstransfer-command');
    }
    
    return this.createSuccessResult(result, 
      filename ? 
        `Base64 data marshaled and ${result.saved ? 'saved' : 'save failed'}` : 
        'Base64 data marshaled (bytes only)'
    );
  }
}

module.exports = WSTransferCommand;