/**
 * FileSaveCommand - Universal file saving command
 * Completes Screenshot command promises and handles all file operations
 */

const BaseCommand = require('../../core/BaseCommand.cjs');
const fs = require('fs');
const path = require('path');

class FileSaveCommand extends BaseCommand {
  static getDefinition() {
    return {
      name: 'fileSave',
      description: 'Save file data to filesystem with directory structure support',
      icon: '💾',
      category: 'core',
      parameters: {
        filename: {
          type: 'string',
          required: true,
          description: 'File path relative to base directory (e.g., "tests/image.png")'
        },
        data: {
          type: 'string',
          required: true,
          description: 'Base64 encoded file data or data URL'
        },
        baseDirectory: {
          type: 'string',
          required: false,
          description: 'Base directory for file saving',
          default: '.continuum/screenshots'
        },
        mimeType: {
          type: 'string',
          required: false,
          description: 'MIME type of the file'
        }
      }
    };
  }

  static async execute(paramsString, continuum) {
    try {
      const params = this.parseParams(paramsString);
      const { filename, data, baseDirectory = '.continuum/screenshots', mimeType } = params;

      console.log(`💾 FileSave: Saving ${filename} to ${baseDirectory}`);

      // Parse directory structure from filename path
      const baseDir = path.join(process.cwd(), baseDirectory);
      const fullPath = path.join(baseDir, filename);
      const targetDir = path.dirname(fullPath);
      const actualFilename = path.basename(fullPath);

      console.log(`📁 FileSave: ${filename} -> dir: ${path.relative(baseDir, targetDir)}, file: ${actualFilename}`);

      // Handle data URL or base64 data
      let buffer;
      if (data.startsWith('data:')) {
        // Data URL format
        const base64Data = data.split(',')[1];
        buffer = Buffer.from(base64Data, 'base64');
      } else {
        // Assume raw base64
        buffer = Buffer.from(data, 'base64');
      }

      // Ensure target directory exists
      if (!fs.existsSync(targetDir)) {
        fs.mkdirSync(targetDir, { recursive: true });
        console.log(`📁 FileSave: Created directory: ${targetDir}`);
      }

      // Write file
      fs.writeFileSync(fullPath, buffer);
      
      const fileSizeKB = Math.round(buffer.length / 1024);
      const fileSize = buffer.length;
      
      console.log(`✅ FileSave: Saved ${filename} (${fileSizeKB}KB)`);

      // Return success with file details
      return this.createSuccessResult({
        filename,
        fullPath,
        fileSize,
        fileSizeKB,
        directory: path.relative(baseDir, targetDir),
        actualFilename,
        mimeType: mimeType || this.detectMimeType(filename)
      }, `File saved: ${filename} (${fileSizeKB}KB)`);

    } catch (error) {
      console.error(`❌ FileSave failed: ${error.message}`);
      return this.createErrorResult(error.message);
    }
  }

  static detectMimeType(filename) {
    const ext = path.extname(filename).toLowerCase();
    const mimeTypes = {
      '.png': 'image/png',
      '.jpg': 'image/jpeg',
      '.jpeg': 'image/jpeg',
      '.gif': 'image/gif',
      '.webp': 'image/webp',
      '.svg': 'image/svg+xml',
      '.txt': 'text/plain',
      '.json': 'application/json',
      '.pdf': 'application/pdf'
    };
    return mimeTypes[ext] || 'application/octet-stream';
  }
}

module.exports = FileSaveCommand;