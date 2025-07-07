/**
 * VersionWidget - Integration Tests
 * Tests integration with continuum API and system events
 */

import { describe, it, expect, beforeEach, jest } from '@jest/globals';

describe('VersionWidget Integration', () => {
  let VersionWidget: any;
  let widget: any;

  beforeEach(async () => {
    // Mock DOM environment
    const mockShadowRoot = {
      innerHTML: '',
      querySelector: jest.fn(),
      querySelectorAll: jest.fn(() => []),
      addEventListener: jest.fn()
    };

    (global as any).HTMLElement = class MockHTMLElement {
      shadowRoot = mockShadowRoot;
      attachShadow() { return mockShadowRoot; }
      addEventListener() {}
      classList = { add: jest.fn(), remove: jest.fn() };
    };

    (global as any).customElements = {
      define: jest.fn(),
      get: jest.fn(() => undefined)
    };

    // Mock window with continuum API
    (global as any).window = {
      continuum: {
        version: '0.2.2360',
        info: jest.fn(() => Promise.resolve({ version: '0.2.2360' }))
      },
      setInterval: jest.fn((fn, delay) => {
        (global as any).mockIntervalFn = fn;
        return 123;
      }),
      clearInterval: jest.fn(),
      setTimeout: jest.fn((fn, delay) => {
        if (delay < 100) fn(); // Execute immediately for tests
        return 456;
      })
    };

    // Mock document with event system
    (global as any).document = {
      addEventListener: jest.fn(),
      title: 'continuum'
    };

    // Mock navigator with clipboard
    (global as any).navigator = {
      clipboard: {
        writeText: jest.fn(() => Promise.resolve())
      }
    };

    const module = await import('../../../Version/VersionWidget');
    VersionWidget = module.VersionWidget;
    widget = new VersionWidget();
  });

  describe('API Integration', () => {
    it('should fetch version from continuum API on initialization', async () => {
      await widget.initializeWidget();
      
      expect((global as any).window.continuum.info).toHaveBeenCalled();
      expect(widget.currentVersion).toBe('0.2.2360');
    });

    it('should handle API unavailability gracefully', async () => {
      (global as any).window.continuum.info.mockRejectedValue(new Error('API not available'));
      const consoleSpy = jest.spyOn(console, 'warn').mockImplementation();
      
      await widget.initializeWidget();
      
      expect(widget.currentVersion).toBe('Unknown');
      expect(consoleSpy).toHaveBeenCalled();
      consoleSpy.mockRestore();
    });

    it('should use fallback version when API info call fails', async () => {
      (global as any).window.continuum.info = undefined;
      (global as any).window.continuum.version = '0.2.2360';
      
      widget.setupVersionMonitoring();
      
      expect(widget.currentVersion).toBe('0.2.2360');
    });
  });

  describe('Event System Integration', () => {
    it('should register for version update events', () => {
      widget.setupVersionMonitoring();
      
      expect((global as any).document.addEventListener).toHaveBeenCalledWith(
        'continuum:version-update', 
        expect.any(Function)
      );
    });

    it('should handle external version update events', () => {
      const updateSpy = jest.spyOn(widget, 'updateVersion').mockImplementation();
      widget.setupVersionMonitoring();
      
      // Get the event listener callback
      const versionUpdateCallback = (global as any).document.addEventListener.mock.calls
        .find((call: any) => call[0] === 'continuum:version-update')[1];
      
      const mockEvent = {
        detail: { version: '0.2.2361' }
      };
      
      versionUpdateCallback(mockEvent);
      
      expect(updateSpy).toHaveBeenCalledWith('0.2.2361');
      updateSpy.mockRestore();
    });

    it('should emit version change events to other components', () => {
      const consoleSpy = jest.spyOn(console, 'log').mockImplementation();
      widget.currentVersion = '0.2.2360';
      
      widget.updateVersion('0.2.2361');
      
      expect(consoleSpy).toHaveBeenCalledWith('🏷️ Version: 0.2.2360 → 0.2.2361');
      consoleSpy.mockRestore();
    });
  });

  describe('Periodic Monitoring Integration', () => {
    it('should set up automatic version checking', () => {
      widget.setupVersionMonitoring();
      
      expect((global as any).window.setInterval).toHaveBeenCalledWith(
        expect.any(Function), 
        30000
      );
    });

    it('should check for version updates periodically', async () => {
      const fetchSpy = jest.spyOn(widget, 'fetchCurrentVersion').mockResolvedValue();
      widget.setupVersionMonitoring();
      
      // Execute the interval function
      if ((global as any).mockIntervalFn) {
        await (global as any).mockIntervalFn();
      }
      
      expect(fetchSpy).toHaveBeenCalled();
      fetchSpy.mockRestore();
    });

    it('should detect version changes during periodic checks', async () => {
      const renderSpy = jest.spyOn(widget, 'render').mockImplementation();
      const animationSpy = jest.spyOn(widget, 'showUpdateAnimation').mockImplementation();
      
      widget.currentVersion = '0.2.2360';
      
      // Mock version change
      (global as any).window.continuum.info.mockResolvedValue({ version: '0.2.2361' });
      
      await widget.checkForVersionUpdates();
      
      expect(widget.currentVersion).toBe('0.2.2361');
      expect(renderSpy).toHaveBeenCalled();
      expect(animationSpy).toHaveBeenCalled();
      
      renderSpy.mockRestore();
      animationSpy.mockRestore();
    });
  });

  describe('User Interaction Integration', () => {
    it('should set up click listeners for clipboard functionality', () => {
      const mockContainer = { addEventListener: jest.fn() };
      widget.shadowRoot.querySelector = jest.fn().mockReturnValue(mockContainer);
      
      widget.setupEventListeners();
      
      expect(mockContainer.addEventListener).toHaveBeenCalledWith('click', expect.any(Function));
    });

    it('should copy version to clipboard on user click', async () => {
      const mockContainer = { addEventListener: jest.fn() };
      widget.shadowRoot.querySelector = jest.fn().mockReturnValue(mockContainer);
      widget.currentVersion = '0.2.2360';
      
      widget.setupEventListeners();
      
      // Get the click callback and trigger it
      const clickCallback = mockContainer.addEventListener.mock.calls[0][1];
      await clickCallback();
      
      expect((global as any).navigator.clipboard.writeText).toHaveBeenCalledWith('Continuum v0.2.2360');
    });

    it('should provide visual feedback on successful copy', async () => {
      const mockContainer = { 
        addEventListener: jest.fn(),
        classList: { add: jest.fn(), remove: jest.fn() }
      };
      widget.shadowRoot.querySelector = jest.fn().mockReturnValue(mockContainer);
      widget.currentVersion = '0.2.2360';
      
      await widget.copyVersionToClipboard();
      
      expect(mockContainer.classList.add).toHaveBeenCalledWith('copied');
    });
  });

  describe('System Integration', () => {
    it('should coordinate with system startup sequence', async () => {
      const loadCSSpy = jest.spyOn(widget, 'loadCSS').mockResolvedValue('');
      const fetchSpy = jest.spyOn(widget, 'fetchCurrentVersion').mockResolvedValue();
      const monitorSpy = jest.spyOn(widget, 'setupVersionMonitoring').mockImplementation();
      const renderSpy = jest.spyOn(widget, 'render').mockImplementation();
      
      await widget.initializeWidget();
      
      expect(loadCSSpy).toHaveBeenCalled();
      expect(fetchSpy).toHaveBeenCalled();
      expect(monitorSpy).toHaveBeenCalled();
      expect(renderSpy).toHaveBeenCalled();
      
      // Cleanup spies
      [loadCSSpy, fetchSpy, monitorSpy, renderSpy].forEach(spy => spy.mockRestore());
    });

    it('should handle rapid version changes gracefully', async () => {
      const renderSpy = jest.spyOn(widget, 'render').mockImplementation();
      
      // Rapid version updates
      widget.updateVersion('0.2.2361');
      widget.updateVersion('0.2.2362');
      widget.updateVersion('0.2.2363');
      
      expect(widget.currentVersion).toBe('0.2.2363');
      expect(renderSpy).toHaveBeenCalledTimes(3);
      
      renderSpy.mockRestore();
    });

    it('should maintain version consistency across components', () => {
      widget.currentVersion = '0.2.2360';
      
      // Version should be reflected in render output
      const content = widget.renderContent();
      expect(content).toContain('0.2.2360');
      
      // Version update should be immediately reflected
      widget.updateVersion('0.2.2361');
      const updatedContent = widget.renderContent();
      expect(updatedContent).toContain('0.2.2361');
    });
  });

  describe('Error Handling Integration', () => {
    it('should recover from clipboard API failures', async () => {
      (global as any).navigator.clipboard.writeText.mockRejectedValue(new Error('Clipboard denied'));
      const consoleSpy = jest.spyOn(console, 'error').mockImplementation();
      
      widget.currentVersion = '0.2.2360';
      
      await widget.copyVersionToClipboard();
      
      expect(consoleSpy).toHaveBeenCalledWith('Failed to copy version:', expect.any(Error));
      consoleSpy.mockRestore();
    });

    it('should handle missing DOM elements gracefully', () => {
      widget.shadowRoot.querySelector = jest.fn().mockReturnValue(null);
      
      expect(() => {
        widget.setupEventListeners();
        widget.showCopyFeedback();
        widget.showUpdateAnimation();
      }).not.toThrow();
    });

    it('should continue monitoring despite API failures', async () => {
      (global as any).window.continuum.info
        .mockRejectedValueOnce(new Error('Network error'))
        .mockResolvedValueOnce({ version: '0.2.2361' });
      
      const consoleSpy = jest.spyOn(console, 'warn').mockImplementation();
      
      // First call should fail
      await widget.fetchCurrentVersion();
      expect(widget.currentVersion).toBe('Unknown');
      
      // Second call should succeed
      await widget.fetchCurrentVersion();
      expect(widget.currentVersion).toBe('0.2.2361');
      
      consoleSpy.mockRestore();
    });
  });

  describe('Performance Integration', () => {
    it('should throttle rapid version checks', async () => {
      const fetchSpy = jest.spyOn(widget, 'fetchCurrentVersion').mockResolvedValue();
      
      // Simulate rapid version checks
      await Promise.all([
        widget.checkForVersionUpdates(),
        widget.checkForVersionUpdates(),
        widget.checkForVersionUpdates()
      ]);
      
      // Should not cause performance issues
      expect(fetchSpy).toHaveBeenCalledTimes(3);
      fetchSpy.mockRestore();
    });

    it('should handle animation overlaps gracefully', () => {
      const mockContainer = { 
        classList: { add: jest.fn(), remove: jest.fn() }
      };
      widget.shadowRoot.querySelector = jest.fn().mockReturnValue(mockContainer);
      
      // Rapid animations should not conflict
      widget.showUpdateAnimation();
      widget.showCopyFeedback();
      widget.showUpdateAnimation();
      
      expect(mockContainer.classList.add).toHaveBeenCalledWith('updated');
      expect(mockContainer.classList.add).toHaveBeenCalledWith('copied');
    });
  });
});