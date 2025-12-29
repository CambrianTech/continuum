#!/usr/bin/env npx tsx
/**
 * Shared Minimal Server Template for Examples
 * Simple HTTP server that serves static files for JTAG examples
 */

import * as http from 'http';
import * as path from 'path';
import * as fs from 'fs';
import { createConnectionConfigAuto } from '../examples/shared/ConnectionConfigFactory';
import type { ConnectionConfig } from '@continuum/jtag/types';

// Create connection config ONCE - does all the reading
const connectionConfig: ConnectionConfig = createConnectionConfigAuto();
const PORT = connectionConfig.httpPort;

class MinimalServer {
  private server: http.Server;
  private requestInProgress = false;

  constructor() {
    this.server = http.createServer(this.handleRequest.bind(this));
  }

  private handleRequest(req: http.IncomingMessage, res: http.ServerResponse): void {
    if (this.requestInProgress) {
      res.writeHead(503, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ error: 'Server busy' }));
      return;
    }
    
    this.requestInProgress = true;
    
    try {
      const url = req.url || '/';
      const method = req.method || 'GET';
      
      console.log(`📥 ${method} ${url}`);

      if (url === '/') {
        this.serveUniversalDemo(res);
      } else if (url === '/config') {
        this.serveConfiguration(res).catch(error => {
          console.error('❌ Configuration request failed:', error);
          if (!res.headersSent) {
            res.writeHead(500, { 'Content-Type': 'application/json' });
            res.end(JSON.stringify({ error: 'Configuration error' }));
          }
        });
      } else if (url === '/demo.css') {
        this.serveFile(res, 'public/demo.css', 'text/css');
      } else if (url.startsWith('/dist/')) {
        const filePath = 'dist' + url.substring(5);
        const contentType = url.endsWith('.js') ? 'application/javascript' : 
                           url.endsWith('.css') ? 'text/css' :
                           url.endsWith('.json') ? 'application/json' : 'application/octet-stream';
        this.serveFile(res, filePath, contentType);
      } else if (url === '/favicon.ico') {
        this.serve404(res);
      } else if (url.startsWith('/proxy/')) {
        // Web proxy for co-browsing widget
        this.handleProxy(req, res, url).catch(error => {
          console.error('❌ Proxy request failed:', error);
          if (!res.headersSent) {
            res.writeHead(500, { 'Content-Type': 'text/plain' });
            res.end('Proxy error: ' + error.message);
          }
        });
      } else {
        // SPA fallback: serve the main HTML for client-side routing
        // This allows routes like /settings, /help, /chat/academy to work
        this.serveUniversalDemo(res);
      }
    } catch (error) {
      console.error('🚨 Request handling failed:', error);
      try {
        if (!res.headersSent) {
          res.writeHead(500, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ error: 'Internal server error' }));
        }
      } catch (responseError) {
        console.error('Could not send error response:', responseError);
      }
    } finally {
      this.requestInProgress = false;
    }
  }

  private serveFile(res: http.ServerResponse, filename: string, contentType: string): void {
    try {
      // Use current working directory (the example directory) instead of __dirname (scripts directory)
      const filePath = path.join(process.cwd(), filename);
      
      if (!fs.existsSync(filePath)) {
        console.error(`❌ File not found: ${filePath}`);
        this.serve404(res);
        return;
      }
      
      const content = fs.readFileSync(filePath, 'utf8');
      
      if (!res.headersSent) {
        res.writeHead(200, { 
          'Content-Type': contentType,
          'Cache-Control': 'no-cache, no-store, must-revalidate',
          'Content-Length': Buffer.byteLength(content, 'utf8')
        });
      }
      
      res.end(content);
      
    } catch (error) {
      console.error(`❌ Failed to serve ${filename}:`, error);
      this.serve404(res);
    }
  }

  /**
   * Serve universal demo HTML that adapts to any example configuration
   */
  private serveUniversalDemo(res: http.ServerResponse): void {
    try {
      const templatePath = path.join(__dirname, '../../templates/universal-demo.html');
      
      if (fs.existsSync(templatePath)) {
        const content = fs.readFileSync(templatePath, 'utf8');
        res.writeHead(200, { 
          'Content-Type': 'text/html',
          'Cache-Control': 'no-cache, no-store, must-revalidate'
        });
        res.end(content);
      } else {
        // Fallback to local demo.html or index.html
        if (fs.existsSync('public/demo.html')) {
          this.serveFile(res, 'public/demo.html', 'text/html');
        } else if (fs.existsSync('index.html')) {
          this.serveFile(res, 'index.html', 'text/html');
        } else {
          this.serve404(res);
        }
      }
    } catch (error) {
      console.error('❌ Failed to serve universal demo:', error.message);
      this.serve404(res);
    }
  }

  /**
   * Serve dynamic configuration based on examples.json
   */
  private async serveConfiguration(res: http.ServerResponse): Promise<void> {
    try {
      // Serve the pre-created connectionConfig - no config reading here
      const config = {
        activeExample: connectionConfig.exampleName,
        websocketPort: connectionConfig.websocketPort,
        httpPort: connectionConfig.httpPort,
        exampleConfig: {
          features: {
            screenshot_testing: connectionConfig.exampleName === 'test-bench',
            widget_testing: connectionConfig.exampleName === 'widget-ui', 
            browser_automation: connectionConfig.exampleName === 'test-bench'
          }
        }
      };
      
      res.writeHead(200, { 
        'Content-Type': 'application/json',
        'Cache-Control': 'no-cache, no-store, must-revalidate'
      });
      res.end(JSON.stringify(config, null, 2));
      
    } catch (error) {
      console.error('❌ Failed to serve configuration:', error.message);
      // Return fallback configuration
      const fallbackConfig = {
        activeExample: 'unknown',
        websocketPort: 9001,
        httpPort: PORT,
        exampleConfig: {
          features: {
            screenshot_testing: true,
            widget_testing: true,
            browser_automation: true
          }
        }
      };
      
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify(fallbackConfig, null, 2));
    }
  }

  private serve404(res: http.ServerResponse): void {
    res.writeHead(404, { 'Content-Type': 'text/html' });
    res.end('<h1>404 - Not Found</h1>');
  }

  /**
   * Handle proxy requests for co-browsing widget
   * Fetches external URLs and serves them from our origin (bypasses X-Frame-Options)
   */
  private async handleProxy(req: http.IncomingMessage, res: http.ServerResponse, url: string): Promise<void> {
    // Extract encoded URL from /proxy/{encodedUrl}
    const encodedUrl = url.substring(7); // Remove '/proxy/'
    const targetUrl = decodeURIComponent(encodedUrl);

    console.log(`🌐 Proxy: Fetching ${targetUrl}`);

    try {
      // Forward browser headers to look like the user's browser
      const browserHeaders: Record<string, string> = {
        'User-Agent': req.headers['user-agent'] || 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36',
        'Accept': req.headers['accept'] as string || 'text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8',
        'Accept-Language': req.headers['accept-language'] as string || 'en-US,en;q=0.9',
        'Accept-Encoding': 'identity', // Don't request compression, we need to rewrite content
        'Cache-Control': 'no-cache',
      };

      const response = await fetch(targetUrl, {
        headers: browserHeaders,
        redirect: 'follow'
      });

      if (!response.ok) {
        res.writeHead(response.status, { 'Content-Type': 'text/plain' });
        res.end(`Failed to fetch: ${response.status} ${response.statusText}`);
        return;
      }

      const contentType = response.headers.get('content-type') || 'text/html';
      let content = await response.text();

      // Rewrite URLs in HTML/CSS to go through our proxy
      if (contentType.includes('text/html') || contentType.includes('text/css')) {
        content = this.rewriteProxyUrls(content, targetUrl);
      }

      // Serve with CORS headers and WITHOUT blocking headers
      res.writeHead(200, {
        'Content-Type': contentType,
        'Access-Control-Allow-Origin': '*',
        'Access-Control-Allow-Methods': 'GET, POST, OPTIONS',
        'Access-Control-Allow-Headers': '*',
        // Explicitly NOT setting X-Frame-Options or CSP
        'Cache-Control': 'no-cache'
      });

      res.end(content);
      console.log(`✅ Proxy: Served ${targetUrl} (${content.length} bytes)`);

    } catch (error) {
      console.error(`❌ Proxy: Failed to fetch ${targetUrl}:`, error);
      res.writeHead(502, { 'Content-Type': 'text/plain' });
      res.end('Proxy error: ' + (error instanceof Error ? error.message : 'Unknown error'));
    }
  }

  /**
   * Rewrite URLs in content to go through proxy
   */
  private rewriteProxyUrls(content: string, baseUrl: string): string {
    try {
      const base = new URL(baseUrl);

      // Rewrite absolute URLs (http:// and https://)
      content = content.replace(
        /(href|src|action)=(["'])(https?:\/\/[^"']+)(["'])/gi,
        (match, attr, q1, url, q2) => {
          if (url.startsWith('/proxy/')) return match;
          return `${attr}=${q1}/proxy/${encodeURIComponent(url)}${q2}`;
        }
      );

      // Rewrite protocol-relative URLs (//example.com)
      content = content.replace(
        /(href|src|action)=(["'])(\/\/[^"']+)(["'])/gi,
        (match, attr, q1, url, q2) => {
          const fullUrl = 'https:' + url;
          return `${attr}=${q1}/proxy/${encodeURIComponent(fullUrl)}${q2}`;
        }
      );

      // Rewrite root-relative URLs (/path)
      content = content.replace(
        /(href|src|action)=(["'])(\/[^/"'][^"']*)(["'])/gi,
        (match, attr, q1, path, q2) => {
          if (path.startsWith('/proxy/')) return match;
          const fullUrl = new URL(path, base).href;
          return `${attr}=${q1}/proxy/${encodeURIComponent(fullUrl)}${q2}`;
        }
      );

      // Rewrite CSS url() references
      content = content.replace(
        /url\((["']?)([^)"']+)(["']?)\)/gi,
        (match, q1, url, q2) => {
          if (url.startsWith('data:') || url.startsWith('/proxy/')) return match;
          let fullUrl: string;
          if (url.startsWith('http')) {
            fullUrl = url;
          } else if (url.startsWith('//')) {
            fullUrl = 'https:' + url;
          } else if (url.startsWith('/')) {
            fullUrl = new URL(url, base).href;
          } else {
            fullUrl = new URL(url, baseUrl).href;
          }
          return `url(${q1}/proxy/${encodeURIComponent(fullUrl)}${q2})`;
        }
      );

      // Inject base tag for relative URLs that we might miss
      if (content.includes('<head>')) {
        const baseTag = `<base href="/proxy/${encodeURIComponent(base.origin + '/')}">`;
        content = content.replace('<head>', `<head>\n${baseTag}`);
      }

      return content;
    } catch (error) {
      console.warn('⚠️ Proxy: URL rewriting failed:', error);
      return content;
    }
  }

  async start(): Promise<void> {
    const exampleName = path.basename(process.cwd());
    console.log(`🚀 Starting ${exampleName} HTTP server...`);
    console.log('🌐 Browser client will connect to JTAG system via WebSocket');
    
    return new Promise((resolve, reject) => {
      this.server.on('error', reject);
      this.server.listen(PORT, () => {
        console.log(`✅ HTTP server running at http://localhost:${PORT}`);
        
        // Browser launch handled by main JTAG system - no duplicate launch needed
        console.log(`   🌐 Access at: http://localhost:${PORT} (browser auto-opened by JTAG system)`);
        
        resolve();
      });
    });
  }
}

// Start server
const server = new MinimalServer();
server.start().catch((error) => {
  console.error('🚨 Server startup failed:', error);
  process.exit(1);
});

// Global error handlers
process.on('uncaughtException', (error) => {
  console.error('🚨 Uncaught Exception:', error.message);
  process.exit(1);
});

process.on('unhandledRejection', (reason) => {
  console.error('🚨 Unhandled Rejection:', reason);
  process.exit(1);
});