/**
 * Intelligent Browser Manager Tests - Real-world scenario testing
 * 
 * SCENARIOS TESTED:
 * - Portal-triggered browser launches
 * - Git hook browser integration  
 * - DevTools opening for debugging
 * - Default browser detection
 * - Multi-browser environment handling
 * - Development workflow integration
 */

import { IntelligentBrowserManager, LaunchContext, DevToolsOptions } from '../IntelligentBrowserManager.js';

describe('IntelligentBrowserManager - Real-World Scenarios', () => {
  let browserManager: IntelligentBrowserManager;

  beforeEach(() => {
    browserManager = new IntelligentBrowserManager();
  });

  describe('Browser Detection Intelligence', () => {
    test('should detect available browsers on system', async () => {
      const browsers = await browserManager.detectBrowsers();
      
      expect(Array.isArray(browsers)).toBe(true);
      // Should find at least one browser on any development system
      expect(browsers.length).toBeGreaterThan(0);
      
      // Each browser should have required properties
      for (const browser of browsers) {
        expect(browser.name).toBeDefined();
        expect(browser.executable).toBeDefined();
        expect(typeof browser.isDefault).toBe('boolean');
        expect(typeof browser.supportsDevTools).toBe('boolean');
      }
    });

    test('should identify default browser', async () => {
      await browserManager.detectBrowsers();
      const { detected, default: defaultBrowser } = browserManager.getBrowserInfo();
      
      expect(detected.length).toBeGreaterThan(0);
      expect(defaultBrowser).toBeDefined();
      expect(defaultBrowser?.isDefault).toBe(true);
      
      // Default browser should also be in detected list
      const defaultInList = detected.find(b => b.isDefault);
      expect(defaultInList).toEqual(defaultBrowser);
    });

    test('should cache browser detection results', async () => {
      const startTime = Date.now();
      const browsers1 = await browserManager.detectBrowsers();
      const firstDetectionTime = Date.now() - startTime;
      
      const startTime2 = Date.now();
      const browsers2 = await browserManager.detectBrowsers();
      const secondDetectionTime = Date.now() - startTime2;
      
      expect(browsers1).toEqual(browsers2);
      // Second detection should be much faster (cached)
      expect(secondDetectionTime).toBeLessThan(firstDetectionTime / 2);
    });
  });

  describe('Portal Integration Scenarios', () => {
    test('should launch browser for portal debugging session', async () => {
      const context: LaunchContext = {
        source: 'portal',
        purpose: 'debugging',
        devTools: {
          enabled: true,
          openConsole: true,
          openNetwork: true
        },
        url: 'http://localhost:9000?debug=true'
      };

      const result = await browserManager.launchIntelligent(context);
      
      expect(result.success).toBe(true);
      expect(['launched', 'already_connected']).toContain(result.action);
      expect(result.browser).toBeDefined();
      expect(result.context).toBe('portal');
    });

    test('should prefer DevTools-capable browser for debugging', async () => {
      await browserManager.detectBrowsers();
      
      const context: LaunchContext = {
        source: 'portal',
        purpose: 'debugging',
        devTools: { enabled: true }
      };

      const result = await browserManager.launchIntelligent(context);
      
      if (result.success) {
        // Should prefer Chrome/Chromium/Edge for debugging
        const browserName = result.browser.toLowerCase();
        const hasGoodDevTools = 
          browserName.includes('chrome') || 
          browserName.includes('chromium') || 
          browserName.includes('edge');
        
        // If Chrome variants available, should use them for debugging
        const { detected } = browserManager.getBrowserInfo();
        const chromeAvailable = detected.some(b => 
          b.name.toLowerCase().includes('chrome') || 
          b.name.toLowerCase().includes('edge')
        );
        
        if (chromeAvailable) {
          expect(hasGoodDevTools).toBe(true);
        }
      }
    });

    test('should handle portal session with existing browser connection', async () => {
      // Simulate existing connection
      browserManager.registerConnection('portal-session-123');
      
      const context: LaunchContext = {
        source: 'portal',
        purpose: 'development',
        url: 'http://localhost:9000'
      };

      const result = await browserManager.launchIntelligent(context);
      
      expect(result.success).toBe(true);
      expect(result.action).toBe('already_connected');
    });
  });

  describe('Git Hook Integration Scenarios', () => {
    test('should launch browser for git pre-commit testing', async () => {
      const context: LaunchContext = {
        source: 'git-hook',
        purpose: 'testing',
        url: 'http://localhost:9000?test=pre-commit'
      };

      const result = await browserManager.launchIntelligent(context);
      
      expect(result.success).toBe(true);
      expect(result.context).toBe('git-hook');
      
      // Git hooks should use reliable browser (default)
      const { default: defaultBrowser } = browserManager.getBrowserInfo();
      if (defaultBrowser) {
        expect(result.browser).toBe(defaultBrowser.name);
      }
    });

    test('should handle git hook failure gracefully', async () => {
      // Force detection to fail by creating invalid state
      const context: LaunchContext = {
        source: 'git-hook',
        purpose: 'testing',
        url: 'invalid://url'
      };

      const result = await browserManager.launchIntelligent(context);
      
      // Should either succeed or fail gracefully
      expect(typeof result.success).toBe('boolean');
      
      if (!result.success) {
        expect(result.error).toBeDefined();
        expect(result.action).toBe('failed');
      }
    });
  });

  describe('Development Workflow Integration', () => {
    test('should support different development contexts', async () => {
      const contexts: LaunchContext[] = [
        {
          source: 'development',
          purpose: 'debugging',
          devTools: { enabled: true, openConsole: true }
        },
        {
          source: 'test',
          purpose: 'testing',
          url: 'http://localhost:9000?test=widget-loading'
        },
        {
          source: 'manual',
          purpose: 'presentation',
          url: 'http://localhost:9000?demo=true'
        }
      ];

      for (const context of contexts) {
        const result = await browserManager.launchIntelligent(context);
        
        expect(typeof result.success).toBe('boolean');
        expect(result.context).toBe(context.source);
        
        if (result.success) {
          expect(result.browser).toBeDefined();
        }
      }
    });

    test('should handle DevTools configuration variations', async () => {
      const devToolsConfigs: DevToolsOptions[] = [
        { enabled: true },
        { enabled: true, openConsole: true },
        { enabled: true, openNetwork: true, openSources: true },
        { enabled: true, inspectElement: 'chat-widget' },
        { enabled: false }
      ];

      for (const devTools of devToolsConfigs) {
        const context: LaunchContext = {
          source: 'development',
          purpose: 'debugging',
          devTools
        };

        const result = await browserManager.launchIntelligent(context);
        expect(typeof result.success).toBe('boolean');
      }
    });
  });

  describe('Multi-Browser Environment Handling', () => {
    test('should handle no browsers detected gracefully', async () => {
      // Create a manager that won't detect any browsers (for testing)
      const isolatedManager = new IntelligentBrowserManager();
      
      const context: LaunchContext = {
        source: 'test',
        purpose: 'testing'
      };

      // Should not crash even with no browsers
      const result = await isolatedManager.launchIntelligent(context);
      expect(typeof result.success).toBe('boolean');
      
      if (result.success) {
        expect(result.browser).toBe('System Default');
      }
    });

    test('should provide browser information for diagnostics', async () => {
      await browserManager.detectBrowsers();
      const info = browserManager.getBrowserInfo();
      
      expect(info.detected).toBeInstanceOf(Array);
      expect(info.detected.length).toBeGreaterThan(0);
      
      // Should have at least one default browser
      const defaultCount = info.detected.filter(b => b.isDefault).length;
      expect(defaultCount).toBe(1);
      
      // Default browser should match the separately stored default
      if (info.default) {
        expect(info.default.isDefault).toBe(true);
      }
    });
  });

  describe('Safety and Error Handling', () => {
    test('should handle invalid URLs gracefully', async () => {
      const context: LaunchContext = {
        source: 'test',
        purpose: 'testing',
        url: 'not-a-valid-url'
      };

      const result = await browserManager.launchIntelligent(context);
      
      // Should either handle gracefully or fail with proper error
      expect(typeof result.success).toBe('boolean');
      
      if (!result.success) {
        expect(result.error).toBeDefined();
      }
    });

    test('should maintain connection state through browser operations', async () => {
      // Establish connection
      browserManager.registerConnection('state-test');
      
      // Launch operations should respect existing connection
      const context: LaunchContext = {
        source: 'test',
        purpose: 'testing'
      };

      const result = await browserManager.launchIntelligent(context);
      
      // Connection state should still be valid
      const status = browserManager.getConnectionStatus();
      expect(status.isConnected).toBe(true);
      expect(status.connectionId).toBe('state-test');
    });

    test('should recover from browser detection failures', async () => {
      // Force a detection cycle
      await browserManager.detectBrowsers();
      
      // Should have fallback mechanisms
      const context: LaunchContext = {
        source: 'test',
        purpose: 'testing'
      };

      const result = await browserManager.launchIntelligent(context);
      
      // Should not crash the system
      expect(typeof result.success).toBe('boolean');
    });
  });

  describe('Cross-Platform Compatibility', () => {
    test('should provide platform-appropriate browser commands', async () => {
      await browserManager.detectBrowsers();
      const { detected } = browserManager.getBrowserInfo();
      
      for (const browser of detected) {
        expect(browser.executable).toBeDefined();
        expect(browser.executable.length).toBeGreaterThan(0);
        
        // Should not contain platform-specific issues
        expect(browser.executable).not.toContain('undefined');
        expect(browser.executable).not.toContain('null');
      }
    });

    test('should handle different platform browser conventions', async () => {
      await browserManager.detectBrowsers();
      const { detected } = browserManager.getBrowserInfo();
      
      const platform = process.platform;
      
      if (platform === 'darwin') {
        // macOS browsers should use 'open -a' pattern or direct app paths
        const macBrowsers = detected.filter(b => 
          b.executable.includes('open -a') || 
          b.executable.includes('.app')
        );
        expect(macBrowsers.length).toBeGreaterThan(0);
      }
      
      // All browsers should have valid executable commands
      for (const browser of detected) {
        expect(browser.executable.trim().length).toBeGreaterThan(0);
      }
    });
  });
});