/**
 * StaticFileDaemon - Handles all static file serving
 * 
 * Responsibilities:
 * - Serve CSS, JS, HTML, images, and other static files
 * - Compile TypeScript on-demand when .js is requested but only .ts exists
 * - Handle caching with ETags and Cache-Control headers
 * - Prevent path traversal attacks
 * - Set correct content-types
 */

import { BaseDaemon } from '../base/BaseDaemon';
import { DaemonMessage, DaemonResponse } from '../base/DaemonProtocol';
import { DaemonType } from '../base/DaemonTypes';
import * as fs from 'fs';
import * as path from 'path';
import * as crypto from 'crypto';
import * as ts from 'typescript';

export class StaticFileDaemon extends BaseDaemon {
  public readonly name = 'static-file';
  public readonly version = '1.0.0';
  public readonly daemonType = DaemonType.STATIC_FILE;
  
  private projectRoot: string;
  private etagCache = new Map<string, string>();

  constructor() {
    super();
    // Find project root by looking for package.json
    // In ES modules, use process.cwd() instead of __dirname
    this.projectRoot = this.findProjectRoot(process.cwd());
  }

  protected async onStart(): Promise<void> {
    this.log('📁 Static File Daemon started');
    this.log(`📁 Serving files from: ${this.projectRoot}`);
  }

  protected async onStop(): Promise<void> {
    this.etagCache.clear();
    this.log('📁 Static File Daemon stopped');
  }

  protected async handleMessage(message: DaemonMessage): Promise<DaemonResponse> {
    switch (message.type) {
      case 'serve_file':
        return this.serveFile(message.data as { pathname: string; headers?: Record<string, string> });
      
      case 'get_status':
        return {
          success: true,
          data: {
            projectRoot: this.projectRoot,
            cacheSize: this.etagCache.size
          }
        };
      
      default:
        return {
          success: false,
          error: `Unknown message type: ${message.type}`
        };
    }
  }

  private async serveFile(data: { pathname: string; headers?: Record<string, string> }): Promise<DaemonResponse> {
    try {
      const { pathname, headers = {} } = data;
      
      // Security: Clean and validate path
      const cleanPath = path.normalize(pathname).replace(/^(\.\.[/\\])+/, '');
      const fullPath = path.join(this.projectRoot, cleanPath);
      
      // Security: Ensure path is within project root
      if (!fullPath.startsWith(this.projectRoot)) {
        return {
          success: false,
          error: 'Access forbidden',
          data: { status: 403 }
        };
      }

      // Check if we need to compile TypeScript
      let actualPath = fullPath;
      
      if (fullPath.endsWith('.js') && !fs.existsSync(fullPath)) {
        const tsPath = fullPath.replace(/\.js$/, '.ts');
        if (fs.existsSync(tsPath)) {
          const compiled = await this.compileTypeScript(tsPath);
          if (!compiled.success) {
            return compiled;
          }
          
          return {
            success: true,
            data: {
              content: compiled.content,
              contentType: 'application/javascript',
              status: 200,
              headers: {
                'Content-Type': 'application/javascript',
                'X-Compiled-From': 'typescript',
                'Cache-Control': 'no-cache, no-store, must-revalidate'
              }
            }
          };
        }
      }

      // Check if file exists
      if (!fs.existsSync(actualPath)) {
        return {
          success: false,
          error: `File not found: ${pathname}`,
          data: { status: 404 }
        };
      }

      // Get file stats for ETag
      const stats = fs.statSync(actualPath);
      const etag = this.generateETag(actualPath, stats);
      
      // Check If-None-Match header
      if (headers['If-None-Match'] === etag) {
        return {
          success: true,
          data: {
            status: 304,
            headers: { ETag: etag }
          }
        };
      }

      // Read file
      const content = fs.readFileSync(actualPath);
      const contentType = this.getContentType(actualPath);
      
      return {
        success: true,
        data: {
          content: content.toString(),
          contentType,
          status: 200,
          headers: {
            'Content-Type': contentType,
            'Cache-Control': this.getCacheControl(actualPath),
            'ETag': etag,
            'X-Handled-By': this.name
          }
        }
      };
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log(`❌ Error serving file: ${errorMessage}`, 'error');
      
      return {
        success: false,
        error: errorMessage,
        data: { status: 500 }
      };
    }
  }

  private async compileTypeScript(tsPath: string): Promise<{ success: boolean; content?: string; error?: string; data?: { status: number } }> {
    try {
      const source = fs.readFileSync(tsPath, 'utf8');
      
      // First compile TypeScript to JavaScript
      const result = ts.transpileModule(source, {
        compilerOptions: {
          target: ts.ScriptTarget.ES2020,
          module: ts.ModuleKind.ES2020,
          moduleResolution: ts.ModuleResolutionKind.NodeJs,
          esModuleInterop: true,
          allowSyntheticDefaultImports: true,
          sourceMap: true,
          inlineSourceMap: true
        }
      });
      
      // Post-process the compiled JavaScript to fix imports
      let processedContent = this.addJsExtensionsToImports(result.outputText);
      
      this.log(`📦 Compiled TypeScript and fixed imports: ${tsPath}`);
      
      return {
        success: true,
        content: processedContent
      };
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        error: `TypeScript compilation failed: ${errorMessage}`,
        data: { status: 500 }
      };
    }
  }
  
  private addJsExtensionsToImports(jsCode: string): string {
    // Simpler approach: find all import/export statements and process them
    const importExportPattern = /((?:import|export)\s+[^;]+from\s*["'])([^"']+)(["'])/g;
    
    return jsCode.replace(importExportPattern, (match, prefix, path, suffix) => {
      // Only add .js to relative imports that don't already have extensions
      if (this.isRelativeImport(path) && !this.hasFileExtension(path)) {
        return prefix + path + '.js' + suffix;
      }
      return match;
    });
  }
  
  private isRelativeImport(importPath: string): boolean {
    return importPath.startsWith('./') || importPath.startsWith('../');
  }
  
  private hasFileExtension(importPath: string): boolean {
    return /\.(js|ts|css|json|html)$/.test(importPath);
  }

  private generateETag(filePath: string, stats: fs.Stats): string {
    const cached = this.etagCache.get(filePath);
    if (cached) return cached;
    
    const hash = crypto.createHash('md5');
    hash.update(filePath);
    hash.update(stats.mtime.toISOString());
    hash.update(stats.size.toString());
    
    const etag = `"${hash.digest('hex')}"`;
    this.etagCache.set(filePath, etag);
    
    return etag;
  }

  private getContentType(filePath: string): string {
    const ext = path.extname(filePath).toLowerCase();
    
    const contentTypes: Record<string, string> = {
      '.html': 'text/html',
      '.css': 'text/css',
      '.js': 'application/javascript',
      '.json': 'application/json',
      '.png': 'image/png',
      '.jpg': 'image/jpeg',
      '.jpeg': 'image/jpeg',
      '.gif': 'image/gif',
      '.svg': 'image/svg+xml',
      '.ico': 'image/x-icon',
      '.woff': 'font/woff',
      '.woff2': 'font/woff2',
      '.ttf': 'font/ttf',
      '.otf': 'font/otf',
      '.txt': 'text/plain',
      '.md': 'text/markdown',
      '.xml': 'application/xml',
      '.pdf': 'application/pdf',
      '.zip': 'application/zip'
    };
    
    return contentTypes[ext] || 'application/octet-stream';
  }

  private getCacheControl(filePath: string): string {
    const ext = path.extname(filePath).toLowerCase();
    
    // Development mode - no caching for frequently changing files
    if (['.js', '.css', '.html'].includes(ext)) {
      return 'no-cache, no-store, must-revalidate';
    }
    
    // Images and fonts can be cached
    if (['.png', '.jpg', '.jpeg', '.gif', '.svg', '.woff', '.woff2'].includes(ext)) {
      return 'public, max-age=3600';
    }
    
    return 'no-cache, no-store, must-revalidate';
  }

  private findProjectRoot(startPath: string): string {
    let currentPath = startPath;
    
    while (currentPath !== path.dirname(currentPath)) {
      if (fs.existsSync(path.join(currentPath, 'package.json'))) {
        return currentPath;
      }
      currentPath = path.dirname(currentPath);
    }
    
    // Fallback to cwd
    return process.cwd();
  }

  /**
   * Register with WebSocketDaemon to handle file routes
   */
  public registerWithWebSocketDaemon(wsDaemon: { registerRouteHandler: (pattern: string, daemonName: string, messageType: string) => void }): void {
    // Register handlers for various file patterns
    wsDaemon.registerRouteHandler('*.css', this.name, 'serve_file');
    wsDaemon.registerRouteHandler('*.js', this.name, 'serve_file');
    wsDaemon.registerRouteHandler('*.html', this.name, 'serve_file');
    wsDaemon.registerRouteHandler('*.json', this.name, 'serve_file');
    wsDaemon.registerRouteHandler('/src/*', this.name, 'serve_file');
    wsDaemon.registerRouteHandler('/dist/*', this.name, 'serve_file');
    wsDaemon.registerRouteHandler('/static/*', this.name, 'serve_file');
    
    this.log('📁 Registered file serving routes with WebSocketDaemon');
  }
}