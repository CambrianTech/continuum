/**
 * Proxy Navigate Command - Browser Implementation
 * 
 * Browser-side proxy navigation that sets iframe src to proxy URL.
 * Enables cross-origin screenshot capture by making content same-origin.
 */

import { type ProxyNavigateParams, type ProxyNavigateResult, createProxyNavigateResult } from '@commandsProxyNavigate/shared/ProxyNavigateTypes';
import { ValidationError } from '@shared/ErrorTypes';
import { ProxyNavigateCommand } from '@commandsProxyNavigate/shared/ProxyNavigateCommand';

export class ProxyNavigateBrowserCommand extends ProxyNavigateCommand {
  
  /**
   * Browser sets iframe to proxy URL, enabling screenshot capture
   */
  async execute(params: ProxyNavigateParams): Promise<ProxyNavigateResult> {
    console.log(`🌐 BROWSER: Proxy navigating to ${params.url}`);

    try {
      const startTime = Date.now();
      
      // Create proxy URL
      const proxyUrl = `/proxy/${encodeURIComponent(params.url)}`;
      
      // Find target iframe or create one
      let iframe = document.querySelector(`#${params.target}`) as HTMLIFrameElement;
      if (!iframe) {
        // If no iframe exists, we could create one, but for now just report the proxy URL
        console.log(`📋 BROWSER: Target iframe #${params.target} not found, returning proxy URL`);
      } else {
        // Navigate iframe to proxy URL
        iframe.src = proxyUrl;
        console.log(`🎯 BROWSER: Set iframe src to ${proxyUrl}`);
      }
      
      const loadTime = Date.now() - startTime;
      console.log(`✅ BROWSER: Proxy navigation prepared in ${loadTime}ms`);
      
      return createProxyNavigateResult(params.context, params.sessionId, {
        success: true,
        proxyUrl: proxyUrl,
        originalUrl: params.url,
        loadTime
      });

    } catch (error: any) {
      console.error(`❌ BROWSER: Proxy navigation failed:`, error.message);
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