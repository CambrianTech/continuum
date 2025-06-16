/**
 * Base Widget Class
 * Shared functionality for all Continuum UI widgets
 */

class BaseWidget extends HTMLElement {
  constructor() {
    super();
    this.attachShadow({ mode: 'open' });
    
    // Common widget state
    this.isLoading = false;
    this.hasError = false;
    this.errorMessage = '';
    this.refreshTimeout = null;
    
    // Widget metadata (should be overridden)
    this.widgetName = 'BaseWidget';
    this.widgetIcon = '🎛️';
    this.widgetCategory = 'User Interface';
  }

  async connectedCallback() {
    await this.render();
    this.setupEventListeners();
    this.initializeWidget();
  }

  disconnectedCallback() {
    this.cleanup();
  }

  /**
   * Common widget header styling with collapsible functionality
   */
  getHeaderStyle() {
    return `
      .widget-container {
        background: rgba(20, 25, 35, 0.95);
        border-radius: 12px;
        margin-bottom: 20px;
        border: 1px solid rgba(255, 255, 255, 0.1);
      }
      
      .widget-header-bar {
        display: flex;
        align-items: center;
        justify-content: space-between;
        padding: 16px 20px;
        cursor: pointer;
        border-bottom: 1px solid rgba(255, 255, 255, 0.1);
        transition: all 0.3s ease;
        user-select: none;
      }
      
      .widget-header-bar:hover {
        background: rgba(255, 255, 255, 0.05);
      }
      
      .widget-header-title {
        color: #e0e6ed;
        font-size: 14px;
        font-weight: 600;
        text-transform: uppercase;
        letter-spacing: 0.5px;
        margin: 0;
      }
      
      .widget-collapse-btn {
        color: #8a92a5;
        font-size: 16px;
        font-weight: bold;
        transition: all 0.3s ease;
        transform: rotate(0deg);
      }
      
      .widget-collapse-btn.collapsed {
        transform: rotate(-90deg);
      }
      
      .widget-content {
        padding: 20px;
        transition: all 0.3s ease;
        overflow: hidden;
      }
      
      .widget-content.collapsed {
        display: none;
      }
      
      .loading {
        color: #8a92a5;
        font-size: 14px;
        text-align: center;
        padding: 20px;
      }
      
      .error {
        color: #F44336;
        font-size: 14px;
        text-align: center;
        padding: 20px;
      }
      
      .refresh-btn {
        width: 100%;
        background: rgba(79, 195, 247, 0.1);
        border: 1px solid rgba(79, 195, 247, 0.3);
        color: #4FC3F7;
        border-radius: 8px;
        padding: 10px 15px;
        font-size: 13px;
        cursor: pointer;
        transition: all 0.3s ease;
        margin-top: 15px;
      }
      
      .refresh-btn:hover {
        background: rgba(79, 195, 247, 0.2);
        border-color: rgba(79, 195, 247, 0.5);
      }
    `;
  }

  /**
   * Common loading state management
   */
  setLoading(loading, message = 'Loading...') {
    this.isLoading = loading;
    this.loadingMessage = message;
    this.updateLoadingState();
  }

  /**
   * Common error state management
   */
  setError(error, message = 'An error occurred') {
    this.hasError = !!error;
    this.errorMessage = message;
    this.updateErrorState();
  }

  /**
   * Common refresh functionality
   */
  async refresh() {
    try {
      this.setLoading(true, `Refreshing ${this.widgetName}...`);
      this.setError(false);
      
      await this.onRefresh();
      
      this.setLoading(false);
    } catch (error) {
      console.error(`${this.widgetName} refresh failed:`, error);
      this.setError(true, `Failed to refresh ${this.widgetName}`);
      this.setLoading(false);
    }
  }

  /**
   * Auto-refresh functionality
   */
  startAutoRefresh(intervalMs = 30000) {
    this.stopAutoRefresh();
    this.refreshTimeout = setInterval(() => this.refresh(), intervalMs);
  }

  stopAutoRefresh() {
    if (this.refreshTimeout) {
      clearInterval(this.refreshTimeout);
      this.refreshTimeout = null;
    }
  }

  /**
   * Common API call wrapper
   */
  async apiCall(endpoint, options = {}) {
    try {
      const response = await fetch(endpoint, {
        headers: {
          'Content-Type': 'application/json',
          ...options.headers
        },
        ...options
      });
      
      if (!response.ok) {
        throw new Error(`API call failed: ${response.status} ${response.statusText}`);
      }
      
      return await response.json();
    } catch (error) {
      console.error(`${this.widgetName} API call failed:`, error);
      throw error;
    }
  }

  /**
   * Widget lifecycle methods - should be overridden by subclasses
   */
  async onRefresh() {
    // Override in subclasses
    console.log(`${this.widgetName}: onRefresh not implemented`);
  }

  initializeWidget() {
    // Override in subclasses
    console.log(`${this.widgetName}: initializeWidget not implemented`);
  }

  render() {
    // Override in subclasses
    console.log(`${this.widgetName}: render not implemented`);
  }

  setupEventListeners() {
    // Override in subclasses
    console.log(`${this.widgetName}: setupEventListeners not implemented`);
  }

  cleanup() {
    this.stopAutoRefresh();
    // Additional cleanup in subclasses
  }

  updateLoadingState() {
    // Update UI based on loading state - override in subclasses
  }

  updateErrorState() {
    // Update UI based on error state - override in subclasses
  }
}

// Export for use by other widgets
if (typeof module !== 'undefined' && module.exports) {
  module.exports = BaseWidget;
} else if (typeof window !== 'undefined') {
  window.BaseWidget = BaseWidget;
}