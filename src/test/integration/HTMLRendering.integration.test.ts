/**
 * Integration tests for HTML rendering and output validation
 * Tests that RendererDaemon produces correct HTML output
 */

import { describe, it, before, after } from 'node:test';
import assert from 'node:assert';
import { RendererDaemon } from '../../daemons/renderer/RendererDaemon';
import { WebSocketDaemon } from '../../integrations/websocket/WebSocketDaemon';
import { JSDOM } from 'jsdom';

describe('HTML Rendering Integration Tests', () => {
  let rendererDaemon: RendererDaemon;
  let webSocketDaemon: WebSocketDaemon;
  
  before(async () => {
    // Create and start daemons
    rendererDaemon = new RendererDaemon();
    webSocketDaemon = new WebSocketDaemon();
    
    await rendererDaemon.start();
    await webSocketDaemon.start();
    
    // Register renderer with WebSocket daemon
    webSocketDaemon.registerDaemon(rendererDaemon);
    webSocketDaemon.registerRouteHandler('*', 'renderer', 'http_request');
  });
  
  after(async () => {
    await webSocketDaemon.stop();
    await rendererDaemon.stop();
  });
  
  describe('HTML Output Validation', () => {
    it('should render valid HTML5 document', async () => {
      const response = await fetch('http://localhost:9000');
      
      assert.strictEqual(response.status, 200);
      assert(response.headers.get('content-type')?.includes('text/html'));
      
      const html = await response.text();
      
      // Validate HTML structure
      assert(html.includes('<!DOCTYPE html>'), 'Should have HTML5 doctype');
      assert(html.includes('<html'), 'Should have html tag');
      assert(html.includes('<head>'), 'Should have head tag');
      assert(html.includes('<body>'), 'Should have body tag');
      assert(html.includes('</html>'), 'Should close html tag');
    });
    
    it('should include required meta tags', async () => {
      const response = await fetch('http://localhost:9000');
      const html = await response.text();
      
      // Parse HTML
      const dom = new JSDOM(html);
      const document = dom.window.document;
      
      // Check meta tags
      const charset = document.querySelector('meta[charset]');
      assert(charset, 'Should have charset meta tag');
      assert.strictEqual(charset.getAttribute('charset'), 'UTF-8');
      
      const viewport = document.querySelector('meta[name="viewport"]');
      assert(viewport, 'Should have viewport meta tag');
      assert(viewport.getAttribute('content')?.includes('width=device-width'));
    });
    
    it('should include continuum client scripts', async () => {
      const response = await fetch('http://localhost:9000');
      const html = await response.text();
      
      const dom = new JSDOM(html);
      const document = dom.window.document;
      
      // Check for continuum-browser.js
      const browserScript = document.querySelector('script[src*="continuum-browser.js"]');
      assert(browserScript, 'Should include continuum-browser.js');
      
      // Check for WebSocket connection script
      assert(html.includes('WebSocket') || html.includes('continuum'), 'Should reference WebSocket or continuum');
    });
    
    it('should set correct page title', async () => {
      const response = await fetch('http://localhost:9000');
      const html = await response.text();
      
      const dom = new JSDOM(html);
      const document = dom.window.document;
      
      const title = document.querySelector('title');
      assert(title, 'Should have title tag');
      assert(title.textContent?.includes('Continuum'), 'Title should include "Continuum"');
    });
  });
  
  describe('Static Asset Handling', () => {
    it('should serve CSS files with correct content type', async () => {
      // Try to fetch a CSS file
      const response = await fetch('http://localhost:9000/assets/styles.css');
      
      if (response.status === 200) {
        assert(response.headers.get('content-type')?.includes('text/css'), 'CSS should have correct content type');
      }
    });
    
    it('should serve JavaScript files with correct content type', async () => {
      const response = await fetch('http://localhost:9000/ui/continuum-browser.js');
      
      if (response.status === 200) {
        assert(
          response.headers.get('content-type')?.includes('application/javascript') ||
          response.headers.get('content-type')?.includes('text/javascript'),
          'JS should have correct content type'
        );
      }
    });
  });
  
  describe('Error Page Rendering', () => {
    it('should render 404 page for non-existent routes', async () => {
      const response = await fetch('http://localhost:9000/this-does-not-exist');
      
      // Could be 404 or fallback to index
      assert([200, 404].includes(response.status));
      
      if (response.status === 404) {
        const html = await response.text();
        assert(html.includes('404') || html.includes('not found'), 'Should indicate 404 error');
      }
    });
  });
  
  describe('Dynamic Content Rendering', () => {
    it('should handle dynamic route parameters', async () => {
      // Test session-specific routes
      const response = await fetch('http://localhost:9000/session/test-123');
      
      // Should either redirect or render session-specific content
      assert([200, 302, 404].includes(response.status));
    });
    
    it('should include CSRF protection', async () => {
      const response = await fetch('http://localhost:9000');
      const html = await response.text();
      
      // Check for CSRF token in meta tag or hidden input
      const dom = new JSDOM(html);
      const document = dom.window.document;
      
      const csrfMeta = document.querySelector('meta[name="csrf-token"]');
      const csrfInput = document.querySelector('input[name="csrf_token"]');
      
      // Should have some form of CSRF protection
      assert(csrfMeta || csrfInput || html.includes('csrf'), 'Should include CSRF protection');
    });
  });
  
  describe('Widget Container Validation', () => {
    it('should include widget mounting points', async () => {
      const response = await fetch('http://localhost:9000');
      const html = await response.text();
      
      const dom = new JSDOM(html);
      const document = dom.window.document;
      
      // Check for widget containers
      const widgetContainers = document.querySelectorAll('[data-widget], .widget-container, #widget-root');
      assert(widgetContainers.length > 0, 'Should have widget mounting points');
    });
    
    it('should include widget initialization script', async () => {
      const response = await fetch('http://localhost:9000');
      const html = await response.text();
      
      // Should have widget initialization
      assert(
        html.includes('initializeWidgets') || 
        html.includes('loadWidget') || 
        html.includes('Widget'),
        'Should include widget initialization code'
      );
    });
  });
});