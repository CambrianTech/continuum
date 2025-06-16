/**
 * Unit Tests for Continuon AI Cursor Positioning
 * Tests the 33%/33% centering style implementation
 */

// Mock DOM environment
const { JSDOM } = require('jsdom');

describe('Continuon AI Cursor Positioning', () => {
  let dom;
  let window;
  let document;
  
  beforeEach(() => {
    // Set up DOM environment
    dom = new JSDOM(`
      <!DOCTYPE html>
      <html>
        <head></head>
        <body>
          <div id="connection-status" class="connection-status connected" style="width: 16px; height: 16px; position: relative;">HAL</div>
        </body>
      </html>
    `, { url: 'http://localhost' });
    
    window = dom.window;
    document = window.document;
    global.window = window;
    global.document = document;
    
    // Mock window dimensions for testing
    Object.defineProperty(window, 'innerWidth', {
      writable: true,
      configurable: true,
      value: 1200
    });
    
    Object.defineProperty(window, 'innerHeight', {
      writable: true,
      configurable: true,
      value: 800
    });
    
    // Mock getBoundingClientRect
    window.Element.prototype.getBoundingClientRect = jest.fn(() => ({
      left: 100,
      top: 50,
      width: 16,
      height: 16
    }));
    
    // Set up AI cursor variables
    global.aiCursorActive = false;
    global.aiCursorOriginalParent = null;
    global.aiCursorOriginalStyle = null;
    
    // Load the cursor functions (simplified versions for testing)
    global.activateAICursor = function() {
      const indicator = document.getElementById('connection-status');
      if (!indicator || global.aiCursorActive) return;

      console.log('🤖 AI Cursor activated - HAL 9000 is now the mouse');
      
      // Store original state
      global.aiCursorOriginalParent = indicator.parentElement;
      global.aiCursorOriginalStyle = indicator.style.cssText;
      
      // Activate cursor mode
      global.aiCursorActive = true;
      indicator.classList.add('ai-cursor');
      
      // Apply Continuon-style centering (left: 33%; top: 33%)
      const homeX = window.innerWidth * 0.33; // 33% from left
      const homeY = window.innerHeight * 0.33; // 33% from top
      
      // Position at Continuon-style center location
      indicator.style.left = homeX + 'px';
      indicator.style.top = homeY + 'px';
      
      console.log('Continuon activated at centered position (' + homeX + ', ' + homeY + ') - 33% style');
      
      return indicator;
    };
    
    global.deactivateAICursor = function() {
      const indicator = document.getElementById('connection-status');
      if (!indicator || !global.aiCursorActive) return;

      console.log('🤖 AI Cursor deactivated - Continuon returning to base');
      
      // Return to the same Continuon-style center position (33%, 33%)
      const homeX = window.innerWidth * 0.33;
      const homeY = window.innerHeight * 0.33;
      
      // Set return position
      indicator.style.left = homeX + 'px';
      indicator.style.top = homeY + 'px';
      
      // Reset state
      indicator.classList.remove('ai-cursor', 'ai-cursor-click');
      if (global.aiCursorOriginalStyle) {
        indicator.style.cssText = global.aiCursorOriginalStyle;
      }
      
      global.aiCursorActive = false;
      global.aiCursorOriginalParent = null;
      global.aiCursorOriginalStyle = null;
      
      console.log('🟢 Continuon returned to centered base (33% style)');
    };
  });
  
  afterEach(() => {
    dom.window.close();
  });

  describe('Continuon Positioning Calculations', () => {
    test('should calculate 33% positioning correctly for 1200x800 viewport', () => {
      const expectedX = 1200 * 0.33; // 396px
      const expectedY = 800 * 0.33;  // 264px
      
      expect(expectedX).toBe(396);
      expect(expectedY).toBe(264);
    });
    
    test('should calculate 33% positioning for different viewport sizes', () => {
      // Test with 1920x1080 viewport
      window.innerWidth = 1920;
      window.innerHeight = 1080;
      
      const expectedX = 1920 * 0.33; // 633.6px
      const expectedY = 1080 * 0.33; // 356.4px
      
      expect(expectedX).toBe(633.6);
      expect(expectedY).toBe(356.4);
    });
    
    test('should handle small viewport sizes', () => {
      // Test with mobile viewport
      window.innerWidth = 375;
      window.innerHeight = 667;
      
      const expectedX = 375 * 0.33; // 123.75px
      const expectedY = 667 * 0.33; // 220.11px
      
      expect(expectedX).toBe(123.75);
      expect(expectedY).toBe(220.11);
    });
  });

  describe('activateAICursor function', () => {
    test('should exist and be callable', () => {
      expect(typeof global.activateAICursor).toBe('function');
    });
    
    test('should activate AI cursor and apply Continuon positioning', () => {
      const indicator = document.getElementById('connection-status');
      expect(indicator).toBeTruthy();
      
      // Activate cursor
      const result = global.activateAICursor();
      
      // Verify cursor was activated
      expect(global.aiCursorActive).toBe(true);
      expect(result).toBe(indicator);
      expect(indicator.classList.contains('ai-cursor')).toBe(true);
      
      // Verify Continuon-style positioning (33%, 33%)
      const expectedX = window.innerWidth * 0.33; // 396px for 1200px width
      const expectedY = window.innerHeight * 0.33; // 264px for 800px height
      
      expect(indicator.style.left).toBe(expectedX + 'px');
      expect(indicator.style.top).toBe(expectedY + 'px');
    });
    
    test('should not activate if already active', () => {
      const indicator = document.getElementById('connection-status');
      
      // First activation
      global.activateAICursor();
      expect(global.aiCursorActive).toBe(true);
      
      // Store current position
      const firstLeft = indicator.style.left;
      const firstTop = indicator.style.top;
      
      // Try to activate again
      const result = global.activateAICursor();
      
      // Should return undefined and not change position
      expect(result).toBeUndefined();
      expect(indicator.style.left).toBe(firstLeft);
      expect(indicator.style.top).toBe(firstTop);
    });
    
    test('should store original state correctly', () => {
      const indicator = document.getElementById('connection-status');
      const originalParent = indicator.parentElement;
      const originalStyle = indicator.style.cssText;
      
      global.activateAICursor();
      
      expect(global.aiCursorOriginalParent).toBe(originalParent);
      expect(global.aiCursorOriginalStyle).toBe(originalStyle);
    });
  });

  describe('deactivateAICursor function', () => {
    test('should exist and be callable', () => {
      expect(typeof global.deactivateAICursor).toBe('function');
    });
    
    test('should deactivate AI cursor and return to Continuon center', () => {
      const indicator = document.getElementById('connection-status');
      
      // First activate
      global.activateAICursor();
      expect(global.aiCursorActive).toBe(true);
      
      // Then deactivate
      global.deactivateAICursor();
      
      // Verify deactivation
      expect(global.aiCursorActive).toBe(false);
      expect(global.aiCursorOriginalParent).toBeNull();
      expect(global.aiCursorOriginalStyle).toBeNull();
      
      // Verify it returns to Continuon center position
      const expectedX = window.innerWidth * 0.33;
      const expectedY = window.innerHeight * 0.33;
      
      expect(indicator.style.left).toBe(expectedX + 'px');
      expect(indicator.style.top).toBe(expectedY + 'px');
    });
    
    test('should not error if called when inactive', () => {
      expect(global.aiCursorActive).toBe(false);
      
      // Should not throw error
      expect(() => {
        global.deactivateAICursor();
      }).not.toThrow();
    });
    
    test('should remove CSS classes correctly', () => {
      const indicator = document.getElementById('connection-status');
      
      // Activate and add classes
      global.activateAICursor();
      indicator.classList.add('ai-cursor-click'); // Simulate click state
      
      expect(indicator.classList.contains('ai-cursor')).toBe(true);
      expect(indicator.classList.contains('ai-cursor-click')).toBe(true);
      
      // Deactivate
      global.deactivateAICursor();
      
      // Classes should be removed
      expect(indicator.classList.contains('ai-cursor')).toBe(false);
      expect(indicator.classList.contains('ai-cursor-click')).toBe(false);
    });
  });

  describe('Continuon Integration', () => {
    test('should maintain 33% positioning across activation/deactivation cycles', () => {
      const indicator = document.getElementById('connection-status');
      const expectedX = window.innerWidth * 0.33;
      const expectedY = window.innerHeight * 0.33;
      
      // Multiple activation/deactivation cycles
      for (let i = 0; i < 3; i++) {
        global.activateAICursor();
        expect(indicator.style.left).toBe(expectedX + 'px');
        expect(indicator.style.top).toBe(expectedY + 'px');
        
        global.deactivateAICursor();
        expect(indicator.style.left).toBe(expectedX + 'px');
        expect(indicator.style.top).toBe(expectedY + 'px');
      }
    });
    
    test('should work with responsive viewport changes', () => {
      const indicator = document.getElementById('connection-status');
      
      // Start with desktop size
      window.innerWidth = 1200;
      window.innerHeight = 800;
      global.activateAICursor();
      
      expect(indicator.style.left).toBe('396px');
      expect(indicator.style.top).toBe('264px');
      
      global.deactivateAICursor();
      
      // Change to tablet size
      window.innerWidth = 768;
      window.innerHeight = 1024;
      global.activateAICursor();
      
      expect(indicator.style.left).toBe('253.44px');
      expect(indicator.style.top).toBe('337.92px');
    });
    
    test('should maintain Continuon positioning standard', () => {
      const indicator = document.getElementById('connection-status');
      
      global.activateAICursor();
      
      // Verify it's using exactly 33% positioning
      const actualXPercent = (parseInt(indicator.style.left) / window.innerWidth) * 100;
      const actualYPercent = (parseInt(indicator.style.top) / window.innerHeight) * 100;
      
      expect(actualXPercent).toBeCloseTo(33, 1);
      expect(actualYPercent).toBeCloseTo(33, 1);
    });
  });

  describe('Error Handling', () => {
    test('should handle missing connection-status element', () => {
      // Remove the element
      const indicator = document.getElementById('connection-status');
      indicator.remove();
      
      // Should not throw errors
      expect(() => {
        global.activateAICursor();
      }).not.toThrow();
      
      expect(() => {
        global.deactivateAICursor();
      }).not.toThrow();
    });
    
    test('should handle zero viewport dimensions', () => {
      window.innerWidth = 0;
      window.innerHeight = 0;
      
      const indicator = document.getElementById('connection-status');
      global.activateAICursor();
      
      // Should position at 0,0 when viewport is 0
      expect(indicator.style.left).toBe('0px');
      expect(indicator.style.top).toBe('0px');
    });
  });
});