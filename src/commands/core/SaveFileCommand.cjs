/**
 * Save File Command
 * Saves files to the filesystem from base64 blob data
 */

const fs = require('fs').promises;
const path = require('path');

class SaveFileCommand {
  static getDefinition() {
    return {
      name: 'SAVE_FILE',
      category: 'Core',
      icon: '💾',
      description: 'Save file from base64 data to filesystem',
      params: '{"filename": "name.ext", "directory": "path", "content": "base64_data", "mimeType": "type"}',
      examples: [
        '{"params": "{\\"filename\\": \\"test.txt\\", \\"directory\\": \\".continuum/screenshots\\", \\"content\\": \\"SGVsbG8gV29ybGQ=\\", \\"mimeType\\": \\"text/plain\\"}"}',
        '{"params": "{\\"filename\\": \\"image.png\\", \\"directory\\": \\".continuum/screenshots\\", \\"content\\": \\"iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==\\", \\"mimeType\\": \\"image/png\\"}"}'
      ],
      usage: 'Save files to server filesystem. Accepts JSON with filename, directory, base64 content, and mimeType. Creates directories as needed.'
    };
  }
  
  static async execute(params, continuum, encoding = 'utf-8') {
    console.log('💾 [SAVE_FILE] Command received');
    console.log('   📤 Raw params length:', params.length, 'chars');
    
    try {
      const data = JSON.parse(params);
      console.log('   📋 Parsed data:');
      console.log('      🏷️ Filename:', data.filename);
      console.log('      📁 Directory:', data.directory);
      console.log('      🎯 MIME type:', data.mimeType);
      console.log('      📊 Content length:', data.content?.length || 0, 'chars');
      console.log('      🔍 Metadata:', JSON.stringify(data.metadata || {}, null, 2));
      
      // Validate required fields
      if (!data.filename || !data.directory || !data.content) {
        console.log('   ❌ Missing required fields');
        return {
          success: false,
          error: 'Missing required fields: filename, directory, or content'
        };
      }
      
      // Construct full file path
      const fullPath = path.join(data.directory, data.filename);
      console.log('   🎯 FULL FILE PATH:', fullPath);
      console.log('   📍 Absolute path:', path.resolve(fullPath));
      
      // Create directory if needed
      console.log('   📁 Ensuring directory exists:', data.directory);
      await fs.mkdir(data.directory, { recursive: true });
      console.log('   ✅ Directory ready');
      
      // Decode base64 content
      console.log('   🔄 Decoding base64 content...');
      const buffer = Buffer.from(data.content, 'base64');
      console.log('   📊 Decoded buffer size:', buffer.length, 'bytes');
      
      // LOG IMMEDIATELY BEFORE SAVE
      console.log('   🚀 ABOUT TO SAVE FILE:');
      console.log('      📍 Path:', fullPath);
      console.log('      💾 Size:', buffer.length, 'bytes');
      console.log('      ⏰ Timestamp:', new Date().toISOString());
      
      // Write file to filesystem
      await fs.writeFile(fullPath, buffer);
      
      // Verify file was written
      const stats = await fs.stat(fullPath);
      console.log('   ✅ FILE SAVED SUCCESSFULLY!');
      console.log('      📁 Final path:', fullPath);
      console.log('      📊 File size on disk:', stats.size, 'bytes');
      console.log('      📅 Created:', stats.birthtime);
      console.log('      📝 Modified:', stats.mtime);
      
      // Log success summary
      console.log('   🎉 SAVE_FILE COMPLETE:');
      console.log('      ✅ Success: true');
      console.log('      🏷️ File:', data.filename);
      console.log('      📁 Location:', data.directory);
      console.log('      💾 Bytes written:', stats.size);
      
      return {
        success: true,
        filename: data.filename,
        fullPath: fullPath,
        absolutePath: path.resolve(fullPath),
        size: stats.size,
        created: stats.birthtime,
        modified: stats.mtime,
        mimeType: data.mimeType || 'application/octet-stream',
        metadata: data.metadata || {},
        message: `File saved successfully to ${fullPath}`,
        timestamp: new Date().toISOString()
      };
      
    } catch (error) {
      console.log('   ❌ SAVE_FILE ERROR:');
      console.log('      🚨 Error type:', error.constructor.name);
      console.log('      📝 Error message:', error.message);
      console.log('      📍 Error stack:', error.stack);
      console.log('      📊 Params received:', params.substring(0, 200) + '...');
      
      return {
        success: false,
        error: error.message,
        errorType: error.constructor.name,
        stack: error.stack,
        timestamp: new Date().toISOString()
      };
    }
  }
  
  static validateFileData(data) {
    const errors = [];
    
    // Check required fields
    if (!data.filename || typeof data.filename !== 'string') {
      errors.push('filename is required and must be a string');
    }
    
    if (!data.directory || typeof data.directory !== 'string') {
      errors.push('directory is required and must be a string');
    }
    
    if (!data.content || typeof data.content !== 'string') {
      errors.push('content is required and must be a base64 string');
    }
    
    // Validate filename
    if (data.filename && !/^[a-zA-Z0-9._-]+\.[a-zA-Z0-9]+$/.test(data.filename)) {
      errors.push('filename must have valid format with extension');
    }
    
    // Validate directory path
    if (data.directory && data.directory.includes('..')) {
      errors.push('directory cannot contain parent directory references (..)');
    }
    
    // Validate base64
    if (data.content) {
      try {
        Buffer.from(data.content, 'base64');
      } catch (e) {
        errors.push('content must be valid base64 data');
      }
    }
    
    return {
      valid: errors.length === 0,
      errors: errors
    };
  }
  
  static generateSafeDirectory(directory) {
    // Ensure directory is safe and within allowed paths
    const safePaths = [
      '.continuum',
      '.continuum/screenshots',
      '.continuum/files',
      '.continuum/exports',
      'tmp',
      'temp'
    ];
    
    const normalizedDir = path.normalize(directory);
    
    // Check if directory starts with any safe path
    const isSafe = safePaths.some(safePath => 
      normalizedDir === safePath || normalizedDir.startsWith(safePath + path.sep)
    );
    
    if (!isSafe) {
      throw new Error(`Directory ${directory} is not in allowed paths: ${safePaths.join(', ')}`);
    }
    
    return normalizedDir;
  }
}

module.exports = SaveFileCommand;