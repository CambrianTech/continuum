/**
 * Static File Transport - Serves static files through the JTAG router
 * 
 * Instead of handling static files directly in the server, route them through
 * the universal router system to demonstrate transport flexibility
 */

import { JTAGUniversalMessage } from './JTAGTypes';
import { JTAGTransportBackend } from './JTAGRouter';
import * as path from 'path';
import * as fs from 'fs';

export class StaticFileTransport implements JTAGTransportBackend {
  name = 'static-files';
  private basePath: string;

  constructor(basePath?: string) {
    this.basePath = basePath || path.join(__dirname, '..', 'examples');
  }

  canHandle(message: JTAGUniversalMessage): boolean {
    // Handle messages with type 'static-file' 
    return message.type === 'static-file';
  }

  async process(message: JTAGUniversalMessage): Promise<any> {
    const payload = message.payload as any;
    const filename = payload.filename;
    const requestPath = payload.path || '/';

    console.log(`📁 Static File Transport: Serving ${filename} via router`);

    try {
      const filePath = this.resolveFilePath(filename);
      
      if (!this.isValidFile(filePath)) {
        return {
          success: false,
          status: 404,
          error: 'File not found or access denied',
          filename
        };
      }

      const content = fs.readFileSync(filePath, 'utf8');
      const stats = fs.statSync(filePath);
      const contentType = this.getContentType(filename);

      return {
        success: true,
        status: 200,
        content,
        headers: {
          'Content-Type': contentType,
          'Content-Length': stats.size,
          'Last-Modified': stats.mtime.toUTCString()
        },
        filename,
        size: stats.size,
        transport: 'router-static'
      };

    } catch (error) {
      console.error(`❌ Static File Transport: Failed to serve ${filename}:`, error);
      return {
        success: false,
        status: 500,
        error: error instanceof Error ? error.message : String(error),
        filename
      };
    }
  }

  private resolveFilePath(filename: string): string {
    // Security: prevent directory traversal
    const sanitized = filename.replace(/\.\./g, '').replace(/^\/+/, '');
    return path.join(this.basePath, sanitized);
  }

  private isValidFile(filePath: string): boolean {
    try {
      // Security checks
      const resolvedPath = path.resolve(filePath);
      const basePath = path.resolve(this.basePath);
      
      // Must be within base directory
      if (!resolvedPath.startsWith(basePath)) {
        return false;
      }

      // File must exist and be readable
      fs.accessSync(filePath, fs.constants.R_OK);
      const stats = fs.statSync(filePath);
      
      return stats.isFile();
    } catch (error) {
      return false;
    }
  }

  private getContentType(filename: string): string {
    const ext = path.extname(filename).toLowerCase();
    
    const mimeTypes: Record<string, string> = {
      '.html': 'text/html',
      '.css': 'text/css',
      '.js': 'application/javascript',
      '.json': 'application/json',
      '.png': 'image/png',
      '.jpg': 'image/jpeg',
      '.jpeg': 'image/jpeg',
      '.gif': 'image/gif',
      '.svg': 'image/svg+xml',
      '.txt': 'text/plain',
      '.md': 'text/markdown'
    };

    return mimeTypes[ext] || 'application/octet-stream';
  }

  isHealthy(): boolean {
    try {
      // Check if base directory is accessible
      fs.accessSync(this.basePath, fs.constants.R_OK);
      return true;
    } catch (error) {
      return false;
    }
  }

  getBasePath(): string {
    return this.basePath;
  }
}