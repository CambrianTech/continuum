/**
 * Graphics Renderer - Modular graphics system for continuon animations
 * Supports CSS, WebGL, Three.js backends through plugin architecture
 */

export class GraphicsRenderer {
  constructor(backend = 'css') {
    this.backend = backend;
    this.renderer = null;
    this.capabilities = {};
    this.systemState = {
      color: '#00ff88',
      performance: 'auto'
    };
    
    this.initializeRenderer();
  }

  /**
   * Initialize the appropriate graphics backend
   */
  async initializeRenderer() {
    switch (this.backend) {
      case 'css':
        this.renderer = new CSSRenderer();
        break;
      case 'webgl':
        this.renderer = await this.loadWebGLRenderer();
        break;
      case 'threejs':
        this.renderer = await this.loadThreeJSRenderer();
        break;
      case 'auto':
        this.renderer = await this.detectBestRenderer();
        break;
      default:
        console.warn(`Unknown graphics backend: ${this.backend}, falling back to CSS`);
        this.renderer = new CSSRenderer();
    }
    
    this.capabilities = this.renderer.getCapabilities();
    console.log(`🎨 Graphics renderer initialized: ${this.renderer.getName()}`);
  }

  /**
   * Detect the best available renderer based on system capabilities
   */
  async detectBestRenderer() {
    // Check for WebGL support
    if (this.hasWebGLSupport()) {
      try {
        return await this.loadWebGLRenderer();
      } catch (error) {
        console.log('WebGL renderer failed to load, falling back to CSS');
      }
    }
    
    // Fallback to CSS
    return new CSSRenderer();
  }

  /**
   * Check if WebGL is supported
   */
  hasWebGLSupport() {
    try {
      const canvas = document.createElement('canvas');
      return !!(window.WebGLRenderingContext && 
               (canvas.getContext('webgl') || canvas.getContext('experimental-webgl')));
    } catch (e) {
      return false;
    }
  }

  /**
   * Load WebGL renderer module
   */
  async loadWebGLRenderer() {
    try {
      const { WebGLRenderer } = await import('./WebGLRenderer.js');
      return new WebGLRenderer();
    } catch (error) {
      throw new Error(`Failed to load WebGL renderer: ${error.message}`);
    }
  }

  /**
   * Load Three.js renderer module
   */
  async loadThreeJSRenderer() {
    try {
      const { ThreeJSRenderer } = await import('./ThreeJSRenderer.js');
      return new ThreeJSRenderer();
    } catch (error) {
      throw new Error(`Failed to load Three.js renderer: ${error.message}`);
    }
  }

  /**
   * Update global system state that affects all continuon rendering
   */
  updateSystemState(state) {
    const stateColors = {
      normal: '#00ff88',    // Continuum green
      error: '#ff3333',     // Red for errors
      warning: '#ffaa00',   // Orange for warnings  
      processing: '#3399ff', // Blue for processing
      success: '#00ff00',   // Bright green for success
      disabled: '#666666'   // Gray for disabled
    };

    if (state && stateColors[state]) {
      this.systemState.color = stateColors[state];
      this.systemState.state = state;
      
      // Update renderer with new global state
      if (this.renderer) {
        this.renderer.updateGlobalState(this.systemState);
      }
      
      console.log(`🎨 Continuon state updated: ${state} (${this.systemState.color})`);
    }
  }

  /**
   * Render continuon with movement animation
   */
  async renderContinuonMovement(fromPos, toPos, options = {}) {
    if (!this.renderer) {
      throw new Error('Graphics renderer not initialized');
    }

    const renderOptions = {
      ...options,
      color: this.systemState.color,
      state: this.systemState.state,
      backend: this.backend
    };

    return await this.renderer.renderMovement(fromPos, toPos, renderOptions);
  }

  /**
   * Render ROI highlighting
   */
  async renderROI(targetRect, animationType, options = {}) {
    if (!this.renderer) {
      throw new Error('Graphics renderer not initialized');
    }

    const renderOptions = {
      ...options,
      color: this.systemState.color,
      state: this.systemState.state,
      animation: animationType
    };

    return await this.renderer.renderROI(targetRect, renderOptions);
  }

  /**
   * Render trail effects
   */
  renderTrail(positions, options = {}) {
    if (!this.renderer) return;

    const renderOptions = {
      ...options,
      color: this.systemState.color,
      opacity: 0.6
    };

    return this.renderer.renderTrail(positions, renderOptions);
  }

  /**
   * Get current renderer capabilities
   */
  getCapabilities() {
    return {
      ...this.capabilities,
      backend: this.backend,
      hardwareAccelerated: this.backend !== 'css',
      supportsComplexAnimations: this.backend === 'threejs',
      supportsParticles: this.backend !== 'css'
    };
  }

  /**
   * Switch to a different graphics backend
   */
  async switchBackend(newBackend) {
    if (newBackend === this.backend) return;
    
    console.log(`🎨 Switching graphics backend from ${this.backend} to ${newBackend}`);
    
    // Cleanup current renderer
    if (this.renderer && this.renderer.cleanup) {
      this.renderer.cleanup();
    }
    
    this.backend = newBackend;
    await this.initializeRenderer();
  }

  /**
   * Clean up graphics resources
   */
  cleanup() {
    if (this.renderer && this.renderer.cleanup) {
      this.renderer.cleanup();
    }
    this.renderer = null;
  }
}

/**
 * CSS-based renderer (fallback/default)
 */
class CSSRenderer {
  constructor() {
    this.name = 'CSS Renderer';
    this.globalState = { color: '#00ff88', state: 'normal' };
  }

  getName() {
    return this.name;
  }

  getCapabilities() {
    return {
      hardwareAccelerated: false,
      supportsComplexCurves: true,
      maxParticles: 50,
      supportsShaders: false
    };
  }

  updateGlobalState(state) {
    this.globalState = { ...this.globalState, ...state };
    
    // Update CSS custom properties for dynamic colors
    document.documentElement.style.setProperty('--continuon-color', state.color);
    document.documentElement.style.setProperty('--continuon-glow', `${state.color}66`);
  }

  async renderMovement(fromPos, toPos, options) {
    // CSS-based Bezier animation (existing implementation)
    const element = document.querySelector('.continuon-cursor');
    if (!element) return;

    const bezier = options.bezier || [0.25, 0.46, 0.45, 0.94];
    const duration = options.duration || 1200;

    element.style.transition = `all ${duration}ms cubic-bezier(${bezier.join(', ')})`;
    element.style.left = toPos.x + 'px';
    element.style.top = toPos.y + 'px';

    return new Promise(resolve => setTimeout(resolve, duration));
  }

  async renderROI(targetRect, options) {
    // CSS-based ROI rendering (existing implementation)
    const roi = document.createElement('div');
    roi.className = `screenshot-roi ${options.animation}`;
    
    roi.style.left = targetRect.left + 'px';
    roi.style.top = targetRect.top + 'px';
    roi.style.width = targetRect.width + 'px';
    roi.style.height = targetRect.height + 'px';
    
    document.body.appendChild(roi);
    
    return roi;
  }

  renderTrail(positions, options) {
    // CSS-based trail rendering
    positions.forEach((pos, index) => {
      setTimeout(() => {
        const trail = document.createElement('div');
        trail.className = 'continuon-trail';
        trail.style.left = pos.x + 'px';
        trail.style.top = pos.y + 'px';
        document.body.appendChild(trail);
        
        setTimeout(() => trail.remove(), 1000);
      }, index * 50);
    });
  }

  cleanup() {
    // Remove CSS custom properties
    document.documentElement.style.removeProperty('--continuon-color');
    document.documentElement.style.removeProperty('--continuon-glow');
  }
}

// Global graphics renderer instance
window.graphicsRenderer = window.graphicsRenderer || new GraphicsRenderer('auto');

export default window.graphicsRenderer;