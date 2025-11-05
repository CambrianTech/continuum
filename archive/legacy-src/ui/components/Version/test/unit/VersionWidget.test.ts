/**
 * VersionWidget - Unit Tests
 * Tests version display and monitoring functionality
 */

import { describe, it, expect, beforeEach, jest } from '@jest/globals';

// Mock DOM environment
const mockShadowRoot = {
  innerHTML: '',
  querySelector: jest.fn(),
  querySelectorAll: jest.fn(() => []),
  addEventListener: jest.fn()
};

const mockWindow = {
  continuum: {
    version: '0.2.2360',
    info: jest.fn(() => Promise.resolve({ version: '0.2.2360' }))
  },
  setInterval: jest.fn(),
  clearInterval: jest.fn()
};

const mockDocument = {
  addEventListener: jest.fn(),
  title: 'continuum'
};

const mockNavigator = {
  clipboard: {
    writeText: jest.fn(() => Promise.resolve())
  }
};

// Set up global mocks
(global as any).window = mockWindow;
(global as any).document = mockDocument;
(global as any).navigator = mockNavigator;
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

describe('VersionWidget', () => {
  let VersionWidget: any;

  beforeEach(async () => {
    jest.clearAllMocks();
    mockShadowRoot.innerHTML = '';
    
    // Reset mock values
    mockWindow.continuum.version = '0.2.2360';
    mockShadowRoot.querySelector.mockReturnValue(null);
    
    const module = await import('../../../Version/VersionWidget');
    VersionWidget = module.VersionWidget;
  });

  describe('Constructor', () => {
    it('should initialize with correct properties', () => {
      const widget = new VersionWidget();
      
      expect(widget.widgetName).toBe('VersionWidget');
      expect(widget.widgetIcon).toBe('🏷️');
      expect(widget.widgetTitle).toBe('System Version');
    });

    it('should initialize with default values', () => {
      const widget = new VersionWidget();
      
      expect(widget.currentVersion).toBe('Loading...');
      expect(widget.lastUpdate).toBeInstanceOf(Date);
    });
  });

  describe('Version Fetching', () => {
    it('should fetch version from continuum API', async () => {
      const widget = new VersionWidget();
      
      await widget.fetchCurrentVersion();
      
      expect(mockWindow.continuum.info).toHaveBeenCalled();
      expect(widget.currentVersion).toBe('0.2.2360');
    });

    it('should handle API failure gracefully', async () => {
      const widget = new VersionWidget();
      mockWindow.continuum.info.mockRejectedValue(new Error('API failed'));
      const consoleSpy = jest.spyOn(console, 'warn').mockImplementation();
      
      await widget.fetchCurrentVersion();
      
      expect(widget.currentVersion).toBe('Unknown');
      expect(consoleSpy).toHaveBeenCalledWith('Could not get version:', expect.any(Error));
      
      consoleSpy.mockRestore();
    });

    it('should update lastUpdate timestamp on successful fetch', async () => {
      const widget = new VersionWidget();
      const beforeTime = new Date();
      
      await widget.fetchCurrentVersion();
      
      expect(widget.lastUpdate).toBeInstanceOf(Date);
      expect(widget.lastUpdate.getTime()).toBeGreaterThanOrEqual(beforeTime.getTime());
    });
  });

  describe('Version Monitoring', () => {
    it('should set up version monitoring interval', () => {
      const widget = new VersionWidget();
      
      widget.setupVersionMonitoring();
      
      expect(mockWindow.setInterval).toHaveBeenCalledWith(expect.any(Function), 30000);
    });

    it('should set up document event listeners', () => {
      const widget = new VersionWidget();
      
      widget.setupVersionMonitoring();
      
      expect(mockDocument.addEventListener).toHaveBeenCalledWith('continuum:version-update', expect.any(Function));
    });

    it('should handle version update events', () => {
      const widget = new VersionWidget();
      const updateSpy = jest.spyOn(widget, 'updateVersion').mockImplementation();
      widget.setupVersionMonitoring();
      
      // Get the event listener callback
      const versionUpdateCallback = mockDocument.addEventListener.mock.calls
        .find((call: any) => call[0] === 'continuum:version-update')[1];
      
      const mockEvent = {
        detail: { version: '0.2.2361' }
      };
      
      versionUpdateCallback(mockEvent);
      
      expect(updateSpy).toHaveBeenCalledWith('0.2.2361');
      updateSpy.mockRestore();
    });

    it('should check for version updates periodically', async () => {
      const widget = new VersionWidget();
      const checkSpy = jest.spyOn(widget, 'checkForVersionUpdates').mockImplementation();
      widget.setupVersionMonitoring();
      
      // Get the interval callback
      const intervalCallback = mockWindow.setInterval.mock.calls[0][0];
      await intervalCallback();
      
      expect(checkSpy).toHaveBeenCalled();
      checkSpy.mockRestore();
    });
  });

  describe('Version Updates', () => {
    it('should update version when different', () => {
      const widget = new VersionWidget();
      widget.currentVersion = '0.2.2360';
      widget.render = jest.fn();
      widget.showUpdateAnimation = jest.fn();
      const consoleSpy = jest.spyOn(console, 'log').mockImplementation();
      
      widget.updateVersion('0.2.2361');
      
      expect(widget.currentVersion).toBe('0.2.2361');
      expect(widget.render).toHaveBeenCalled();
      expect(widget.showUpdateAnimation).toHaveBeenCalled();
      expect(consoleSpy).toHaveBeenCalledWith('🏷️ Version: 0.2.2360 → 0.2.2361');
      
      consoleSpy.mockRestore();
    });

    it('should not update when version is the same', () => {
      const widget = new VersionWidget();
      widget.currentVersion = '0.2.2360';
      widget.render = jest.fn();
      widget.showUpdateAnimation = jest.fn();
      
      widget.updateVersion('0.2.2360');
      
      expect(widget.render).not.toHaveBeenCalled();
      expect(widget.showUpdateAnimation).not.toHaveBeenCalled();
    });

    it('should check for version updates and trigger animation if changed', async () => {
      const widget = new VersionWidget();
      widget.currentVersion = '0.2.2360';
      widget.fetchCurrentVersion = jest.fn().mockImplementation(() => {
        widget.currentVersion = '0.2.2361';
      });
      widget.render = jest.fn();
      widget.showUpdateAnimation = jest.fn();
      
      await widget.checkForVersionUpdates();
      
      expect(widget.render).toHaveBeenCalled();
      expect(widget.showUpdateAnimation).toHaveBeenCalled();
    });
  });

  describe('Rendering', () => {
    it('should render version information', () => {
      const widget = new VersionWidget();
      widget.currentVersion = '0.2.2360';
      widget.lastUpdate = new Date('2025-01-01T12:00:00Z');
      
      const content = widget.renderContent();
      
      expect(content).toContain('version-container');
      expect(content).toContain('0.2.2360');
      expect(content).toContain('Updated');
    });

    it('should format update time correctly', () => {
      const widget = new VersionWidget();
      widget.currentVersion = '0.2.2360';
      
      // Mock a specific time
      const mockDate = new Date('2025-01-01T12:30:00Z');
      widget.lastUpdate = mockDate;
      
      const content = widget.renderContent();
      
      // Should contain formatted time (format depends on locale, but should be there)
      expect(content).toContain('Updated');
      expect(content).toMatch(/\d{1,2}:\d{2}/); // Should contain time format
    });

    it('should include version label and number', () => {
      const widget = new VersionWidget();
      widget.currentVersion = '0.2.2360';
      
      const content = widget.renderContent();
      
      expect(content).toContain('version-label');
      expect(content).toContain('version-number');
      expect(content).toContain('v');
    });
  });

  describe('Clipboard Functionality', () => {
    it('should copy version to clipboard on click', async () => {
      const widget = new VersionWidget();
      widget.currentVersion = '0.2.2360';
      widget.showCopyFeedback = jest.fn();
      
      await widget.copyVersionToClipboard();
      
      expect(mockNavigator.clipboard.writeText).toHaveBeenCalledWith('Continuum v0.2.2360');
      expect(widget.showCopyFeedback).toHaveBeenCalled();
    });

    it('should handle clipboard write failure', async () => {
      const widget = new VersionWidget();
      widget.currentVersion = '0.2.2360';
      mockNavigator.clipboard.writeText.mockRejectedValue(new Error('Clipboard failed'));
      const consoleSpy = jest.spyOn(console, 'error').mockImplementation();
      
      await widget.copyVersionToClipboard();
      
      expect(consoleSpy).toHaveBeenCalledWith('Failed to copy version:', expect.any(Error));
      consoleSpy.mockRestore();
    });

    it('should show copy feedback animation', () => {
      const widget = new VersionWidget();
      const mockContainer = { classList: { add: jest.fn(), remove: jest.fn() } };
      mockShadowRoot.querySelector.mockReturnValue(mockContainer);
      
      widget.showCopyFeedback();
      
      expect(mockContainer.classList.add).toHaveBeenCalledWith('copied');
      
      // Simulate setTimeout callback
      setTimeout(() => {
        expect(mockContainer.classList.remove).toHaveBeenCalledWith('copied');
      }, 1000);
    });
  });

  describe('Animation Effects', () => {
    it('should show update animation', () => {
      const widget = new VersionWidget();
      const mockContainer = { classList: { add: jest.fn(), remove: jest.fn() } };
      mockShadowRoot.querySelector.mockReturnValue(mockContainer);
      
      widget.showUpdateAnimation();
      
      expect(mockContainer.classList.add).toHaveBeenCalledWith('updated');
      
      // Simulate setTimeout callback
      setTimeout(() => {
        expect(mockContainer.classList.remove).toHaveBeenCalledWith('updated');
      }, 600);
    });

    it('should handle missing container gracefully', () => {
      const widget = new VersionWidget();
      mockShadowRoot.querySelector.mockReturnValue(null);
      
      expect(() => {
        widget.showUpdateAnimation();
        widget.showCopyFeedback();
      }).not.toThrow();
    });
  });

  describe('Event Listeners Setup', () => {
    it('should set up click listener on container', () => {
      const widget = new VersionWidget();
      const mockContainer = { addEventListener: jest.fn() };
      mockShadowRoot.querySelector.mockReturnValue(mockContainer);
      
      widget.setupEventListeners();
      
      expect(mockContainer.addEventListener).toHaveBeenCalledWith('click', expect.any(Function));
    });

    it('should handle click events', () => {
      const widget = new VersionWidget();
      const mockContainer = { addEventListener: jest.fn() };
      mockShadowRoot.querySelector.mockReturnValue(mockContainer);
      const copySpy = jest.spyOn(widget, 'copyVersionToClipboard').mockImplementation();
      
      widget.setupEventListeners();
      
      // Get the click callback and trigger it
      const clickCallback = mockContainer.addEventListener.mock.calls[0][1];
      clickCallback();
      
      expect(copySpy).toHaveBeenCalled();
      copySpy.mockRestore();
    });

    it('should handle missing container in event setup', () => {
      const widget = new VersionWidget();
      mockShadowRoot.querySelector.mockReturnValue(null);
      
      expect(() => {
        widget.setupEventListeners();
      }).not.toThrow();
    });
  });

  describe('Initialization', () => {
    it('should initialize widget properly', async () => {
      const widget = new VersionWidget();
      const fetchSpy = jest.spyOn(widget, 'fetchCurrentVersion').mockResolvedValue();
      const monitorSpy = jest.spyOn(widget, 'setupVersionMonitoring').mockImplementation();
      const renderSpy = jest.spyOn(widget, 'render').mockImplementation();
      
      await widget.initializeWidget();
      
      expect(fetchSpy).toHaveBeenCalled();
      expect(monitorSpy).toHaveBeenCalled();
      expect(renderSpy).toHaveBeenCalled();
      
      fetchSpy.mockRestore();
      monitorSpy.mockRestore();
      renderSpy.mockRestore();
    });
  });
});