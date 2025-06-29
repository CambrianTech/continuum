/**
 * Component Loader Utility
 * Helps load and manage web components in the UI
 */

class ComponentLoader {
  constructor() {
    this.loadedComponents = new Set();
    this.componentInstances = new Map();
  }

  /**
   * Load a component script and register it
   */
  async loadComponent(componentName, scriptPath) {
    if (this.loadedComponents.has(componentName)) {
      return true;
    }

    try {
      // In browser environment, load via script tag
      if (typeof document !== 'undefined') {
        return new Promise((resolve, reject) => {
          const script = document.createElement('script');
          script.src = scriptPath;
          script.onload = () => {
            this.loadedComponents.add(componentName);
            resolve(true);
          };
          script.onerror = () => reject(new Error(`Failed to load component: ${componentName}`));
          document.head.appendChild(script);
        });
      } else {
        // In Node.js environment (for testing)
        require(scriptPath);
        this.loadedComponents.add(componentName);
        return true;
      }
    } catch (error) {
      console.error(`Error loading component ${componentName}:`, error);
      return false;
    }
  }

  /**
   * Create and configure a component instance
   */
  createComponent(tagName, config = {}) {
    const element = document.createElement(tagName);
    
    // Apply configuration
    Object.keys(config).forEach(key => {
      if (typeof element[key] === 'function') {
        element[key](config[key]);
      } else {
        element[key] = config[key];
      }
    });

    return element;
  }

  /**
   * Replace existing HTML with a component
   */
  replaceWithComponent(selector, tagName, config = {}) {
    const targetElement = document.querySelector(selector);
    if (!targetElement) {
      console.warn(`Target element not found: ${selector}`);
      return null;
    }

    const component = this.createComponent(tagName, config);
    targetElement.parentNode.replaceChild(component, targetElement);
    
    // Store reference
    const instanceId = `${tagName}_${Date.now()}`;
    this.componentInstances.set(instanceId, component);
    
    return { component, instanceId };
  }

  /**
   * Insert component into container
   */
  insertComponent(containerSelector, tagName, config = {}) {
    const container = document.querySelector(containerSelector);
    if (!container) {
      console.warn(`Container not found: ${containerSelector}`);
      return null;
    }

    const component = this.createComponent(tagName, config);
    container.appendChild(component);
    
    // Store reference
    const instanceId = `${tagName}_${Date.now()}`;
    this.componentInstances.set(instanceId, component);
    
    return { component, instanceId };
  }

  /**
   * Get component instance by ID
   */
  getInstance(instanceId) {
    return this.componentInstances.get(instanceId);
  }

  /**
   * Remove component instance
   */
  removeInstance(instanceId) {
    const component = this.componentInstances.get(instanceId);
    if (component && component.parentNode) {
      component.parentNode.removeChild(component);
    }
    this.componentInstances.delete(instanceId);
  }

  /**
   * Get inline script for loading components in HTML
   */
  getInlineLoaderScript(components) {
    return `
      <script>
        window.componentLoader = new (${ComponentLoader.toString()})();
        
        // Load components
        Promise.all([
          ${components.map(comp => 
            `window.componentLoader.loadComponent('${comp.name}', '${comp.path}')`
          ).join(',\n          ')}
        ]).then(() => {
          console.log('✅ All components loaded successfully');
          // Trigger component initialization event
          document.dispatchEvent(new CustomEvent('components-ready'));
        }).catch(error => {
          console.error('❌ Component loading failed:', error);
        });
      </script>
    `;
  }
}

// Export for both browser and Node.js
if (typeof module !== 'undefined' && module.exports) {
  module.exports = ComponentLoader;
}
if (typeof window !== 'undefined') {
  window.ComponentLoader = ComponentLoader;
}