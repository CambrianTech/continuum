/**
 * Continuon Animator
 * Handles the green cursor animations and ROI highlighting for screenshots
 */

export class ContinuonAnimator {
  constructor() {
    this.continuonElement = null;
    this.roiElement = null;
    this.trailElements = [];
    this.isAnimating = false;
    this.ringElement = null;
    
    this.loadCSS();
    this.initializeContinuon();
  }

  /**
   * Load the continuon animation CSS
   */
  loadCSS() {
    const cssId = 'continuon-animations-css';
    if (document.getElementById(cssId)) return;

    const link = document.createElement('link');
    link.id = cssId;
    link.rel = 'stylesheet';
    link.type = 'text/css';
    link.href = '/src/commands/core/screenshot/ContinuonAnimations.css';
    document.head.appendChild(link);
  }

  /**
   * Initialize the continuon cursor element
   */
  initializeContinuon() {
    // Find the ring element (top-left corner)
    this.ringElement = document.querySelector('.continuum-ring') || 
                      document.querySelector('#connection-status') ||
                      document.querySelector('.version-badge');
    
    if (!this.ringElement) {
      console.warn('🟢 Continuon: Ring element not found, creating default position');
      this.createDefaultRing();
    }

    // Create continuon cursor element
    this.continuonElement = document.createElement('div');
    this.continuonElement.className = 'continuon-cursor';
    this.continuonElement.style.display = 'none';
    document.body.appendChild(this.continuonElement);
  }

  /**
   * Create a default ring position if none found
   */
  createDefaultRing() {
    this.ringElement = document.createElement('div');
    this.ringElement.className = 'continuum-ring default';
    this.ringElement.style.cssText = `
      position: fixed;
      top: 16px;
      left: 16px;
      width: 16px;
      height: 16px;
      background: #00ff88;
      border-radius: 50%;
      z-index: 10000;
    `;
    document.body.appendChild(this.ringElement);
  }

  /**
   * Start screenshot animation sequence
   */
  async startScreenshotAnimation(params) {
    if (this.isAnimating) return;
    
    const { selector, animation, roi, continuonAnimation } = params;
    
    if (!continuonAnimation?.enabled) {
      // No animation requested
      return this.captureWithoutAnimation(selector);
    }

    this.isAnimating = true;
    
    try {
      // Step 1: Activate ring
      await this.activateRing();
      
      // Step 2: Emerge continuon from ring
      await this.emergeContinuon();
      
      // Step 3: Move to target element
      const targetRect = await this.moveToTarget(selector);
      
      // Step 4: Draw ROI if requested
      if (roi && targetRect) {
        await this.drawROI(targetRect, animation);
      }
      
      // Step 5: Flash and capture
      await this.flashAndCapture();
      
      // Step 6: Return to ring
      await this.returnToRing();
      
    } finally {
      this.isAnimating = false;
    }
  }

  /**
   * Activate the ring with pulse animation
   */
  async activateRing() {
    if (!this.ringElement) return;
    
    this.ringElement.classList.add('continuon-active');
    
    return new Promise(resolve => {
      setTimeout(() => {
        resolve();
      }, 600);
    });
  }

  /**
   * Make continuon emerge from the ring
   */
  async emergeContinuon() {
    if (!this.continuonElement || !this.ringElement) return;
    
    const ringRect = this.ringElement.getBoundingClientRect();
    
    // Position continuon at ring center
    this.continuonElement.style.left = (ringRect.left + ringRect.width / 2 - 8) + 'px';
    this.continuonElement.style.top = (ringRect.top + ringRect.height / 2 - 8) + 'px';
    this.continuonElement.style.display = 'block';
    this.continuonElement.classList.add('active');
    
    return new Promise(resolve => {
      setTimeout(resolve, 300);
    });
  }

  /**
   * Move continuon to target element with trail
   */
  async moveToTarget(selector) {
    const targetElement = document.querySelector(selector);
    if (!targetElement) {
      console.warn(`🟢 Continuon: Target element not found: ${selector}`);
      return null;
    }

    const targetRect = targetElement.getBoundingClientRect();
    const centerX = targetRect.left + targetRect.width / 2;
    const centerY = targetRect.top + targetRect.height / 2;

    // Create movement trail
    this.createMovementTrail(
      this.continuonElement.offsetLeft + 8,
      this.continuonElement.offsetTop + 8,
      centerX,
      centerY
    );

    // Animate continuon to target
    this.continuonElement.style.transition = 'all 1.2s cubic-bezier(0.25, 0.46, 0.45, 0.94)';
    this.continuonElement.style.left = (centerX - 8) + 'px';
    this.continuonElement.style.top = (centerY - 8) + 'px';

    return new Promise(resolve => {
      setTimeout(() => {
        resolve(targetRect);
      }, 1200);
    });
  }

  /**
   * Create movement trail from continuon to target
   */
  createMovementTrail(startX, startY, endX, endY) {
    const distance = Math.sqrt(Math.pow(endX - startX, 2) + Math.pow(endY - startY, 2));
    const steps = Math.floor(distance / 20); // Trail point every 20px
    
    for (let i = 0; i < steps; i++) {
      const progress = i / steps;
      const x = startX + (endX - startX) * progress;
      const y = startY + (endY - startY) * progress;
      
      setTimeout(() => {
        this.createTrailPoint(x, y);
      }, i * 50); // Stagger trail points
    }
  }

  /**
   * Create a single trail point
   */
  createTrailPoint(x, y) {
    const trail = document.createElement('div');
    trail.className = 'continuon-trail';
    trail.style.left = x + 'px';
    trail.style.top = y + 'px';
    document.body.appendChild(trail);
    
    // Clean up after animation
    setTimeout(() => {
      if (trail.parentNode) {
        trail.parentNode.removeChild(trail);
      }
    }, 1000);
  }

  /**
   * Draw ROI around target element
   */
  async drawROI(targetRect, animationType) {
    if (this.roiElement) {
      this.roiElement.remove();
    }

    this.roiElement = document.createElement('div');
    this.roiElement.className = `screenshot-roi ${animationType}`;
    
    // Position ROI around target with padding
    const padding = 8;
    this.roiElement.style.left = (targetRect.left - padding) + 'px';
    this.roiElement.style.top = (targetRect.top - padding) + 'px';
    this.roiElement.style.width = (targetRect.width + padding * 2) + 'px';
    this.roiElement.style.height = (targetRect.height + padding * 2) + 'px';
    
    document.body.appendChild(this.roiElement);
    
    // Add drawing state to continuon
    this.continuonElement.classList.add('drawing');
    
    return new Promise(resolve => {
      setTimeout(() => {
        this.continuonElement.classList.remove('drawing');
        resolve();
      }, animationType === 'animated' ? 2000 : 1500);
    });
  }

  /**
   * Flash screen and trigger capture
   */
  async flashAndCapture() {
    const flash = document.createElement('div');
    flash.className = 'screenshot-flash';
    document.body.appendChild(flash);
    
    // Clean up flash
    setTimeout(() => {
      if (flash.parentNode) {
        flash.parentNode.removeChild(flash);
      }
    }, 300);
    
    return new Promise(resolve => {
      setTimeout(resolve, 500);
    });
  }

  /**
   * Return continuon to ring
   */
  async returnToRing() {
    if (!this.continuonElement || !this.ringElement) return;
    
    const ringRect = this.ringElement.getBoundingClientRect();
    
    this.continuonElement.classList.add('returning');
    this.continuonElement.style.transition = 'all 1.5s cubic-bezier(0.68, -0.55, 0.265, 1.55)';
    this.continuonElement.style.left = (ringRect.left + ringRect.width / 2 - 8) + 'px';
    this.continuonElement.style.top = (ringRect.top + ringRect.height / 2 - 8) + 'px';
    
    return new Promise(resolve => {
      setTimeout(() => {
        this.continuonElement.style.display = 'none';
        this.continuonElement.classList.remove('active', 'returning');
        this.ringElement.classList.remove('continuon-active');
        
        // Clean up ROI
        if (this.roiElement) {
          this.roiElement.remove();
          this.roiElement = null;
        }
        
        resolve();
      }, 1500);
    });
  }

  /**
   * Capture screenshot without animation
   */
  async captureWithoutAnimation(selector) {
    // Just flash for feedback
    const flash = document.createElement('div');
    flash.className = 'screenshot-flash';
    document.body.appendChild(flash);
    
    setTimeout(() => {
      if (flash.parentNode) {
        flash.parentNode.removeChild(flash);
      }
    }, 300);
  }

  /**
   * Clean up all animation elements
   */
  cleanup() {
    if (this.continuonElement) {
      this.continuonElement.remove();
    }
    if (this.roiElement) {
      this.roiElement.remove();
    }
    this.trailElements.forEach(trail => trail.remove());
    this.trailElements = [];
  }
}

// Global instance
window.continuonAnimator = window.continuonAnimator || new ContinuonAnimator();

export default window.continuonAnimator;