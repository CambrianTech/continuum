/**
 * Screenshot Feedback Module
 * Provides visual feedback when screenshots are taken
 * 
 * Features:
 * - HAL 9000 style glowing rectangle
 * - Corner indicators with sequential animation
 * - Configurable animation timing and colors
 * - Auto-cleanup and memory management
 */

class ScreenshotFeedback {
  constructor(options = {}) {
    this.options = {
      duration: options.duration || 2000,
      borderWidth: options.borderWidth || 4,
      borderRadius: options.borderRadius || 12,
      zIndex: options.zIndex || 9999,
      primaryColor: options.primaryColor || '#00ff41',
      flashColor: options.flashColor || '#ffffff',
      brightColor: options.brightColor || '#00ff88',
      fadeColor: options.fadeColor || '#00aa33',
      ...options
    };
    
    this.isActive = false;
    this.activeElements = new Set();
    this.stylesInjected = false;
  }

  /**
   * Inject CSS styles for animations
   */
  injectStyles() {
    if (this.stylesInjected || typeof document === 'undefined') return;

    const styleId = 'screenshot-feedback-styles';
    if (document.getElementById(styleId)) return;

    const style = document.createElement('style');
    style.id = styleId;
    style.textContent = this.generateCSS();
    document.head.appendChild(style);
    this.stylesInjected = true;
  }

  /**
   * Generate CSS for animations
   */
  generateCSS() {
    const { primaryColor, flashColor, brightColor, fadeColor } = this.options;
    
    return `
      @keyframes screenshot-flash {
        0% {
          opacity: 0;
          transform: scale(1.05);
          border-color: ${flashColor};
          box-shadow: 
            0 0 50px rgba(255, 255, 255, 1),
            0 0 100px rgba(255, 255, 255, 0.8),
            0 0 150px rgba(255, 255, 255, 0.6),
            inset 0 0 50px rgba(255, 255, 255, 0.4);
        }
        15% {
          opacity: 1;
          transform: scale(1);
          border-color: ${brightColor};
          box-shadow: 
            0 0 30px ${this.hexToRgba(brightColor, 1)},
            0 0 60px ${this.hexToRgba(brightColor, 0.8)},
            0 0 90px ${this.hexToRgba(brightColor, 0.6)},
            inset 0 0 40px ${this.hexToRgba(brightColor, 0.3)};
        }
        50% {
          opacity: 0.8;
          transform: scale(1);
          border-color: ${primaryColor};
          box-shadow: 
            0 0 20px ${this.hexToRgba(primaryColor, 0.8)},
            0 0 40px ${this.hexToRgba(primaryColor, 0.6)},
            0 0 60px ${this.hexToRgba(primaryColor, 0.4)},
            inset 0 0 30px ${this.hexToRgba(primaryColor, 0.2)};
        }
        100% {
          opacity: 0;
          transform: scale(0.98);
          border-color: ${fadeColor};
          box-shadow: 
            0 0 5px ${this.hexToRgba(fadeColor, 0.3)},
            0 0 10px ${this.hexToRgba(fadeColor, 0.2)},
            0 0 15px ${this.hexToRgba(fadeColor, 0.1)},
            inset 0 0 10px ${this.hexToRgba(fadeColor, 0.1)};
        }
      }

      @keyframes screenshot-corner-flash {
        0%, 100% { 
          opacity: 0; 
          transform: scale(0.5); 
        }
        50% { 
          opacity: 1; 
          transform: scale(1); 
        }
      }

      .screenshot-feedback-rectangle {
        position: fixed;
        top: 0;
        left: 0;
        right: 0;
        bottom: 0;
        border: ${this.options.borderWidth}px solid ${primaryColor};
        border-radius: ${this.options.borderRadius}px;
        background: ${this.hexToRgba(primaryColor, 0.1)};
        pointer-events: none;
        z-index: ${this.options.zIndex};
        animation: screenshot-flash ${this.options.duration}ms ease-out forwards;
        backdrop-filter: blur(1px);
      }

      .screenshot-corner {
        position: fixed;
        width: 20px;
        height: 20px;
        border: 2px solid ${primaryColor};
        background: radial-gradient(circle, ${primaryColor}, transparent);
        animation: screenshot-corner-flash ${this.options.duration}ms ease-in-out;
        pointer-events: none;
        z-index: ${this.options.zIndex + 1};
      }
    `;
  }

  /**
   * Convert hex color to rgba
   */
  hexToRgba(hex, alpha) {
    const r = parseInt(hex.slice(1, 3), 16);
    const g = parseInt(hex.slice(3, 5), 16);
    const b = parseInt(hex.slice(5, 7), 16);
    return `rgba(${r}, ${g}, ${b}, ${alpha})`;
  }

  /**
   * Show screenshot feedback
   */
  show() {
    if (typeof document === 'undefined') {
      throw new Error('ScreenshotFeedback requires a DOM environment');
    }

    if (this.isActive) {
      this.cleanup(); // Clean up existing feedback
    }

    this.injectStyles();
    this.isActive = true;

    // Create main rectangle
    const rectangle = this.createRectangle();
    this.activeElements.add(rectangle);
    document.body.appendChild(rectangle);

    // Create corner indicators
    this.createCornerIndicators();

    // Auto-cleanup after animation
    setTimeout(() => {
      this.cleanup();
    }, this.options.duration);

    return this;
  }

  /**
   * Create main feedback rectangle
   */
  createRectangle() {
    const rectangle = document.createElement('div');
    rectangle.className = 'screenshot-feedback-rectangle';
    rectangle.setAttribute('data-screenshot-feedback', 'main');
    return rectangle;
  }

  /**
   * Create corner indicators
   */
  createCornerIndicators() {
    const corners = [
      { top: '10px', left: '10px' },
      { top: '10px', right: '10px' },
      { bottom: '10px', left: '10px' },
      { bottom: '10px', right: '10px' }
    ];

    corners.forEach((corner, index) => {
      setTimeout(() => {
        const cornerDiv = document.createElement('div');
        cornerDiv.className = 'screenshot-corner';
        cornerDiv.setAttribute('data-screenshot-feedback', 'corner');
        Object.assign(cornerDiv.style, corner);
        
        this.activeElements.add(cornerDiv);
        document.body.appendChild(cornerDiv);

        // Remove corner after animation
        setTimeout(() => {
          this.activeElements.delete(cornerDiv);
          if (cornerDiv.parentNode) {
            cornerDiv.parentNode.removeChild(cornerDiv);
          }
        }, this.options.duration);
      }, index * 100);
    });
  }

  /**
   * Clean up all active elements
   */
  cleanup() {
    this.activeElements.forEach(element => {
      if (element.parentNode) {
        element.parentNode.removeChild(element);
      }
    });
    this.activeElements.clear();
    this.isActive = false;
  }

  /**
   * Get current state
   */
  getState() {
    return {
      isActive: this.isActive,
      activeElementsCount: this.activeElements.size,
      stylesInjected: this.stylesInjected,
      options: { ...this.options }
    };
  }

  /**
   * Destroy instance and clean up
   */
  destroy() {
    this.cleanup();
    
    // Remove styles if no other instances
    if (this.stylesInjected && typeof document !== 'undefined') {
      const style = document.getElementById('screenshot-feedback-styles');
      if (style) {
        style.remove();
      }
    }
    
    this.stylesInjected = false;
  }
}

// Factory function for easy instantiation
function createScreenshotFeedback(options) {
  return new ScreenshotFeedback(options);
}

// Default instance for global use
let defaultInstance = null;

function getDefaultInstance() {
  if (!defaultInstance) {
    defaultInstance = new ScreenshotFeedback();
  }
  return defaultInstance;
}

// Global trigger function
function triggerScreenshotFeedback(options) {
  if (options) {
    return createScreenshotFeedback(options).show();
  }
  return getDefaultInstance().show();
}

// Export for different environments
if (typeof module !== 'undefined' && module.exports) {
  // Node.js
  module.exports = {
    ScreenshotFeedback,
    createScreenshotFeedback,
    triggerScreenshotFeedback
  };
} else if (typeof window !== 'undefined') {
  // Browser
  window.ScreenshotFeedback = ScreenshotFeedback;
  window.createScreenshotFeedback = createScreenshotFeedback;
  window.triggerScreenshotFeedback = triggerScreenshotFeedback;
}

// AMD/RequireJS support
if (typeof define === 'function' && define.amd) {
  define([], function() {
    return {
      ScreenshotFeedback,
      createScreenshotFeedback,
      triggerScreenshotFeedback
    };
  });
}