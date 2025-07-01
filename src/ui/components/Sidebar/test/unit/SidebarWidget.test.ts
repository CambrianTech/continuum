/**
 * SidebarWidget Unit Tests  
 * Tests for the Sidebar widget component following MIDDLE-OUT methodology
 */

import { SidebarWidget } from '../../SidebarWidget.js';

describe('SidebarWidget', () => {
  let widget: SidebarWidget;

  beforeEach(() => {
    widget = new SidebarWidget();
  });

  afterEach(() => {
    if (widget.shadowRoot) {
      widget.remove();
    }
  });

  describe('initialization', () => {
    it('should create widget instance', () => {
      expect(widget).toBeInstanceOf(SidebarWidget);
      expect(widget.widgetName).toBe('SidebarWidget');
      expect(widget.widgetIcon).toBe('📋');
      expect(widget.widgetTitle).toBe('Sidebar');
    });

    it('should have CSS bundled', () => {
      const css = widget.getBundledCSS();
      expect(css).toContain('sidebar');
      expect(css.length).toBeGreaterThan(0);
    });
  });

  describe('content rendering', () => {
    it('should render sidebar structure', () => {
      const content = widget.renderContent();
      
      expect(content).toContain('sidebar-container');
      expect(content).toContain('sidebar-header');
      expect(content).toContain('sidebar-content');
    });

    it('should include navigation elements', () => {
      const content = widget.renderContent();
      
      expect(content).toContain('nav-item');
      expect(content).toContain('Dashboard');
      expect(content).toContain('Projects');
    });

    it('should include status section', () => {
      const content = widget.renderContent();
      
      expect(content).toContain('status-section');
      expect(content).toContain('WebSocket');
      expect(content).toContain('Commands');
    });
  });

  describe('version handling', () => {
    it('should get version from window global', () => {
      // Mock window global
      (window as any).__CONTINUUM_VERSION__ = '1.2.3';
      
      const version = widget['getVersion']();
      expect(version).toBe('1.2.3');
    });

    it('should fallback to unknown when no version available', () => {
      delete (window as any).__CONTINUUM_VERSION__;
      delete (window as any).continuum;
      
      const version = widget['getVersion']();
      expect(version).toBe('unknown');
    });
  });

  describe('navigation', () => {
    it('should handle navigation item clicks', () => {
      // Test navigation logic when implemented
      expect(widget['handleNavigation']).toBeDefined();
    });
  });

  describe('status updates', () => {
    it('should update connection status', () => {
      widget['updateConnectionStatus'](true);
      // Verify status indicator updates
      expect(widget['isConnected']).toBe(true);
    });

    it('should update command status', () => {
      widget['updateCommandStatus']('healthy');
      // Verify command status updates
      expect(widget['commandStatus']).toBe('healthy');
    });
  });
});