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
      } else if (url.startsWith('/proxy-html2canvas')) {
        // html2canvas proxy - fetches images and returns as base64 data URL
        this.handleHtml2CanvasProxy(req, res, url).catch(error => {
          console.error('❌ html2canvas proxy failed:', error);
          if (!res.headersSent) {
            res.writeHead(500, { 'Content-Type': 'text/plain' });
            res.end('');
          }
        });
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
   * Read request body as string
   */
  private getRequestBody(req: http.IncomingMessage): Promise<string> {
    return new Promise((resolve, reject) => {
      const chunks: Buffer[] = [];
      req.on('data', chunk => chunks.push(chunk));
      req.on('end', () => resolve(Buffer.concat(chunks).toString()));
      req.on('error', reject);
    });
  }

  /**
   * Decode HTML entities in a string (&#x3D; → =, &amp; → &, etc.)
   */
  private decodeHtmlEntities(str: string): string {
    return str
      // Numeric hex entities: &#x3D; → =
      .replace(/&#x([0-9a-fA-F]+);/g, (_, hex) => String.fromCharCode(parseInt(hex, 16)))
      // Numeric decimal entities: &#61; → =
      .replace(/&#(\d+);/g, (_, dec) => String.fromCharCode(parseInt(dec, 10)))
      // Named entities
      .replace(/&amp;/g, '&')
      .replace(/&lt;/g, '<')
      .replace(/&gt;/g, '>')
      .replace(/&quot;/g, '"')
      .replace(/&apos;/g, "'");
  }

  /**
   * Handle proxy requests for co-browsing widget
   * Fetches external URLs and serves them from our origin (bypasses X-Frame-Options)
   */
  private async handleProxy(req: http.IncomingMessage, res: http.ServerResponse, url: string): Promise<void> {
    // Extract encoded URL from /proxy/{encodedUrl}
    const encodedUrl = url.substring(7); // Remove '/proxy/'
    // URL-decode first, then decode any HTML entities (like &#x3D; → =)
    const targetUrl = this.decodeHtmlEntities(decodeURIComponent(encodedUrl));

    const method = req.method || 'GET';
    console.log(`🌐 Proxy: ${method} ${targetUrl}`);

    // Handle CORS preflight
    if (method === 'OPTIONS') {
      res.writeHead(204, {
        'Access-Control-Allow-Origin': '*',
        'Access-Control-Allow-Methods': 'GET, POST, PUT, PATCH, DELETE, OPTIONS',
        'Access-Control-Allow-Headers': '*',
        'Access-Control-Max-Age': '86400'
      });
      res.end();
      return;
    }

    try {
      // Forward browser headers to look like the user's browser
      const browserHeaders: Record<string, string> = {
        'User-Agent': req.headers['user-agent'] || 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36',
        'Accept': req.headers['accept'] as string || 'text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8',
        'Accept-Language': req.headers['accept-language'] as string || 'en-US,en;q=0.9',
        'Accept-Encoding': 'identity', // Don't request compression, we need to rewrite content
        'Cache-Control': 'no-cache',
      };

      // Forward content-type for POST requests
      if (req.headers['content-type']) {
        browserHeaders['Content-Type'] = req.headers['content-type'] as string;
      }

      // Get request body for POST/PUT
      let body: string | undefined;
      if (method === 'POST' || method === 'PUT' || method === 'PATCH') {
        body = await this.getRequestBody(req);
      }

      const response = await fetch(targetUrl, {
        method,
        headers: browserHeaders,
        redirect: 'follow',
        body
      });

      if (!response.ok) {
        // Return same status code with appropriate content type
        // Determine expected content type from URL extension
        const ext = targetUrl.split('?')[0].split('.').pop()?.toLowerCase();
        const contentTypes: Record<string, string> = {
          'js': 'application/javascript',
          'css': 'text/css',
          'json': 'application/json',
          'png': 'image/png',
          'jpg': 'image/jpeg',
          'jpeg': 'image/jpeg',
          'gif': 'image/gif',
          'webp': 'image/webp',
          'mp4': 'video/mp4',
          'webm': 'video/webm',
          'svg': 'image/svg+xml',
        };
        const contentType = contentTypes[ext || ''] || 'text/plain';

        // For binary types, just return empty response
        if (['png', 'jpg', 'jpeg', 'gif', 'webp', 'mp4', 'webm'].includes(ext || '')) {
          res.writeHead(response.status, {
            'Content-Type': contentType,
            'Access-Control-Allow-Origin': '*',
          });
          res.end();
        } else {
          // For text types, return error comment in appropriate format
          let errorContent: string;
          if (contentType === 'application/javascript') {
            errorContent = `/* Proxy error: ${response.status} ${response.statusText} for ${targetUrl} */`;
          } else if (contentType === 'text/css') {
            errorContent = `/* Proxy error: ${response.status} ${response.statusText} for ${targetUrl} */`;
          } else if (contentType === 'application/json') {
            errorContent = JSON.stringify({ error: `${response.status} ${response.statusText}`, url: targetUrl });
          } else {
            errorContent = `Proxy error: ${response.status} ${response.statusText} for ${targetUrl}`;
          }
          res.writeHead(response.status, {
            'Content-Type': contentType,
            'Access-Control-Allow-Origin': '*',
          });
          res.end(errorContent);
        }
        return;
      }

      const contentType = response.headers.get('content-type') || 'text/html';

      // Handle binary content (images, fonts, etc.) separately
      const isBinary = contentType.startsWith('image/') ||
                       contentType.startsWith('audio/') ||
                       contentType.startsWith('video/') ||
                       contentType.startsWith('font/') ||
                       contentType.includes('octet-stream') ||
                       contentType.includes('application/pdf');

      if (isBinary) {
        const buffer = await response.arrayBuffer();
        res.writeHead(200, {
          'Content-Type': contentType,
          'Content-Length': buffer.byteLength,
          'Access-Control-Allow-Origin': '*',
          'Cache-Control': 'public, max-age=3600'
        });
        res.end(Buffer.from(buffer));
        console.log(`✅ Proxy: Served binary ${targetUrl} (${buffer.byteLength} bytes)`);
        return;
      }

      // Text content - can be rewritten
      let content = await response.text();

      // Rewrite URLs in HTML/CSS to go through our proxy
      if (contentType.includes('text/html')) {
        content = this.rewriteProxyUrls(content, targetUrl);
        content = this.injectProxyShim(content, targetUrl);
      } else if (contentType.includes('text/css')) {
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

      // Determine expected content type from URL extension
      const ext = targetUrl.split('?')[0].split('.').pop()?.toLowerCase();
      const contentTypes: Record<string, string> = {
        'js': 'application/javascript',
        'css': 'text/css',
        'json': 'application/json',
      };
      const contentType = contentTypes[ext || ''] || 'text/plain';
      const errorMsg = error instanceof Error ? error.message : 'Unknown error';

      let errorContent: string;
      if (contentType === 'application/javascript') {
        errorContent = `/* Proxy error: ${errorMsg} */`;
      } else if (contentType === 'text/css') {
        errorContent = `/* Proxy error: ${errorMsg} */`;
      } else if (contentType === 'application/json') {
        errorContent = JSON.stringify({ error: errorMsg, url: targetUrl });
      } else {
        errorContent = `Proxy error: ${errorMsg}`;
      }

      res.writeHead(502, {
        'Content-Type': contentType,
        'Access-Control-Allow-Origin': '*',
      });
      res.end(errorContent);
    }
  }

  /**
   * Handle html2canvas proxy requests for screenshots
   * html2canvas calls this with ?url=<encoded-url> and expects the actual image data back
   * NOT a data URL - the actual binary image with proper Content-Type
   */
  private async handleHtml2CanvasProxy(req: http.IncomingMessage, res: http.ServerResponse, url: string): Promise<void> {
    // Handle CORS preflight
    if (req.method === 'OPTIONS') {
      res.writeHead(204, {
        'Access-Control-Allow-Origin': '*',
        'Access-Control-Allow-Methods': 'GET, OPTIONS',
        'Access-Control-Allow-Headers': '*',
        'Access-Control-Max-Age': '86400'
      });
      res.end();
      return;
    }

    // Parse the URL to get the ?url= parameter
    const parsedUrl = new URL(url, 'http://localhost');
    const targetUrl = parsedUrl.searchParams.get('url');

    if (!targetUrl) {
      res.writeHead(400, { 'Content-Type': 'text/plain', 'Access-Control-Allow-Origin': '*' });
      res.end('Missing url parameter');
      return;
    }

    console.log(`📸 html2canvas proxy: Fetching ${targetUrl}`);

    try {
      const response = await fetch(targetUrl, {
        headers: {
          'User-Agent': 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36',
          'Accept': 'image/*,*/*;q=0.8',
          'Referer': new URL(targetUrl).origin + '/',
        },
      });

      if (!response.ok) {
        console.log(`❌ html2canvas proxy: ${response.status} for ${targetUrl}`);
        res.writeHead(response.status, { 'Access-Control-Allow-Origin': '*' });
        res.end();
        return;
      }

      // Get the image data as a buffer
      const buffer = await response.arrayBuffer();
      const contentType = response.headers.get('content-type') || 'image/png';

      // Return actual binary image data with proper Content-Type
      // html2canvas needs the image to be same-origin, so we serve it from our domain
      res.writeHead(200, {
        'Content-Type': contentType,
        'Content-Length': buffer.byteLength,
        'Access-Control-Allow-Origin': '*',
        'Cache-Control': 'public, max-age=3600',
      });
      res.end(Buffer.from(buffer));

      console.log(`✅ html2canvas proxy: Served ${targetUrl} (${buffer.byteLength} bytes)`);

    } catch (error: any) {
      console.error('❌ html2canvas proxy failed:', error.message);
      res.writeHead(500, { 'Access-Control-Allow-Origin': '*' });
      res.end();
    }
  }

  /**
   * Inject JavaScript shim that intercepts fetch/XHR to route through proxy
   * This fixes CORS issues for dynamic requests made by page JavaScript
   */
  private injectProxyShim(content: string, baseUrl: string): string {
    const base = new URL(baseUrl);

    const shimScript = `
<script>
(function() {
  // Proxy shim: intercept all network requests and route through our proxy
  const PROXY_PREFIX = '/proxy/';
  const BASE_ORIGIN = '${base.origin}';
  var isFrozen = false;  // When true, stop all proxy requests (server disconnected)

  // Listen for freeze command from parent (server disconnected)
  window.addEventListener('message', function(event) {
    if (event.source === window.parent && event.data && event.data.type === 'jtag-shim-freeze') {
      console.log('[Proxy Shim] FREEZE - stopping all proxy requests');
      isFrozen = true;
    }
  });

  // Decode HTML entities (&#x3D; → =, &amp; → &, etc.)
  function decodeHtmlEntities(str) {
    if (!str) return str;
    return str
      .replace(/&#x([0-9a-fA-F]+);/g, function(_, hex) { return String.fromCharCode(parseInt(hex, 16)); })
      .replace(/&#(\\d+);/g, function(_, dec) { return String.fromCharCode(parseInt(dec, 10)); })
      .replace(/&amp;/g, '&')
      .replace(/&lt;/g, '<')
      .replace(/&gt;/g, '>')
      .replace(/&quot;/g, '"')
      .replace(/&apos;/g, "'");
  }

  function proxyUrl(url) {
    // If frozen, return original URL (let it fail naturally, no proxy flood)
    if (isFrozen) return url;
    if (!url || typeof url !== 'string') return url;
    if (url.startsWith(PROXY_PREFIX)) return url;
    if (url.startsWith('data:') || url.startsWith('blob:') || url.startsWith('javascript:')) return url;

    // Decode HTML entities first
    url = decodeHtmlEntities(url);
    // Decode URL-encoded chars to avoid double-encoding (%3D → %253D)
    try { url = decodeURIComponent(url); } catch(e) {}

    let absoluteUrl;
    if (url.startsWith('http://') || url.startsWith('https://')) {
      absoluteUrl = url;
    } else if (url.startsWith('//')) {
      absoluteUrl = 'https:' + url;
    } else if (url.startsWith('/')) {
      absoluteUrl = BASE_ORIGIN + url;
    } else {
      absoluteUrl = new URL(url, BASE_ORIGIN + '/').href;
    }
    return PROXY_PREFIX + encodeURIComponent(absoluteUrl);
  }

  // Override fetch
  const originalFetch = window.fetch;
  window.fetch = function(input, init) {
    let url = typeof input === 'string' ? input : (input instanceof Request ? input.url : String(input));
    const proxiedUrl = proxyUrl(url);

    if (typeof input === 'string') {
      return originalFetch.call(this, proxiedUrl, init);
    } else if (input instanceof Request) {
      return originalFetch.call(this, new Request(proxiedUrl, input), init);
    }
    return originalFetch.call(this, proxiedUrl, init);
  };

  // Override XMLHttpRequest.open
  const originalXHROpen = XMLHttpRequest.prototype.open;
  XMLHttpRequest.prototype.open = function(method, url, async, user, password) {
    const proxiedUrl = proxyUrl(url);
    return originalXHROpen.call(this, method, proxiedUrl, async !== false, user, password);
  };

  // Override Image src
  const originalImageDescriptor = Object.getOwnPropertyDescriptor(HTMLImageElement.prototype, 'src');
  if (originalImageDescriptor && originalImageDescriptor.set) {
    Object.defineProperty(HTMLImageElement.prototype, 'src', {
      set: function(value) {
        originalImageDescriptor.set.call(this, proxyUrl(value));
      },
      get: originalImageDescriptor.get
    });
  }

  console.log('[Proxy Shim] Installed - all requests routed through proxy');
})();
</script>`;

    // JTAG Shim: full interface command support for proxied pages
    const jtagShimScript = `
<script>
(function() {
  var JTAG_SHIM_VERSION = '1.1.0';
  var HTML2CANVAS_CDN = 'https://cdnjs.cloudflare.com/ajax/libs/html2canvas/1.4.1/html2canvas.min.js';
  var html2canvasLoaded = false;
  var html2canvasLoading = false;

  // ═══════════════════════════════════════════════════════════════════════════
  // UTILITIES
  // ═══════════════════════════════════════════════════════════════════════════

  function loadHtml2Canvas() {
    return new Promise(function(resolve, reject) {
      if (html2canvasLoaded) { resolve(); return; }
      if (html2canvasLoading) {
        var check = setInterval(function() {
          if (html2canvasLoaded) { clearInterval(check); resolve(); }
        }, 100);
        return;
      }
      html2canvasLoading = true;
      var script = document.createElement('script');
      script.src = HTML2CANVAS_CDN;
      script.onload = function() {
        html2canvasLoaded = true;
        html2canvasLoading = false;
        console.log('[JTAG Shim] html2canvas loaded');
        resolve();
      };
      script.onerror = function() {
        html2canvasLoading = false;
        reject(new Error('Failed to load html2canvas'));
      };
      document.head.appendChild(script);
    });
  }

  function getElement(selector) {
    if (!selector) return null;
    return document.querySelector(selector);
  }

  function success(data) {
    return { success: true, data: data };
  }

  function fail(message) {
    return { success: false, error: { message: message } };
  }

  // ═══════════════════════════════════════════════════════════════════════════
  // COMMANDS
  // ═══════════════════════════════════════════════════════════════════════════

  var commands = {
    ping: function() {
      return Promise.resolve(success({ version: JTAG_SHIM_VERSION, url: window.location.href }));
    },

    pageInfo: function() {
      return Promise.resolve(success({
        url: window.location.href,
        title: document.title,
        scrollX: window.scrollX,
        scrollY: window.scrollY,
        scrollWidth: document.documentElement.scrollWidth,
        scrollHeight: document.documentElement.scrollHeight,
        viewportWidth: window.innerWidth,
        viewportHeight: window.innerHeight
      }));
    },

    screenshot: function(params) {
      params = params || {};

      // Pre-convert images to data URLs to ensure they render in canvas
      function convertImagesToDataUrls() {
        return new Promise(function(resolve) {
          var images = document.querySelectorAll('img');
          var pending = 0;
          var converted = 0;

          if (images.length === 0) {
            resolve();
            return;
          }

          images.forEach(function(img) {
            // Skip images that are already data URLs or not loaded
            if (!img.src || img.src.startsWith('data:') || !img.complete || img.naturalWidth === 0) {
              return;
            }

            pending++;

            // Create canvas to convert image
            var canvas = document.createElement('canvas');
            canvas.width = img.naturalWidth;
            canvas.height = img.naturalHeight;
            var ctx = canvas.getContext('2d');

            try {
              ctx.drawImage(img, 0, 0);
              var dataUrl = canvas.toDataURL('image/png');
              img.src = dataUrl;
              converted++;
            } catch (e) {
              // Cross-origin image, can't convert - leave as-is
              console.log('[JTAG Shim] Could not convert image:', img.src.substring(0, 50));
            }

            pending--;
            if (pending === 0) {
              console.log('[JTAG Shim] Converted ' + converted + ' images to data URLs');
              resolve();
            }
          });

          // If no images needed processing, resolve immediately
          if (pending === 0) {
            resolve();
          }
        });
      }

      return convertImagesToDataUrls().then(function() {
        return loadHtml2Canvas();
      }).then(function() {
        var h2c = window.html2canvas;
        if (!h2c) throw new Error('html2canvas not available');
        var target = params.selector ? getElement(params.selector) : document.body;
        if (!target) throw new Error('Element not found: ' + params.selector);
        var scale = params.scale || 1;
        var dpr = window.devicePixelRatio || 1;

        var opts = {
          scale: scale,
          useCORS: true,
          allowTaint: false,
          logging: false,
          backgroundColor: params.backgroundColor || null,
          foreignObjectRendering: false,
          imageTimeout: 15000,
          removeContainer: true,
          proxy: '/proxy-html2canvas',
          scrollX: -window.scrollX,
          scrollY: -window.scrollY
        };
        // viewportOnly: capture only visible area by setting window dimensions
        if (params.viewportOnly) {
          opts.windowWidth = window.innerWidth;
          opts.windowHeight = window.innerHeight;
          opts.width = window.innerWidth;
          opts.height = window.innerHeight;
        }
        return h2c(target, opts);
      }).then(function(canvas) {
        var format = params.format || 'png';
        var quality = params.quality || 0.9;

        // For viewport capture, crop to actual viewport size
        var finalCanvas = canvas;
        if (params.viewportOnly && (canvas.width > window.innerWidth || canvas.height > window.innerHeight)) {
          var cropCanvas = document.createElement('canvas');
          cropCanvas.width = window.innerWidth;
          cropCanvas.height = window.innerHeight;
          var ctx = cropCanvas.getContext('2d');
          ctx.drawImage(canvas, 0, 0);
          finalCanvas = cropCanvas;
        }

        var dataUrl = format === 'png' ? finalCanvas.toDataURL('image/png') : finalCanvas.toDataURL('image/' + format, quality);
        return success({
          dataUrl: dataUrl,
          metadata: { width: canvas.width, height: canvas.height, format: format, quality: quality, selector: params.selector || 'body', viewportOnly: !!params.viewportOnly, captureTime: Date.now() }
        });
      }).catch(function(e) { return fail(e.message); });
    },

    click: function(params) {
      var el = getElement(params.selector);
      if (!el) return Promise.resolve(fail('Element not found: ' + params.selector));
      var count = params.clickCount || 1;
      for (var i = 0; i < count; i++) { el.click(); }
      return Promise.resolve(success({ clicked: true, elementTag: el.tagName.toLowerCase() }));
    },

    type: function(params) {
      var el = getElement(params.selector);
      if (!el) return Promise.resolve(fail('Element not found: ' + params.selector));
      if (params.clear) { el.value = ''; }
      el.focus();
      el.value = (el.value || '') + params.text;
      el.dispatchEvent(new Event('input', { bubbles: true }));
      el.dispatchEvent(new Event('change', { bubbles: true }));
      return Promise.resolve(success({ typed: true, finalValue: el.value }));
    },

    scroll: function(params) {
      if (params.selector) {
        var el = getElement(params.selector);
        if (!el) return Promise.resolve(fail('Element not found: ' + params.selector));
        el.scrollIntoView({ behavior: params.behavior || 'auto', block: 'center' });
      } else if (params.x !== undefined || params.y !== undefined) {
        window.scrollTo({ left: params.x || 0, top: params.y || 0, behavior: params.behavior || 'auto' });
      }
      return Promise.resolve(success({ scrolled: true, scrollX: window.scrollX, scrollY: window.scrollY }));
    },

    query: function(params) {
      var el = getElement(params.selector);
      if (!el) return Promise.resolve(success({ found: false }));
      var result = { found: true, tag: el.tagName.toLowerCase(), id: el.id, className: el.className };
      if (params.includeText) result.text = el.textContent ? el.textContent.substring(0, 1000) : '';
      if (params.includeHtml) result.html = el.outerHTML ? el.outerHTML.substring(0, 2000) : '';
      if (params.includeBounds) result.bounds = el.getBoundingClientRect();
      if (params.attributes) {
        result.attributes = {};
        params.attributes.forEach(function(attr) { result.attributes[attr] = el.getAttribute(attr); });
      }
      return Promise.resolve(success(result));
    },

    queryAll: function(params) {
      var els = document.querySelectorAll(params.selector);
      var limit = params.limit || 100;
      var results = [];
      for (var i = 0; i < Math.min(els.length, limit); i++) {
        var el = els[i];
        var item = { tag: el.tagName.toLowerCase(), id: el.id, className: el.className };
        if (params.includeText) item.text = el.textContent ? el.textContent.substring(0, 500) : '';
        if (params.includeBounds) item.bounds = el.getBoundingClientRect();
        results.push(item);
      }
      return Promise.resolve(success({ count: els.length, elements: results }));
    },

    getValue: function(params) {
      var el = getElement(params.selector);
      if (!el) return Promise.resolve(fail('Element not found: ' + params.selector));
      return Promise.resolve(success({ value: el.value || el.textContent || '', type: el.type || el.tagName.toLowerCase() }));
    },

    setValue: function(params) {
      var el = getElement(params.selector);
      if (!el) return Promise.resolve(fail('Element not found: ' + params.selector));
      var prev = el.value;
      el.value = params.value;
      el.dispatchEvent(new Event('input', { bubbles: true }));
      el.dispatchEvent(new Event('change', { bubbles: true }));
      return Promise.resolve(success({ set: true, previousValue: prev }));
    },

    focus: function(params) {
      var el = getElement(params.selector);
      if (!el) return Promise.resolve(fail('Element not found: ' + params.selector));
      el.focus();
      return Promise.resolve(success({}));
    },

    blur: function(params) {
      var el = getElement(params.selector);
      if (!el) return Promise.resolve(fail('Element not found: ' + params.selector));
      el.blur();
      return Promise.resolve(success({}));
    },

    hover: function(params) {
      var el = getElement(params.selector);
      if (!el) return Promise.resolve(fail('Element not found: ' + params.selector));
      el.dispatchEvent(new MouseEvent('mouseenter', { bubbles: true }));
      el.dispatchEvent(new MouseEvent('mouseover', { bubbles: true }));
      return Promise.resolve(success({}));
    },

    waitFor: function(params) {
      var timeout = params.timeout || 10000;
      var start = Date.now();
      return new Promise(function(resolve) {
        function check() {
          var el = getElement(params.selector);
          if (el && (!params.visible || el.offsetParent !== null)) {
            resolve(success({ found: true, tag: el.tagName.toLowerCase() }));
          } else if (Date.now() - start > timeout) {
            resolve(fail('Timeout waiting for: ' + params.selector));
          } else {
            setTimeout(check, 100);
          }
        }
        check();
      });
    },

    evaluate: function(params) {
      try {
        var fn = new Function('args', params.script);
        var result = fn(params.args || []);
        return Promise.resolve(success({ returnValue: result }));
      } catch (e) {
        return Promise.resolve(fail('Evaluate error: ' + e.message));
      }
    }
  };

  // ═══════════════════════════════════════════════════════════════════════════
  // MESSAGE HANDLER
  // ═══════════════════════════════════════════════════════════════════════════

  function handleMessage(event) {
    if (event.source !== window.parent) return;
    var data = event.data || {};
    if (data.type !== 'jtag-shim-request') return;

    var command = data.command;
    var params = data.params || {};
    var requestId = data.requestId;

    console.log('[JTAG Shim] Command:', command);

    var handler = commands[command];
    var resultPromise = handler ? handler(params) : Promise.resolve(fail('Unknown command: ' + command));

    resultPromise.then(function(result) {
      window.parent.postMessage({ type: 'jtag-shim-response', requestId: requestId, result: result }, '*');
    });
  }

  window.addEventListener('message', handleMessage);
  window.parent.postMessage({ type: 'jtag-shim-ready', version: JTAG_SHIM_VERSION, url: window.location.href }, '*');

  // Notify parent of URL changes (for URL bar sync)
  var lastUrl = window.location.href;
  function checkUrlChange() {
    if (window.location.href !== lastUrl) {
      lastUrl = window.location.href;
      var title = document.title || '';
      console.log('[JTAG Shim] URL changed:', lastUrl);
      window.parent.postMessage({
        type: 'jtag-shim-url-change',
        url: lastUrl,
        title: title
      }, '*');
    }
  }

  // Check for URL changes periodically (handles SPA navigation)
  setInterval(checkUrlChange, 500);

  // Also notify on popstate (back/forward navigation)
  window.addEventListener('popstate', function() {
    setTimeout(checkUrlChange, 50);
  });

  // Notify on link clicks that might cause navigation
  document.addEventListener('click', function(e) {
    var target = e.target;
    while (target && target.tagName !== 'A') {
      target = target.parentElement;
    }
    if (target && target.href) {
      // Slight delay to allow navigation to complete
      setTimeout(checkUrlChange, 100);
    }
  }, true);

  console.log('[JTAG Shim] Initialized v' + JTAG_SHIM_VERSION + ' with ' + Object.keys(commands).length + ' commands');
})();
</script>`;

    const allShims = shimScript + jtagShimScript;

    // Inject at start of <head> (after <head> tag)
    if (content.includes('<head>')) {
      content = content.replace('<head>', '<head>' + allShims);
    } else if (content.includes('<html>')) {
      content = content.replace('<html>', '<html><head>' + allShims + '</head>');
    } else {
      // No head/html tag, prepend
      content = allShims + content;
    }

    return content;
  }

  /**
   * Rewrite URLs in content to go through proxy
   */
  private rewriteProxyUrls(content: string, baseUrl: string): string {
    try {
      const base = new URL(baseUrl);

      // Helper to clean and encode a URL for proxy
      const proxyEncodeUrl = (url: string): string => {
        // Decode HTML entities first (&#x3D; → =, &amp; → &)
        let decoded = this.decodeHtmlEntities(url);
        // Also decode any URL-encoded chars to avoid double-encoding (%3D → %253D)
        try {
          decoded = decodeURIComponent(decoded);
        } catch {
          // Ignore decode errors (malformed URLs)
        }
        return encodeURIComponent(decoded);
      };

      // Rewrite absolute URLs (http:// and https://)
      content = content.replace(
        /(href|src|action|poster)=(["'])(https?:\/\/[^"']+)(["'])/gi,
        (match, attr, q1, url, q2) => {
          if (url.startsWith('/proxy/')) return match;
          return `${attr}=${q1}/proxy/${proxyEncodeUrl(url)}${q2}`;
        }
      );

      // Add crossorigin="anonymous" to img tags for canvas access
      content = content.replace(
        /<img([^>]*)\ssrc=/gi,
        '<img$1 crossorigin="anonymous" src='
      );

      // Rewrite protocol-relative URLs (//example.com)
      content = content.replace(
        /(href|src|action|poster)=(["'])(\/\/[^"']+)(["'])/gi,
        (match, attr, q1, url, q2) => {
          let fullUrl = 'https:' + this.decodeHtmlEntities(url);
          try { fullUrl = decodeURIComponent(fullUrl); } catch {}
          return `${attr}=${q1}/proxy/${encodeURIComponent(fullUrl)}${q2}`;
        }
      );

      // Rewrite srcset attribute (contains multiple URLs with descriptors)
      content = content.replace(
        /srcset=(["'])([^"']+)(["'])/gi,
        (match, q1, srcsetValue, q2) => {
          const rewrittenSrcset = srcsetValue.split(',').map((entry: string) => {
            const parts = entry.trim().split(/\s+/);
            let url = parts[0];
            const descriptor = parts.slice(1).join(' ');

            if (url.startsWith('/proxy/') || url.startsWith('data:')) {
              return entry;
            }

            // Handle different URL formats
            if (url.startsWith('//')) {
              url = 'https:' + url;
            } else if (url.startsWith('/')) {
              url = new URL(url, base).href;
            } else if (!url.startsWith('http')) {
              url = new URL(url, baseUrl).href;
            }

            return `/proxy/${encodeURIComponent(url)}${descriptor ? ' ' + descriptor : ''}`;
          }).join(', ');
          return `srcset=${q1}${rewrittenSrcset}${q2}`;
        }
      );

      // Rewrite root-relative URLs (/path)
      content = content.replace(
        /(href|src|action)=(["'])(\/[^/"'][^"']*)(["'])/gi,
        (match, attr, q1, path, q2) => {
          if (path.startsWith('/proxy/')) return match;
          let decodedPath = this.decodeHtmlEntities(path);
          try { decodedPath = decodeURIComponent(decodedPath); } catch {}
          const fullUrl = new URL(decodedPath, base).href;
          return `${attr}=${q1}/proxy/${encodeURIComponent(fullUrl)}${q2}`;
        }
      );

      // Rewrite CSS url() references
      content = content.replace(
        /url\((["']?)([^)"']+)(["']?)\)/gi,
        (match, q1, url, q2) => {
          if (url.startsWith('data:') || url.startsWith('/proxy/')) return match;
          let decodedUrl = this.decodeHtmlEntities(url);
          try { decodedUrl = decodeURIComponent(decodedUrl); } catch {}
          let fullUrl: string;
          if (decodedUrl.startsWith('http')) {
            fullUrl = decodedUrl;
          } else if (decodedUrl.startsWith('//')) {
            fullUrl = 'https:' + decodedUrl;
          } else if (decodedUrl.startsWith('/')) {
            fullUrl = new URL(decodedUrl, base).href;
          } else {
            fullUrl = new URL(decodedUrl, baseUrl).href;
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