/**
 * StaticFileServing Test - Fix 503 errors for /src/ui/continuum.js and /dist/ui/widget-loader.js
 * This test verifies that the RendererDaemon can serve static files directly instead of proxying
 */

import { promises as fs } from 'fs';
import { join, extname } from 'path';

export class StaticFileHandler {
  private readonly projectRoot: string;

  constructor(projectRoot: string = process.cwd()) {
    this.projectRoot = projectRoot;
  }

  /**
   * Test if static files exist and can be served
   */
  async testStaticFiles(): Promise<{success: boolean, files: Record<string, boolean>}> {
    const testFiles = [
      '/src/ui/continuum.js',
      '/dist/ui/widget-loader.js',
      '/src/ui/components/shared/BaseWidget.css'
    ];

    const results: Record<string, boolean> = {};

    for (const filePath of testFiles) {
      try {
        const fullPath = join(this.projectRoot, filePath.substring(1)); // Remove leading slash
        await fs.access(fullPath);
        const stats = await fs.stat(fullPath);
        results[filePath] = stats.isFile() && stats.size > 0;
        console.log(`✅ File exists: ${filePath} (${stats.size} bytes)`);
      } catch (error) {
        results[filePath] = false;
        console.log(`❌ File missing: ${filePath}`);
      }
    }

    const allFound = Object.values(results).every(found => found);
    return { success: allFound, files: results };
  }

  /**
   * Serve a static file directly (replacement for proxy approach)
   */
  async serveStaticFile(pathname: string, res: any): Promise<void> {
    try {
      // Remove query parameters and leading slash
      const cleanPath = pathname.split('?')[0].substring(1);
      const fullPath = join(this.projectRoot, cleanPath);

      // Security check - ensure path is within project
      if (!fullPath.startsWith(this.projectRoot)) {
        res.writeHead(403, { 'Content-Type': 'text/plain' });
        res.end('Forbidden');
        return;
      }

      // Check if file exists
      await fs.access(fullPath);
      const stats = await fs.stat(fullPath);

      if (!stats.isFile()) {
        res.writeHead(404, { 'Content-Type': 'text/plain' });
        res.end('Not Found');
        return;
      }

      // Determine content type
      const contentType = this.getContentType(extname(fullPath));

      // Read and serve file
      const content = await fs.readFile(fullPath);
      
      res.writeHead(200, {
        'Content-Type': contentType,
        'Content-Length': content.length,
        'Cache-Control': 'no-cache, no-store, must-revalidate',
        'Pragma': 'no-cache',
        'Expires': '0'
      });
      res.end(content);

      console.log(`✅ Served static file: ${pathname} (${content.length} bytes, ${contentType})`);

    } catch (error) {
      console.log(`❌ Failed to serve ${pathname}: ${error.message}`);
      res.writeHead(404, { 'Content-Type': 'text/plain' });
      res.end('Not Found');
    }
  }

  private getContentType(ext: string): string {
    const mimeTypes: Record<string, string> = {
      '.js': 'application/javascript',
      '.css': 'text/css',
      '.html': 'text/html',
      '.json': 'application/json',
      '.ts': 'application/typescript',
      '.map': 'application/json'
    };

    return mimeTypes[ext.toLowerCase()] || 'text/plain';
  }
}

// Export for use in RendererDaemon
export default StaticFileHandler;