/**
 * ContinuonWidget - Integration Tests
 * Tests integration with DOM events, WebSocket, and system status
 */

import { describe, it, expect, beforeEach, jest } from '@jest/globals';

describe('ContinuonWidget Integration', () => {
  let ContinuonWidget: any;
  let widget: any;

  beforeEach(async () => {
    // Mock DOM environment
    const mockShadowRoot = {
      innerHTML: '',
      querySelector: jest.fn(),
      querySelectorAll: jest.fn(() => []),
      addEventListener: jest.fn(),
      appendChild: jest.fn(),
      removeChild: jest.fn()
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

    // Mock document with event system
    (global as any).document = {
      addEventListener: jest.fn(),
      title: 'continuum',
      getElementById: jest.fn(() => ({ href: '' })),
      dispatchEvent: jest.fn()
    };

    // Mock window with continuum API
    (global as any).window = {
      continuum: {
        version: '0.2.2360',
        on: jest.fn(),
        off: jest.fn()
      },
      setInterval: jest.fn((fn, delay) => {
        // Store interval function for testing
        (global as any).mockIntervalFn = fn;
        return 123; // mock interval ID
      }),
      setTimeout: jest.fn((fn, delay) => {
        // For immediate testing, execute the function
        if (delay < 100) fn();
        return 456; // mock timeout ID
      })
    };

    (global as any).localStorage = {
      getItem: jest.fn(),
      setItem: jest.fn()
    };

    const module = await import('../../../Continuon/ContinuonWidget');
    ContinuonWidget = module.ContinuonWidget;
    widget = new ContinuonWidget();
  });

  describe('Initialization Integration', () => {
    it('should set up system monitoring on initialization', async () => {
      const setupSpy = jest.spyOn(widget, 'setupStatusFeed').mockImplementation();
      const eventSpy = jest.spyOn(widget, 'setupEventListeners').mockImplementation();
      
      await widget.initializeWidget();
      
      expect(setupSpy).toHaveBeenCalled();
      expect(eventSpy).toHaveBeenCalled();
      setupSpy.mockRestore();
      eventSpy.mockRestore();
    });

    it('should add initial status message', async () => {
      const addStatusSpy = jest.spyOn(widget, 'addStatusMessage').mockImplementation();
      const updateStatusSpy = jest.spyOn(widget, 'updateStatus').mockImplementation();
      
      await widget.initializeWidget();
      
      expect(addStatusSpy).toHaveBeenCalledWith('System initializing...');
      expect(updateStatusSpy).toHaveBeenCalledWith('yellow', 'Starting up...');
      
      addStatusSpy.mockRestore();
      updateStatusSpy.mockRestore();
    });

    it('should transition to green status after initialization', (done) => {
      jest.spyOn(widget, 'setupEventListeners').mockImplementation();
      jest.spyOn(widget, 'setupStatusFeed').mockImplementation();
      const updateStatusSpy = jest.spyOn(widget, 'updateStatus').mockImplementation();
      
      widget.initializeWidget();
      
      setTimeout(() => {
        expect(updateStatusSpy).toHaveBeenCalledWith('green', 'System ready');
        updateStatusSpy.mockRestore();
        done();
      }, 10);
    });
  });

  describe('Event System Integration', () => {
    it('should register document event listeners', () => {
      widget.setupEventListeners();
      
      expect((global as any).document.addEventListener).toHaveBeenCalledWith('continuum:status-change', expect.any(Function));
      expect((global as any).document.addEventListener).toHaveBeenCalledWith('continuum:emotion', expect.any(Function));
      expect((global as any).document.addEventListener).toHaveBeenCalledWith('continuum:system-event', expect.any(Function));
    });

    it('should handle status change events', () => {
      const updateStatusSpy = jest.spyOn(widget, 'updateStatus').mockImplementation();
      widget.setupEventListeners();
      
      // Get the registered callback
      const statusChangeCallback = (global as any).document.addEventListener.mock.calls
        .find((call: any) => call[0] === 'continuum:status-change')[1];
      
      const mockEvent = {
        detail: { status: 'yellow', message: 'System degraded' }
      };
      
      statusChangeCallback(mockEvent);
      
      expect(updateStatusSpy).toHaveBeenCalledWith('yellow', 'System degraded');
      updateStatusSpy.mockRestore();
    });

    it('should handle emotion events', () => {
      const showEmotionSpy = jest.spyOn(widget, 'showEmotion').mockImplementation();
      widget.setupEventListeners();
      
      const emotionCallback = (global as any).document.addEventListener.mock.calls
        .find((call: any) => call[0] === 'continuum:emotion')[1];
      
      const mockEvent = {
        detail: { emotion: '🎉', duration: 3000 }
      };
      
      emotionCallback(mockEvent);
      
      expect(showEmotionSpy).toHaveBeenCalledWith('🎉', 3000);
      showEmotionSpy.mockRestore();
    });

    it('should handle system events', () => {
      const addStatusSpy = jest.spyOn(widget, 'addStatusMessage').mockImplementation();
      widget.setupEventListeners();
      
      const systemEventCallback = (global as any).document.addEventListener.mock.calls
        .find((call: any) => call[0] === 'continuum:system-event')[1];
      
      const mockEvent = {
        detail: { message: 'New deployment detected' }
      };
      
      systemEventCallback(mockEvent);
      
      expect(addStatusSpy).toHaveBeenCalledWith('New deployment detected');
      addStatusSpy.mockRestore();
    });
  });

  describe('WebSocket Integration', () => {
    it('should set up WebSocket event listeners when API is available', () => {
      widget.setupStatusFeed();
      
      expect((global as any).window.continuum.on).toHaveBeenCalledWith('connected', expect.any(Function));
      expect((global as any).window.continuum.on).toHaveBeenCalledWith('disconnected', expect.any(Function));
      expect((global as any).window.continuum.on).toHaveBeenCalledWith('reconnecting', expect.any(Function));
    });

    it('should handle WebSocket connection events', () => {
      const updateStatusSpy = jest.spyOn(widget, 'updateStatus').mockImplementation();
      widget.setupStatusFeed();
      
      // Get the registered callbacks
      const connectedCallback = (global as any).window.continuum.on.mock.calls
        .find((call: any) => call[0] === 'connected')[1];
      const disconnectedCallback = (global as any).window.continuum.on.mock.calls
        .find((call: any) => call[0] === 'disconnected')[1];
      const reconnectingCallback = (global as any).window.continuum.on.mock.calls
        .find((call: any) => call[0] === 'reconnecting')[1];
      
      // Test connected event
      connectedCallback();
      expect(updateStatusSpy).toHaveBeenCalledWith('green', 'Connected');
      
      // Test disconnected event
      disconnectedCallback();
      expect(updateStatusSpy).toHaveBeenCalledWith('red', 'Disconnected');
      
      // Test reconnecting event
      reconnectingCallback();
      expect(updateStatusSpy).toHaveBeenCalledWith('yellow', 'Reconnecting...');
      
      updateStatusSpy.mockRestore();
    });
  });

  describe('Version Monitoring Integration', () => {
    it('should set up version monitoring interval', () => {
      widget.setupVersionMonitoring();
      
      expect((global as any).window.setInterval).toHaveBeenCalledWith(expect.any(Function), 30000);
    });

    it('should check for version updates periodically', async () => {
      const fetchVersionSpy = jest.spyOn(widget, 'fetchCurrentVersion').mockResolvedValue();
      widget.setupVersionMonitoring();
      
      // Execute the interval function
      if ((global as any).mockIntervalFn) {
        await (global as any).mockIntervalFn();
      }
      
      expect(fetchVersionSpy).toHaveBeenCalled();
      fetchVersionSpy.mockRestore();
    });

    it('should trigger update animation when version changes', async () => {
      const showUpdateSpy = jest.spyOn(widget, 'showUpdateAnimation').mockImplementation();
      const renderSpy = jest.spyOn(widget, 'render').mockImplementation();
      
      widget.currentVersion = '0.2.2359';
      await widget.checkForVersionUpdates();
      
      expect(renderSpy).toHaveBeenCalled();
      expect(showUpdateSpy).toHaveBeenCalled();
      
      showUpdateSpy.mockRestore();
      renderSpy.mockRestore();
    });
  });

  describe('DOM Interaction Integration', () => {
    it('should set up orb click listeners', () => {
      const mockOrb = { addEventListener: jest.fn() };
      widget.shadowRoot.querySelector = jest.fn().mockReturnValue(mockOrb);
      
      widget.setupOrbEventListeners();
      
      expect(mockOrb.addEventListener).toHaveBeenCalledWith('click', expect.any(Function));
    });

    it('should trigger emotion demo on orb click', () => {
      const mockOrb = { addEventListener: jest.fn() };
      widget.shadowRoot.querySelector = jest.fn().mockReturnValue(mockOrb);
      const triggerEmotionSpy = jest.spyOn(widget, 'triggerEmotionDemo').mockImplementation();
      
      widget.setupOrbEventListeners();
      
      // Get the click callback and trigger it
      const clickCallback = mockOrb.addEventListener.mock.calls[0][1];
      clickCallback();
      
      expect(triggerEmotionSpy).toHaveBeenCalled();
      triggerEmotionSpy.mockRestore();
    });
  });

  describe('Status Feed Management Integration', () => {
    it('should handle status feed overflow', () => {
      widget.maxStatusMessages = 2;
      
      // Add multiple messages
      for (let i = 0; i < 6; i++) {
        widget.addStatusMessage(`Message ${i}`);
      }
      
      // Should limit to maxStatusMessages * 2
      expect(widget.statusFeed.length).toBeLessThanOrEqual(4);
    });

    it('should render status messages with fade animations', () => {
      widget.statusFeed = [
        { text: 'Message 1', timestamp: Date.now(), id: '1' },
        { text: 'Message 2', timestamp: Date.now(), id: '2' },
        { text: 'Message 3', timestamp: Date.now(), id: '3' }
      ];
      
      const messagesHtml = widget.renderStatusMessages();
      
      expect(messagesHtml).toContain('fade-0');
      expect(messagesHtml).toContain('fade-1');
      expect(messagesHtml).toContain('fade-2');
      expect(messagesHtml).toContain('animation-delay: 0s');
      expect(messagesHtml).toContain('animation-delay: 0.1s');
      expect(messagesHtml).toContain('animation-delay: 0.2s');
    });
  });

  describe('System Health Integration', () => {
    it('should reflect system health in status color', () => {
      widget.currentStatus = 'green';
      expect(widget.getStatusColor()).toBe('status-healthy');
      
      widget.currentStatus = 'yellow';
      expect(widget.getStatusColor()).toBe('status-degraded');
      
      widget.currentStatus = 'red';
      expect(widget.getStatusColor()).toBe('status-error');
    });

    it('should coordinate with browser title and favicon', () => {
      const updateTitleSpy = jest.spyOn(widget, 'updateTitleAndFavicon').mockImplementation();
      
      widget.updateStatus('green', 'All systems operational');
      
      expect(updateTitleSpy).toHaveBeenCalled();
      updateTitleSpy.mockRestore();
    });
  });
});