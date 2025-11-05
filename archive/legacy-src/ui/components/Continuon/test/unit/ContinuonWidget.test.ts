/**
 * ContinuonWidget - Unit Tests
 * Tests HAL 9000-style status orb functionality
 */

import { describe, it, expect, beforeEach, jest } from '@jest/globals';

// Mock DOM environment
const mockShadowRoot = {
  innerHTML: '',
  querySelector: jest.fn(),
  querySelectorAll: jest.fn(() => []),
  addEventListener: jest.fn(),
  appendChild: jest.fn(),
  removeChild: jest.fn()
};

const mockWindow = {
  continuum: {
    version: '0.2.2360',
    on: jest.fn(),
    off: jest.fn()
  }
};

const mockDocument = {
  addEventListener: jest.fn(),
  title: 'continuum',
  getElementById: jest.fn()
};

const mockLocalStorage = {
  getItem: jest.fn(),
  setItem: jest.fn()
};

// Set up global mocks
(global as any).window = mockWindow;
(global as any).document = mockDocument;
(global as any).localStorage = mockLocalStorage;
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

describe('ContinuonWidget', () => {
  let ContinuonWidget: any;

  beforeEach(async () => {
    jest.clearAllMocks();
    mockShadowRoot.innerHTML = '';
    
    // Reset widget state
    mockWindow.continuum.version = '0.2.2360';
    mockDocument.title = 'continuum';
    
    const module = await import('../../../Continuon/ContinuonWidget');
    ContinuonWidget = module.ContinuonWidget;
  });

  describe('Constructor', () => {
    it('should initialize with correct properties', () => {
      const widget = new ContinuonWidget();
      
      expect(widget.widgetName).toBe('ContinuonWidget');
      expect(widget.widgetIcon).toBe('🔮');
      expect(widget.widgetTitle).toBe('System Status Orb');
    });

    it('should have static CSS declaration', () => {
      expect(ContinuonWidget.getOwnCSS()).toEqual(['ContinuonWidget.css']);
    });

    it('should initialize with default status', () => {
      const widget = new ContinuonWidget();
      
      expect(widget.currentStatus).toBe('red');
      expect(widget.currentEmotion).toBeNull();
      expect(widget.statusFeed).toEqual([]);
    });
  });

  describe('Status Management', () => {
    it('should update status correctly', () => {
      const widget = new ContinuonWidget();
      widget.render = jest.fn();
      widget.updateTitleAndFavicon = jest.fn();
      widget.logVersionIfChanged = jest.fn();
      
      widget.updateStatus('green', 'System ready');
      
      expect(widget.currentStatus).toBe('green');
      expect(widget.render).toHaveBeenCalled();
      expect(widget.updateTitleAndFavicon).toHaveBeenCalled();
      expect(widget.logVersionIfChanged).toHaveBeenCalled();
    });

    it('should not update if status is the same', () => {
      const widget = new ContinuonWidget();
      widget.currentStatus = 'green';
      widget.render = jest.fn();
      
      widget.updateStatus('green');
      
      expect(widget.render).not.toHaveBeenCalled();
    });

    it('should add status message when status changes', () => {
      const widget = new ContinuonWidget();
      widget.render = jest.fn();
      widget.updateTitleAndFavicon = jest.fn();
      widget.logVersionIfChanged = jest.fn();
      
      widget.updateStatus('yellow', 'System degraded');
      
      expect(widget.statusFeed.length).toBe(1);
      expect(widget.statusFeed[0].text).toBe('System degraded');
    });
  });

  describe('Status Color Mapping', () => {
    it('should return correct status colors', () => {
      const widget = new ContinuonWidget();
      
      expect(widget.getStatusColor('green')).toBe('status-healthy');
      expect(widget.getStatusColor('yellow')).toBe('status-degraded');
      expect(widget.getStatusColor('red')).toBe('status-error');
      expect(widget.getStatusColor('unknown')).toBe('status-error');
    });
  });

  describe('Status Icons', () => {
    it('should return correct status icons', () => {
      const widget = new ContinuonWidget();
      
      expect(widget.getStatusIcon('green')).toBe('🟢');
      expect(widget.getStatusIcon('yellow')).toBe('🟡');
      expect(widget.getStatusIcon('red')).toBe('🔴');
      expect(widget.getStatusIcon('unknown')).toBe('🔴');
    });
  });

  describe('Emotion System', () => {
    it('should show emotion temporarily', (done) => {
      const widget = new ContinuonWidget();
      widget.render = jest.fn();
      widget.updateTitleAndFavicon = jest.fn();
      
      widget.showEmotion('😉', 100);
      
      expect(widget.currentEmotion).toBe('😉');
      expect(widget.render).toHaveBeenCalled();
      expect(widget.updateTitleAndFavicon).toHaveBeenCalled();
      
      setTimeout(() => {
        expect(widget.currentEmotion).toBeNull();
        done();
      }, 150);
    });

    it('should trigger random emotion on demo', () => {
      const widget = new ContinuonWidget();
      widget.showEmotion = jest.fn();
      
      widget.triggerEmotionDemo();
      
      expect(widget.showEmotion).toHaveBeenCalledWith(expect.any(String), 2000);
    });
  });

  describe('Status Feed Management', () => {
    it('should add status messages', () => {
      const widget = new ContinuonWidget();
      widget.render = jest.fn();
      
      widget.addStatusMessage('Test message');
      
      expect(widget.statusFeed.length).toBe(1);
      expect(widget.statusFeed[0].text).toBe('Test message');
      expect(widget.statusFeed[0]).toHaveProperty('timestamp');
      expect(widget.statusFeed[0]).toHaveProperty('id');
    });

    it('should limit status feed length', () => {
      const widget = new ContinuonWidget();
      widget.render = jest.fn();
      widget.maxStatusMessages = 2;
      
      // Add more messages than the limit
      for (let i = 0; i < 5; i++) {
        widget.addStatusMessage(`Message ${i}`);
      }
      
      expect(widget.statusFeed.length).toBeLessThanOrEqual(widget.maxStatusMessages * 2);
    });
  });

  describe('Version Management', () => {
    it('should get system version from continuum API', () => {
      const widget = new ContinuonWidget();
      
      const version = widget.getSystemVersion();
      
      expect(version).toBe('0.2.2360');
    });

    it('should fall back to default version when API unavailable', () => {
      const widget = new ContinuonWidget();
      mockWindow.continuum.version = undefined;
      
      const version = widget.getSystemVersion();
      
      expect(version).toBe('0.2.2177'); // fallback version
    });

    it('should log version change only when different', () => {
      const widget = new ContinuonWidget();
      const consoleSpy = jest.spyOn(console, 'log').mockImplementation();
      mockLocalStorage.getItem.mockReturnValue('0.2.2359');
      
      widget.logVersionIfChanged();
      
      expect(consoleSpy).toHaveBeenCalledWith('🚀 Continuum 0.2.2359 → 0.2.2360');
      expect(mockLocalStorage.setItem).toHaveBeenCalledWith('continuum-last-version', '0.2.2360');
      
      consoleSpy.mockRestore();
    });
  });

  describe('Rendering', () => {
    it('should render main content structure', () => {
      const widget = new ContinuonWidget();
      
      const content = widget.renderContent();
      
      expect(content).toContain('continuon-container');
      expect(content).toContain('continuon-orb');
      expect(content).toContain('status-feed');
      expect(content).toContain('continuum');
    });

    it('should render status messages', () => {
      const widget = new ContinuonWidget();
      widget.statusFeed = [
        { text: 'Test message 1', timestamp: Date.now(), id: '1' },
        { text: 'Test message 2', timestamp: Date.now(), id: '2' }
      ];
      
      const messagesHtml = widget.renderStatusMessages();
      
      expect(messagesHtml).toContain('Test message 1');
      expect(messagesHtml).toContain('Test message 2');
      expect(messagesHtml).toContain('status-message');
    });

    it('should show emotion in orb content', () => {
      const widget = new ContinuonWidget();
      widget.currentEmotion = '🎉';
      
      const content = widget.renderContent();
      
      expect(content).toContain('🎉');
      expect(content).toContain('data-emotion="🎉"');
    });
  });

  describe('Favicon Management', () => {
    it('should update favicon with status icon', () => {
      const widget = new ContinuonWidget();
      const mockFavicon = { href: '' };
      mockDocument.getElementById.mockReturnValue(mockFavicon);
      
      widget.updateFavicon('🟢');
      
      expect(mockFavicon.href).toContain('data:image/svg+xml');
      expect(mockFavicon.href).toContain('🟢');
    });

    it('should update title and favicon together', () => {
      const widget = new ContinuonWidget();
      widget.currentStatus = 'green';
      widget.updateFavicon = jest.fn();
      
      widget.updateTitleAndFavicon();
      
      expect(mockDocument.title).toBe('continuum');
      expect(widget.updateFavicon).toHaveBeenCalledWith('🟢');
    });

    it('should prioritize emotion over status for favicon', () => {
      const widget = new ContinuonWidget();
      widget.currentStatus = 'green';
      widget.currentEmotion = '😉';
      widget.updateFavicon = jest.fn();
      
      widget.updateTitleAndFavicon();
      
      expect(widget.updateFavicon).toHaveBeenCalledWith('😉');
    });
  });
});