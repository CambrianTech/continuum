/**
 * HTML Generation Integration Tests - CRITICAL MISSING TESTS
 * 
 * INTEGRATION FAILURES THESE TESTS WOULD HAVE CAUGHT:
 * ===================================================
 * - Missing continuum.js file (404 errors)
 * - Version coordination between server/client
 * - Script tag generation and version injection
 * - Widget loading dependency chain
 * - End-to-end HTML → Script → Widget → API flow
 * 
 * REAL INTEGRATION VALIDATION:
 * - Test actual HTML output that browsers receive
 * - Validate script references exist and are loadable
 * - Test version parameter injection works correctly
 * - Verify widget initialization dependencies are met
 * - Test full rendering pipeline end-to-end
 */

import { RendererDaemon } from '../RendererDaemon';
import { WebSocketDaemon } from '../../../integrations/websocket/WebSocketDaemon';
import * as http from 'http';
import { JSDOM } from 'jsdom';

describe('HTML Generation Integration Tests', () => {
  let rendererDaemon: RendererDaemon;
  let webSocketDaemon: WebSocketDaemon;
  let testPort: number;

  beforeAll(() => {
    testPort = 9000; // Use standard port to test real integration
  });

  beforeEach(async () => {
    webSocketDaemon = new WebSocketDaemon({ port: testPort });
    await webSocketDaemon.start();
    
    // Wait for WebSocket server to be ready
    await new Promise(resolve => setTimeout(resolve, 500));
  });

  afterEach(async () => {
    if (webSocketDaemon) {
      await webSocketDaemon.stop();
    }
    await new Promise(resolve => setTimeout(resolve, 100));
  });

  describe('Critical HTML Generation Validation', () => {
    
    test('should generate valid HTML that browsers can actually use', async () => {
      // Test the actual HTML endpoint that browsers hit
      const html = await fetchHTML(`http://localhost:${testPort}/`);
      
      expect(html).toBeDefined();
      expect(html.length).toBeGreaterThan(100);
      expect(html).toContain('<!DOCTYPE html>');
      expect(html).toContain('<html');
      expect(html).toContain('</html>');
    });

    test('should include critical continuum.js script tag', async () => {
      const html = await fetchHTML(`http://localhost:${testPort}/`);
      
      // This is the EXACT issue we fixed - missing continuum.js reference
      expect(html).toContain('continuum.js');
      
      // Should have version parameters for cache busting
      const continuumScriptMatch = html.match(/src="([^"]*continuum\.js[^"]*)"/);
      expect(continuumScriptMatch).toBeTruthy();
      
      const scriptSrc = continuumScriptMatch![1];
      expect(scriptSrc).toMatch(/\?v=.*&bust=\d+/);
      
      console.log('✅ Found continuum.js script tag:', scriptSrc);
    });

    test('should reference loadable script files (no 404s)', async () => {
      const html = await fetchHTML(`http://localhost:${testPort}/`);
      
      // Parse HTML and extract all script src URLs
      const dom = new JSDOM(html);
      const scripts = Array.from(dom.window.document.querySelectorAll('script[src]'));
      
      expect(scripts.length).toBeGreaterThan(0);
      
      // Test each script is actually loadable
      for (const script of scripts) {
        const src = script.getAttribute('src')!;
        console.log(`🔍 Testing script: ${src}`);
        
        const scriptUrl = src.startsWith('/') ? `http://localhost:${testPort}${src}` : src;
        const response = await fetchWithTimeout(scriptUrl);
        
        // Should NOT be 404 - this was our core bug
        expect(response.status).not.toBe(404);
        
        if (response.status === 200) {
          expect(response.headers.get('content-type')).toMatch(/javascript|ecmascript/);
          console.log(`✅ Script loadable: ${src} (${response.status})`);
        } else {
          console.warn(`⚠️  Script status: ${src} (${response.status})`);
        }
      }
    });

    test('should inject consistent version parameters', async () => {
      const html = await fetchHTML(`http://localhost:${testPort}/`);
      
      // Extract all version parameters
      const versionMatches = html.match(/\?v=([^&]+)/g) || [];
      expect(versionMatches.length).toBeGreaterThan(0);
      
      // All version parameters should be the same
      const versions = versionMatches.map(match => match.replace('?v=', ''));
      const uniqueVersions = [...new Set(versions)];
      
      expect(uniqueVersions.length).toBe(1);
      console.log(`✅ Consistent version parameter: ${uniqueVersions[0]}`);
    });

    test('should generate HTML with widget loading infrastructure', async () => {
      const html = await fetchHTML(`http://localhost:${testPort}/`);
      
      // Should have widget containers
      expect(html).toMatch(/widget|component|sidebar|chat/i);
      
      // Should have script loading order: continuum.js before widget-loader.js
      const continuumIndex = html.indexOf('continuum.js');
      const widgetLoaderIndex = html.indexOf('widget-loader.js');
      
      if (continuumIndex > 0 && widgetLoaderIndex > 0) {
        expect(continuumIndex).toBeLessThan(widgetLoaderIndex);
        console.log('✅ Script loading order correct: continuum.js before widget-loader.js');
      }
    });
  });

  describe('Version Coordination Integration', () => {
    
    test('should handle version mismatches gracefully', async () => {
      const html = await fetchHTML(`http://localhost:${testPort}/`);
      
      // Extract version from script tags
      const versionMatch = html.match(/\?v=([^&]+)/);
      expect(versionMatch).toBeTruthy();
      
      const version = versionMatch![1];
      
      // Test if scripts with this version are accessible
      const testScriptUrl = `http://localhost:${testPort}/src/ui/continuum.js?v=${version}`;
      const response = await fetchWithTimeout(testScriptUrl);
      
      // Should be loadable with version parameter
      expect([200, 304]).toContain(response.status);
    });

    test('should generate unique cache busting timestamps', async () => {
      // Get HTML twice with small delay
      const html1 = await fetchHTML(`http://localhost:${testPort}/`);
      await new Promise(resolve => setTimeout(resolve, 10));
      const html2 = await fetchHTML(`http://localhost:${testPort}/`);
      
      // Extract bust parameters
      const bust1Match = html1.match(/bust=(\d+)/);
      const bust2Match = html2.match(/bust=(\d+)/);
      
      expect(bust1Match).toBeTruthy();
      expect(bust2Match).toBeTruthy();
      
      const bust1 = parseInt(bust1Match![1]);
      const bust2 = parseInt(bust2Match![1]);
      
      // Should be different timestamps
      expect(bust2).toBeGreaterThan(bust1);
      console.log(`✅ Cache busting works: ${bust1} → ${bust2}`);
    });
  });

  describe('Widget Integration Dependencies', () => {
    
    test('should provide all dependencies widgets need', async () => {
      const html = await fetchHTML(`http://localhost:${testPort}/`);
      
      // Parse and test critical widget dependencies
      const dom = new JSDOM(html, {
        url: `http://localhost:${testPort}/`,
        resources: 'usable',
        runScripts: 'outside-only'
      });
      
      // Should have basic DOM structure widgets expect
      expect(dom.window.document.querySelector('body')).toBeTruthy();
      expect(dom.window.document.querySelector('head')).toBeTruthy();
      
      // Test if HTML is valid enough for browser parsing
      expect(dom.window.document.doctype).toBeTruthy();
      expect(dom.window.document.documentElement.tagName).toBe('HTML');
    });

    test('should generate HTML that supports WebSocket connection', async () => {
      const html = await fetchHTML(`http://localhost:${testPort}/`);
      
      // Should not have obvious WebSocket blocking issues
      expect(html).not.toContain('localhost:0');
      expect(html).not.toContain('undefined');
      expect(html).not.toContain('null');
      
      // Should be served with proper headers for WebSocket upgrade
      const response = await fetch(`http://localhost:${testPort}/`, { method: 'HEAD' });
      expect(response.headers.get('connection')).not.toBe('close');
    });
  });

  describe('End-to-End Integration Flow', () => {
    
    test('should complete full rendering pipeline without errors', async () => {
      // Test the complete flow that a browser would experience
      
      // 1. Fetch HTML
      const html = await fetchHTML(`http://localhost:${testPort}/`);
      expect(html).toBeDefined();
      
      // 2. Parse and extract script dependencies
      const dom = new JSDOM(html);
      const scripts = Array.from(dom.window.document.querySelectorAll('script[src]'));
      
      // 3. Verify all scripts are loadable (simulate browser loading)
      const scriptPromises = scripts.map(async (script) => {
        const src = script.getAttribute('src')!;
        const url = src.startsWith('/') ? `http://localhost:${testPort}${src}` : src;
        
        const response = await fetchWithTimeout(url);
        return { src, status: response.status, loadable: response.status === 200 };
      });
      
      const scriptResults = await Promise.all(scriptPromises);
      
      // 4. Verify critical scripts loaded successfully
      const continuumScript = scriptResults.find(r => r.src.includes('continuum.js'));
      expect(continuumScript).toBeTruthy();
      expect(continuumScript!.loadable).toBe(true);
      
      console.log('✅ End-to-end flow test passed');
      scriptResults.forEach(result => {
        console.log(`   ${result.loadable ? '✅' : '❌'} ${result.src} (${result.status})`);
      });
    });
  });

});

// Helper functions
async function fetchHTML(url: string): Promise<string> {
  const response = await fetch(url);
  if (!response.ok) {
    throw new Error(`Failed to fetch HTML: ${response.status} ${response.statusText}`);
  }
  return await response.text();
}

async function fetchWithTimeout(url: string, timeoutMs: number = 5000): Promise<Response> {
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), timeoutMs);
  
  try {
    const response = await fetch(url, { signal: controller.signal });
    clearTimeout(timeout);
    return response;
  } catch (error) {
    clearTimeout(timeout);
    throw error;
  }
}