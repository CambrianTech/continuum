/**
 * Proxy Daemon - Universal Web Proxy for Cross-Origin Access
 * 
 * Enables widgets to access any external website through same-origin proxy,
 * solving CORS and iframe restrictions for training and interaction.
 */

import { DaemonBase } from '../../command-daemon/shared/DaemonBase';
import type { JTAGMessage, JTAGContext } from '../../../system/core/types/JTAGTypes';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';

export interface ProxyRequest {
  url: string;
  method?: 'GET' | 'POST' | 'PUT' | 'DELETE';
  headers?: Record<string, string>;
  body?: string;
  rewriteUrls?: boolean;
  followRedirects?: boolean;
}

export interface ProxyResponse {
  success: boolean;
  statusCode?: number;
  headers?: Record<string, string>;
  content?: string;
  contentType?: string;
  finalUrl?: string;
  error?: string;
}

export abstract class ProxyDaemon extends DaemonBase {
  public readonly subpath = 'proxy';

  constructor(context: JTAGContext, router: JTAGRouter) {
    super('proxy-daemon', context, router);
  }

  /**
   * Proxy any HTTP request through our server
   */
  async proxyRequest(request: ProxyRequest): Promise<ProxyResponse> {
    try {
      console.log(`🌐 ProxyDaemon: Proxying ${request.method || 'GET'} ${request.url}`);
      
      // Route through server-side proxy implementation
      return await this.executeProxy(request);
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.error(`❌ ProxyDaemon: Failed to proxy ${request.url}:`, error);
      
      return {
        success: false,
        error: errorMessage
      };
    }
  }

  /**
   * Create proxy URL for iframe navigation
   */
  getProxyUrl(targetUrl: string): string {
    // Encode the target URL for proxy routing
    const encodedUrl = encodeURIComponent(targetUrl);
    return `/proxy/${encodedUrl}`;
  }

  /**
   * Execute proxy request - implemented by environment-specific daemons
   */
  protected abstract executeProxy(request: ProxyRequest): Promise<ProxyResponse>;

  /**
   * Process incoming JTAG messages
   */
  protected async processMessage(message: JTAGMessage): Promise<any> {
    const payload = message.payload as any;
    
    switch (payload.command) {
      case 'proxyRequest':
        // Extract proxy request parameters from payload
        const proxyRequest: ProxyRequest = {
          url: payload.url,
          method: payload.method || 'GET',
          headers: payload.headers,
          body: payload.body,
          rewriteUrls: payload.rewriteUrls,
          followRedirects: payload.followRedirects
        };
        return await this.proxyRequest(proxyRequest);
        
      case 'getProxyUrl':
        return {
          success: true,
          proxyUrl: this.getProxyUrl(payload.url)
        };
        
      default:
        console.warn(`ProxyDaemon: Unknown command ${payload.command}`);
        return { success: false, error: `Unknown command: ${payload.command}` };
    }
  }
}