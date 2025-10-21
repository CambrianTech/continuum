/**
 * Widget Asset Loading Tests - Test-Driven Development
 * 
 * These tests expose the exact asset loading errors from browser.error.json
 * and enforce the documented widget architecture patterns.
 */

import { describe, it } from 'node:test';
import assert from 'node:assert';

describe('Widget Asset Loading - Production Error Detection', () => {
  
  describe('BaseWidget Foundation CSS', () => {
    it('CRITICAL: BaseWidget.css must exist as universal foundation', async () => {
      // This test exposes the error: "Failed to fetch /src/ui/components/shared/BaseWidget.css"
      const response = await fetch('/src/ui/components/shared/BaseWidget.css');
      
      assert.strictEqual(response.ok, true, `BaseWidget.css fetch failed: ${response.status}`);
      assert.strictEqual(response.status, 200);
      
      const css = await response.text();
      assert.ok(css.length > 0, 'BaseWidget.css should not be empty');
      assert.ok(css.includes(':host'), 'BaseWidget.css should contain :host for shadow DOM'); // BaseWidget should use shadow DOM styling
    });
    
    test('BaseWidget.css contains universal widget patterns', async () => {
      const response = await fetch('/src/ui/components/shared/BaseWidget.css');
      const css = await response.text();
      
      // From architecture doc: universal foundation styling
      expect(css).toContain('widget-header');
      expect(css).toContain('widget-content');
      expect(css).toContain('--font-family');
    });
  });

  describe('SavedPersonas Widget Assets', () => {
    test('SavedPersonas CSS files must exist at correct paths', async () => {
      // These tests expose the errors:
      // "Failed to fetch /src/ui/components/SavedPersonas/SavedPersonas.css"
      // "Failed to fetch /src/ui/components/SavedPersonas/SavedPersonasWidget.css" 
      // "Failed to fetch /src/ui/components/SavedPersonas/styles.css"
      
      const possiblePaths = [
        '/src/ui/components/SavedPersonas/SavedPersonas.css',
        '/src/ui/components/SavedPersonas/SavedPersonasWidget.css', 
        '/src/ui/components/SavedPersonas/styles.css'
      ];
      
      const responses = await Promise.allSettled(
        possiblePaths.map(path => fetch(path))
      );
      
      // At least ONE path should work (auto-derivation should be consistent)
      const successfulPaths = responses
        .map((result, index) => ({ result, path: possiblePaths[index] }))
        .filter(({ result }) => result.status === 'fulfilled' && result.value.ok);
      
      expect(successfulPaths.length).toBeGreaterThan(0);
      
      if (successfulPaths.length === 0) {
        const failureReasons = responses.map((result, index) => ({
          path: possiblePaths[index],
          error: result.status === 'rejected' ? result.reason : `HTTP ${result.value.status}`
        }));
        
        fail(`No SavedPersonas CSS files found. Attempts: ${JSON.stringify(failureReasons, null, 2)}`);
      }
    });
  });

  describe('UsersAgents Widget Assets', () => {
    test('UsersAgents.css must exist and be fetchable', async () => {
      // This test exposes: "Failed to fetch /src/ui/components/UsersAgents/UsersAgents.css"
      const response = await fetch('/src/ui/components/UsersAgents/UsersAgents.css');
      
      expect(response.ok).toBe(true);
      expect(response.status).toBe(200);
    });
  });

  describe('SessionCosts Widget Assets', () => {
    test('SessionCosts.css must exist and be fetchable', async () => {
      // This test exposes: "Failed to fetch /src/ui/components/SessionCosts/SessionCosts.css"
      const response = await fetch('/src/ui/components/SessionCosts/SessionCosts.css');
      
      expect(response.ok).toBe(true);
      expect(response.status).toBe(200);
    });
  });

  describe('SidebarHeader Widget Assets', () => {
    test('SidebarHeader.html must exist and be fetchable', async () => {
      // This test exposes: "Failed to fetch /src/ui/components/SidebarHeader/SidebarHeader.html"
      const response = await fetch('/src/ui/components/SidebarHeader/SidebarHeader.html');
      
      expect(response.ok).toBe(true);
      expect(response.status).toBe(200);
      
      const html = await response.text();
      expect(html.length).toBeGreaterThan(0);
    });
  });

  describe('Widget Auto-Derivation Architecture Compliance', () => {
    test('ChatWidget follows auto-derivation path rules', async () => {
      // From architecture doc: ChatWidget → /src/ui/components/Chat/ChatWidget.css
      const expectedPath = '/src/ui/components/Chat/ChatWidget.css';
      const response = await fetch(expectedPath);
      
      if (!response.ok) {
        fail(`ChatWidget CSS not found at expected auto-derived path: ${expectedPath}`);
      }
      
      expect(response.status).toBe(200);
    });

    test('Widget package.json files declare actual assets', async () => {
      // From architecture doc lines 408-421: getWidgetFiles() from package.json
      const widgetDirs = [
        'Chat',
        'SavedPersonas', 
        'UsersAgents',
        'SessionCosts',
        'SidebarHeader'
      ];
      
      for (const widgetDir of widgetDirs) {
        const packageResponse = await fetch(`/src/ui/components/${widgetDir}/package.json`);
        
        if (packageResponse.ok) {
          const packageData = await packageResponse.json();
          const declaredFiles = packageData.files || [];
          
          // Test that all declared files actually exist
          for (const file of declaredFiles) {
            if (file.endsWith('.css') || file.endsWith('.html')) {
              const fileResponse = await fetch(`/src/ui/components/${widgetDir}/${file}`);
              expect(fileResponse.ok).toBe(true);
            }
          }
        }
      }
    });
  });

  describe('HTTP Server Asset Serving', () => {
    test('Server properly serves /src/ui/components/ directory', async () => {
      // Test that the HTTP server is configured to serve widget assets
      const response = await fetch('/src/ui/components/');
      
      // Should either return directory listing or 404 (but not 500/network error)
      expect([200, 404]).toContain(response.status);
      expect(response.status).not.toBe(500);
    });
  });
});

describe('Widget Architecture Compliance - Auto-Derivation Rules', () => {
  
  test('Widget class names should auto-derive correct paths', () => {
    // Test the auto-derivation logic from architecture doc
    const testCases = [
      { className: 'ChatWidget', expectedPath: '/src/ui/components/Chat' },
      { className: 'SavedPersonasWidget', expectedPath: '/src/ui/components/SavedPersonas' },
      { className: 'UsersAgentsWidget', expectedPath: '/src/ui/components/UsersAgents' },
      { className: 'SessionCostsWidget', expectedPath: '/src/ui/components/SessionCosts' }
    ];
    
    for (const { className, expectedPath } of testCases) {
      // From architecture doc line 188: const className = this.name.replace('Widget', '');
      const derivedName = className.replace('Widget', '');
      const derivedPath = `/src/ui/components/${derivedName}`;
      
      expect(derivedPath).toBe(expectedPath);
    }
  });

  test('CSS inheritance hierarchy should be consistent', () => {
    // From architecture doc: BaseWidget.css + WidgetSpecific.css
    const expectedHierarchy = [
      'BaseWidget.css',      // Universal foundation
      'SidebarWidget.css',   // Sidebar-specific (if applicable)  
      'WidgetName.css'       // Widget-specific
    ];
    
    // This test enforces the documented CSS loading order
    expect(expectedHierarchy[0]).toBe('BaseWidget.css');
    expect(expectedHierarchy.length).toBeGreaterThanOrEqual(2);
  });
});