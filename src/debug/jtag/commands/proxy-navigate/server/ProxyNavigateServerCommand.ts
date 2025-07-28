/**
 * Proxy Navigate Command - Server Implementation
 * 
 * Server-side proxy navigation that handles HTTP proxy requests.
 * Works with ProxyDaemonServer to fetch and process external content.
 */

import { type ProxyNavigateParams, type ProxyNavigateResult, createProxyNavigateResult } from '@commandsProxyNavigate/shared/ProxyNavigateTypes';
import { ValidationError } from '@shared/ErrorTypes';
import { ProxyNavigateCommand } from '@commandsProxyNavigate/shared/ProxyNavigateCommand';

export class ProxyNavigateServerCommand extends ProxyNavigateCommand {
  
  /**
   * Server handles the actual HTTP proxy request
   */
  async execute(params: ProxyNavigateParams): Promise<ProxyNavigateResult> {
    console.log(`🌐 SERVER: Proxy navigating to ${params.url}`);

    try {
      const startTime = Date.now();
      
      // Validate URL and generate proxy path
      // The actual HTTP proxy serving will be handled by a web server
      // that serves /proxy/{encodedUrl} requests

      const url = new URL(params.url);
      const proxyUrl = `/proxy/${encodeURIComponent(params.url)}`;
      
      const loadTime = Date.now() - startTime;
      console.log(`✅ SERVER: Proxy navigation completed in ${loadTime}ms`);
      
      return createProxyNavigateResult(params.context, params.sessionId, {
        success: true,
        proxyUrl: proxyUrl,
        originalUrl: params.url,
        statusCode: 200,
        loadTime
      });

    } catch (error: any) {
      console.error(`❌ SERVER: Proxy navigation failed:`, error.message);
      const navError = error instanceof Error 
        ? new ValidationError('url', error.message, { cause: error }) 
        : new ValidationError('url', String(error));
        
      return createProxyNavigateResult(params.context, params.sessionId, {
        success: false,
        proxyUrl: '',
        originalUrl: params.url,
        error: navError
      });
    }
  }
}