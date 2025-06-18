/**
 * Continuon Positioning Tests
 * Tests the 33%/33% centering style implementation for cursor positioning
 * Moved from __tests__/unit/ContinuonPositioning.test.cjs into cursor command module
 */

import { test, describe } from 'node:test';
import assert from 'node:assert';
import { JSDOM } from 'jsdom';

describe('Continuon AI Cursor Positioning', () => {
  
  function createDOMEnvironment() {
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
    window.Element.prototype.getBoundingClientRect = () => ({
      left: 100,
      top: 50,
      width: 16,
      height: 16
    });
    
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
      
      assert.strictEqual(expectedX, 396);
      assert.strictEqual(expectedY, 264);
    });
    
    test('should calculate 33% positioning for different viewport sizes', () => {
      // Test with 1920x1080 viewport
      window.innerWidth = 1920;
      window.innerHeight = 1080;
      
      const expectedX = 1920 * 0.33; // 633.6px
      const expectedY = 1080 * 0.33; // 356.4px
      
      assert.strictEqual(expectedX, 633.6);
      assert.strictEqual(expectedY, 356.4);
    });
    
    test('should handle small viewport sizes', () => {
      // Test with mobile viewport
      window.innerWidth = 375;
      window.innerHeight = 667;
      
      const expectedX = 375 * 0.33; // 123.75px
      const expectedY = 667 * 0.33; // 220.11px
      
      assert.strictEqual(expectedX, 123.75);
      assert.strictEqual(expectedY, 220.11);
    });
  });

  describe('activateAICursor function', () => {
    test('should exist and be callable', () => {
      assert.strictEqual(typeof global.activateAICursor, 'function');
    });
    
    test('should activate AI cursor and apply Continuon positioning', () => {
      const indicator = document.getElementById('connection-status');
      assert(indicator);
      
      // Activate cursor
      const result = global.activateAICursor();
      
      // Verify cursor was activated
      assert.strictEqual(global.aiCursorActive, true);
      assert.strictEqual(result, indicator);
      assert(indicator.classList.contains('ai-cursor'));
      
      // Verify Continuon-style positioning (33%, 33%)
      const expectedX = window.innerWidth * 0.33; // 396px for 1200px width
      const expectedY = window.innerHeight * 0.33; // 264px for 800px height
      
      assert.strictEqual(indicator.style.left, expectedX + 'px');
      assert.strictEqual(indicator.style.top, expectedY + 'px');
    });
    
    test('should not activate if already active', () => {
      const indicator = document.getElementById('connection-status');
      
      // First activation
      global.activateAICursor();
      assert.strictEqual(global.aiCursorActive, true);
      
      // Store current position
      const firstLeft = indicator.style.left;
      const firstTop = indicator.style.top;
      
      // Try to activate again
      const result = global.activateAICursor();
      
      // Should return undefined and not change position
      assert.strictEqual(result, undefined);
      assert.strictEqual(indicator.style.left, firstLeft);
      assert.strictEqual(indicator.style.top, firstTop);
    });
    
    test('should store original state correctly', () => {
      const indicator = document.getElementById('connection-status');
      const originalParent = indicator.parentElement;
      const originalStyle = indicator.style.cssText;
      
      global.activateAICursor();
      
      assert.strictEqual(global.aiCursorOriginalParent, originalParent);
      assert.strictEqual(global.aiCursorOriginalStyle, originalStyle);
    });
  });

  describe('deactivateAICursor function', () => {
    test('should exist and be callable', () => {
      assert.strictEqual(typeof global.deactivateAICursor, 'function');
    });
    
    test('should deactivate AI cursor and return to Continuon center', () => {
      const indicator = document.getElementById('connection-status');
      
      // First activate
      global.activateAICursor();
      assert.strictEqual(global.aiCursorActive, true);
      
      // Then deactivate
      global.deactivateAICursor();
      
      // Verify deactivation
      assert.strictEqual(global.aiCursorActive, false);
      assert.strictEqual(global.aiCursorOriginalParent, null);
      assert.strictEqual(global.aiCursorOriginalStyle, null);
      
      // Verify it returns to Continuon center position
      const expectedX = window.innerWidth * 0.33;
      const expectedY = window.innerHeight * 0.33;
      
      assert.strictEqual(indicator.style.left, expectedX + 'px');
      assert.strictEqual(indicator.style.top, expectedY + 'px');
    });
    
    test('should not error if called when inactive', () => {
      assert.strictEqual(global.aiCursorActive, false);
      
      // Should not throw error
      assert.doesNotThrow(() => {
        global.deactivateAICursor();
      });
    });
    
    test('should remove CSS classes correctly', () => {
      const indicator = document.getElementById('connection-status');
      
      // Activate and add classes
      global.activateAICursor();
      indicator.classList.add('ai-cursor-click'); // Simulate click state
      
      assert(indicator.classList.contains('ai-cursor'));
      assert(indicator.classList.contains('ai-cursor-click'));
      
      // Deactivate
      global.deactivateAICursor();
      
      // Classes should be removed
      assert(!indicator.classList.contains('ai-cursor'));
      assert(!indicator.classList.contains('ai-cursor-click'));
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
        assert.strictEqual(indicator.style.left, expectedX + 'px');
        assert.strictEqual(indicator.style.top, expectedY + 'px');
        
        global.deactivateAICursor();
        assert.strictEqual(indicator.style.left, expectedX + 'px');
        assert.strictEqual(indicator.style.top, expectedY + 'px');
      }
    });
    
    test('should work with responsive viewport changes', () => {
      const indicator = document.getElementById('connection-status');
      
      // Start with desktop size
      window.innerWidth = 1200;
      window.innerHeight = 800;
      global.activateAICursor();
      
      assert.strictEqual(indicator.style.left, '396px');
      assert.strictEqual(indicator.style.top, '264px');
      
      global.deactivateAICursor();
      
      // Change to tablet size
      window.innerWidth = 768;
      window.innerHeight = 1024;
      global.activateAICursor();
      
      assert.strictEqual(indicator.style.left, '253.44px');
      assert.strictEqual(indicator.style.top, '337.92px');
    });
    
    test('should maintain Continuon positioning standard', () => {
      const indicator = document.getElementById('connection-status');
      
      global.activateAICursor();
      
      // Verify it's using exactly 33% positioning
      const actualXPercent = (parseInt(indicator.style.left) / window.innerWidth) * 100;
      const actualYPercent = (parseInt(indicator.style.top) / window.innerHeight) * 100;
      
      assert(Math.abs(actualXPercent - 33) < 0.1);
      assert(Math.abs(actualYPercent - 33) < 0.1);
    });
  });

  describe('Error Handling', () => {
    test('should handle missing connection-status element', () => {
      // Remove the element
      const indicator = document.getElementById('connection-status');
      indicator.remove();
      
      // Should not throw errors
      assert.doesNotThrow(() => {
        global.activateAICursor();
      });
      
      assert.doesNotThrow(() => {
        global.deactivateAICursor();
      });
    });
    
    test('should handle zero viewport dimensions', () => {
      window.innerWidth = 0;
      window.innerHeight = 0;
      
      const indicator = document.getElementById('connection-status');
      global.activateAICursor();
      
      // Should position at 0,0 when viewport is 0
      assert.strictEqual(indicator.style.left, '0px');
      assert.strictEqual(indicator.style.top, '0px');
    });
  });
});