/**
 * Unit Tests for ScreenshotFeedback Module
 */

const { JSDOM } = require('jsdom');

// Mock DOM environment
const dom = new JSDOM('<!DOCTYPE html><html><head></head><body></body></html>');
global.document = dom.window.document;
global.window = dom.window;

const { 
  ScreenshotFeedback, 
  createScreenshotFeedback, 
  triggerScreenshotFeedback 
} = require('../../src/modules/ui/ScreenshotFeedback.js');

describe('ScreenshotFeedback', () => {
  let feedback;

  beforeEach(() => {
    // Clear DOM
    document.head.innerHTML = '';
    document.body.innerHTML = '';
    feedback = new ScreenshotFeedback();
  });

  afterEach(() => {
    if (feedback) {
      feedback.destroy();
    }
  });

  describe('Constructor', () => {
    test('should create instance with default options', () => {
      expect(feedback).toBeInstanceOf(ScreenshotFeedback);
      expect(feedback.options.duration).toBe(2000);
      expect(feedback.options.primaryColor).toBe('#00ff41');
      expect(feedback.isActive).toBe(false);
    });

    test('should accept custom options', () => {
      const customFeedback = new ScreenshotFeedback({
        duration: 1000,
        primaryColor: '#ff0000',
        borderWidth: 8
      });

      expect(customFeedback.options.duration).toBe(1000);
      expect(customFeedback.options.primaryColor).toBe('#ff0000');
      expect(customFeedback.options.borderWidth).toBe(8);
      
      customFeedback.destroy();
    });
  });

  describe('CSS Generation', () => {
    test('should generate valid CSS', () => {
      const css = feedback.generateCSS();
      
      expect(css).toContain('@keyframes screenshot-flash');
      expect(css).toContain('@keyframes screenshot-corner-flash');
      expect(css).toContain('.screenshot-feedback-rectangle');
      expect(css).toContain('.screenshot-corner');
      expect(css).toContain('#00ff41'); // Primary color
    });

    test('should use custom colors in CSS', () => {
      const customFeedback = new ScreenshotFeedback({
        primaryColor: '#ff0000',
        flashColor: '#00ff00'
      });

      const css = customFeedback.generateCSS();
      expect(css).toContain('#ff0000');
      expect(css).toContain('#00ff00');
      
      customFeedback.destroy();
    });
  });

  describe('hexToRgba', () => {
    test('should convert hex to rgba correctly', () => {
      expect(feedback.hexToRgba('#ff0000', 0.5)).toBe('rgba(255, 0, 0, 0.5)');
      expect(feedback.hexToRgba('#00ff41', 1)).toBe('rgba(0, 255, 65, 1)');
      expect(feedback.hexToRgba('#000000', 0)).toBe('rgba(0, 0, 0, 0)');
    });
  });

  describe('Style Injection', () => {
    test('should inject styles only once', () => {
      feedback.injectStyles();
      expect(document.getElementById('screenshot-feedback-styles')).toBeTruthy();
      expect(feedback.stylesInjected).toBe(true);

      // Second injection should not create duplicate
      feedback.injectStyles();
      const styles = document.querySelectorAll('#screenshot-feedback-styles');
      expect(styles.length).toBe(1);
    });
  });

  describe('Show Feedback', () => {
    test('should create main rectangle element', () => {
      feedback.show();
      
      const rectangle = document.querySelector('.screenshot-feedback-rectangle');
      expect(rectangle).toBeTruthy();
      expect(rectangle.getAttribute('data-screenshot-feedback')).toBe('main');
      expect(feedback.isActive).toBe(true);
    });

    test('should create corner indicators', (done) => {
      feedback.show();
      
      // Check that corners are created with delay
      setTimeout(() => {
        const corners = document.querySelectorAll('.screenshot-corner');
        expect(corners.length).toBeGreaterThan(0);
        done();
      }, 150); // After first corner should be created
    });

    test('should clean up existing feedback before showing new', () => {
      feedback.show();
      const firstRectangle = document.querySelector('.screenshot-feedback-rectangle');
      expect(firstRectangle).toBeTruthy();

      feedback.show(); // Show again
      const rectangles = document.querySelectorAll('.screenshot-feedback-rectangle');
      expect(rectangles.length).toBe(1); // Should only have one
    });
  });

  describe('Cleanup', () => {
    test('should remove all elements on cleanup', () => {
      feedback.show();
      expect(document.querySelector('.screenshot-feedback-rectangle')).toBeTruthy();

      feedback.cleanup();
      expect(document.querySelector('.screenshot-feedback-rectangle')).toBeFalsy();
      expect(feedback.isActive).toBe(false);
      expect(feedback.activeElements.size).toBe(0);
    });
  });

  describe('State Management', () => {
    test('should return correct state', () => {
      const initialState = feedback.getState();
      expect(initialState.isActive).toBe(false);
      expect(initialState.activeElementsCount).toBe(0);
      expect(initialState.stylesInjected).toBe(false);

      feedback.show();
      const activeState = feedback.getState();
      expect(activeState.isActive).toBe(true);
      expect(activeState.activeElementsCount).toBeGreaterThan(0);
      expect(activeState.stylesInjected).toBe(true);
    });
  });

  describe('Destroy', () => {
    test('should clean up and remove styles', () => {
      feedback.injectStyles();
      feedback.show();
      
      expect(document.getElementById('screenshot-feedback-styles')).toBeTruthy();
      expect(feedback.isActive).toBe(true);

      feedback.destroy();
      
      expect(feedback.isActive).toBe(false);
      expect(feedback.stylesInjected).toBe(false);
    });
  });

  describe('Error Handling', () => {
    test('should throw error in non-DOM environment', () => {
      const originalDocument = global.document;
      global.document = undefined;

      expect(() => {
        feedback.show();
      }).toThrow('ScreenshotFeedback requires a DOM environment');

      global.document = originalDocument;
    });
  });
});

describe('Factory Functions', () => {
  afterEach(() => {
    // Clean up DOM
    document.head.innerHTML = '';
    document.body.innerHTML = '';
  });

  test('createScreenshotFeedback should return new instance', () => {
    const instance = createScreenshotFeedback({ duration: 1000 });
    expect(instance).toBeInstanceOf(ScreenshotFeedback);
    expect(instance.options.duration).toBe(1000);
    instance.destroy();
  });

  test('triggerScreenshotFeedback should work with default instance', () => {
    triggerScreenshotFeedback();
    expect(document.querySelector('.screenshot-feedback-rectangle')).toBeTruthy();
  });

  test('triggerScreenshotFeedback should work with custom options', () => {
    const instance = triggerScreenshotFeedback({ duration: 500 });
    expect(instance).toBeInstanceOf(ScreenshotFeedback);
    expect(document.querySelector('.screenshot-feedback-rectangle')).toBeTruthy();
  });
});

describe('Integration Tests', () => {
  test('should handle rapid successive calls', () => {
    const feedback1 = new ScreenshotFeedback();
    const feedback2 = new ScreenshotFeedback();

    feedback1.show();
    feedback2.show();

    // Should not cause errors
    expect(document.querySelectorAll('.screenshot-feedback-rectangle').length).toBe(2);

    feedback1.destroy();
    feedback2.destroy();
  });

  test('should handle cleanup during animation', (done) => {
    const testFeedback = new ScreenshotFeedback();
    testFeedback.show();
    
    setTimeout(() => {
      testFeedback.cleanup(); // Cleanup during animation
      expect(testFeedback.isActive).toBe(false);
      testFeedback.destroy();
      done();
    }, 100);
  });
});

describe('Browser Environment Simulation', () => {
  test('should attach to window object in browser', () => {
    // The module should have attached itself to window in JSDOM
    expect(typeof window.ScreenshotFeedback).toBe('function');
    expect(typeof window.createScreenshotFeedback).toBe('function');
    expect(typeof window.triggerScreenshotFeedback).toBe('function');
    
    // Verify they're the same as the imported functions
    expect(window.ScreenshotFeedback).toBe(ScreenshotFeedback);
    expect(window.createScreenshotFeedback).toBe(createScreenshotFeedback);
    expect(window.triggerScreenshotFeedback).toBe(triggerScreenshotFeedback);
  });
});