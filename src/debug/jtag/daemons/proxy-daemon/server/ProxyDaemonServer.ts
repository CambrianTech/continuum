/**
 * Proxy Daemon - Server Implementation
 * 
 * Server-side HTTP proxy that enables cross-origin access for widgets.
 * Handles URL rewriting, header forwarding, and content processing.
 */

import { ProxyDaemon, type ProxyRequest, type ProxyResponse } from '@daemonsProxyDaemon/shared/ProxyDaemon';
import type { JTAGContext } from '@shared/JTAGTypes';
import type { JTAGRouter } from '@shared/JTAGRouter';

export class ProxyDaemonServer extends ProxyDaemon {
  private userAgent = 'Continuum-ProxyDaemon/1.0 (Training Bot)';

  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);
  }

  /**
   * Initialize server proxy daemon
   */
  protected async initialize(): Promise<void> {
    console.log('✅ ProxyDaemonServer: Initialized HTTP proxy');
  }

  /**
   * Execute HTTP proxy request using JTAG router's HTTP transport
   */
  protected async executeProxy(request: ProxyRequest): Promise<ProxyResponse> {
    try {
      const startTime = Date.now();
      
      console.log(`🌐 ProxyDaemonServer: Proxying ${request.method || 'GET'} ${request.url} via JTAG HTTP transport`);
      
      // Use router's HTTP transport for external requests
      // This leverages the existing JTAG infrastructure instead of direct fetch
      const proxyResponse = await this.makeHTTPRequest(request);
      
      const loadTime = Date.now() - startTime;
      console.log(`✅ ProxyDaemonServer: Proxied ${request.url} (${proxyResponse.statusCode || 200}) in ${loadTime}ms`);

      return proxyResponse;

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.error(`❌ ProxyDaemonServer: Proxy failed for ${request.url}:`, error);
      
      return {
        success: false,
        error: errorMessage
      };
    }
  }

  /**
   * Make HTTP request using native fetch with JTAG integration
   */
  private async makeHTTPRequest(request: ProxyRequest): Promise<ProxyResponse> {
    // Import fetch dynamically for Node.js compatibility
    const fetch = (await import('node-fetch')).default;
    
    // Prepare request options
    const fetchOptions: any = {
      method: request.method || 'GET',
      headers: {
        'User-Agent': this.userAgent,
        ...request.headers
      },
      redirect: request.followRedirects !== false ? 'follow' : 'manual',
      timeout: 30000
    };

    // Add body for POST/PUT requests
    if (request.body && ['POST', 'PUT', 'PATCH'].includes(request.method || 'GET')) {
      fetchOptions.body = request.body;
    }

    const response = await fetch(request.url, fetchOptions);
    const content = await response.text();

    // Process content if URL rewriting is enabled
    const processedContent = request.rewriteUrls 
      ? this.rewriteUrls(content, request.url)
      : content;

    return {
      success: true,
      statusCode: response.status,
      headers: this.processHeaders(response.headers),
      content: processedContent,
      contentType: response.headers.get('content-type') || 'text/html',
      finalUrl: response.url
    };
  }

  /**
   * Rewrite URLs in content to use proxy paths
   */
  private rewriteUrls(content: string, baseUrl: string): string {
    try {
      const base = new URL(baseUrl);
      
      // Rewrite absolute URLs to proxy format
      content = content.replace(
        /(?:href|src|action)=["']https?:\/\/[^"']+["']/gi,
        (match) => {
          const url = match.match(/["']([^"']+)["']/)?.[1];
          if (url && !url.startsWith('/proxy/')) {
            const encodedUrl = encodeURIComponent(url);
            return match.replace(url, `/proxy/${encodedUrl}`);
          }
          return match;
        }
      );

      // Rewrite relative URLs to absolute proxy format
      content = content.replace(
        /(?:href|src|action)=["']\/[^"']*["']/gi,
        (match) => {
          const path = match.match(/["']([^"']+)["']/)?.[1];
          if (path && !path.startsWith('/proxy/')) {
            const absoluteUrl = new URL(path, base).href;
            const encodedUrl = encodeURIComponent(absoluteUrl);
            return match.replace(path, `/proxy/${encodedUrl}`);
          }
          return match;
        }
      );

      // Rewrite CSS url() references
      content = content.replace(
        /url\(["']?([^"')]+)["']?\)/gi,
        (match, url) => {
          if (url.startsWith('http') && !url.includes('/proxy/')) {
            const encodedUrl = encodeURIComponent(url);
            return `url("/proxy/${encodedUrl}")`;
          } else if (url.startsWith('/') && !url.startsWith('/proxy/')) {
            const absoluteUrl = new URL(url, base).href;
            const encodedUrl = encodeURIComponent(absoluteUrl);
            return `url("/proxy/${encodedUrl}")`;
          }
          return match;
        }
      );

      return content;

    } catch (error) {
      console.warn('ProxyDaemonServer: URL rewriting failed:', error);
      return content;
    }
  }

  /**
   * Process response headers for proxy forwarding
   */
  private processHeaders(headers: any): Record<string, string> {
    const processed: Record<string, string> = {};
    
    // Forward safe headers
    const safeHeaders = [
      'content-type',
      'content-length',
      'last-modified',
      'etag',
      'cache-control'
    ];

    for (const [key, value] of headers.entries()) {
      const lowerKey = key.toLowerCase();
      if (safeHeaders.includes(lowerKey)) {
        processed[key] = value;
      }
    }

    // Add CORS headers for browser compatibility
    processed['Access-Control-Allow-Origin'] = '*';
    processed['Access-Control-Allow-Methods'] = 'GET, POST, PUT, DELETE, OPTIONS';
    processed['Access-Control-Allow-Headers'] = 'Content-Type, Authorization';

    return processed;
  }

  /**
   * Health check for proxy functionality
   */
  async healthCheck(): Promise<boolean> {
    try {
      // Test proxy with a simple request
      const testResult = await this.executeProxy({
        url: 'https://httpbin.org/get',
        method: 'GET',
        rewriteUrls: false
      });

      return testResult.success && testResult.statusCode === 200;
    } catch (error) {
      console.error('ProxyDaemonServer: Health check failed:', error);
      return false;
    }
  }
}