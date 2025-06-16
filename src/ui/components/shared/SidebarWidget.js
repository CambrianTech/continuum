/**
 * SidebarWidget Class
 * Intermediate class between BaseWidget and sidebar-specific widgets
 * Adds collapsible header functionality for sidebar components
 */

// Import base widget functionality
import('./BaseWidget.js');

class SidebarWidget extends BaseWidget {
  constructor() {
    super();
    
    // Sidebar-specific state
    this.isCollapsed = false;
    this.widgetCategory = 'Sidebar';
  }

  /**
   * Enhanced header styling with collapsible functionality
   */
  getHeaderStyle() {
    return `
      ${super.getHeaderStyle()}
      
      .sidebar-widget-container {
        background: rgba(20, 25, 35, 0.95);
        border-radius: 12px;
        margin-bottom: 0;
        border: 1px solid rgba(255, 255, 255, 0.1);
        transition: all 0.3s ease;
        display: block;
        width: 100%;
        box-sizing: border-box;
      }
      
      .sidebar-widget-header {
        display: flex;
        align-items: center;
        justify-content: space-between;
        padding: 16px 20px;
        cursor: pointer;
        border-bottom: 1px solid rgba(255, 255, 255, 0.1);
        transition: all 0.3s ease;
        user-select: none;
        background: rgba(255, 255, 255, 0.02);
      }
      
      .sidebar-widget-header:hover {
        background: rgba(255, 255, 255, 0.05);
      }
      
      .sidebar-widget-title {
        display: flex;
        align-items: center;
        gap: 8px;
        color: #e0e6ed;
        font-size: 14px;
        font-weight: 600;
        text-transform: uppercase;
        letter-spacing: 0.5px;
        margin: 0;
      }
      
      .sidebar-widget-icon {
        font-size: 16px;
      }
      
      .sidebar-collapse-btn {
        color: #8a92a5;
        font-size: 18px;
        font-weight: bold;
        transition: all 0.3s ease;
        transform: rotate(0deg);
        line-height: 1;
      }
      
      .sidebar-collapse-btn.collapsed {
        transform: rotate(-90deg);
      }
      
      .sidebar-widget-content {
        padding: 20px;
        transition: all 0.3s ease;
        max-height: 1000px;
        overflow: hidden;
      }
      
      .sidebar-widget-content.collapsed {
        max-height: 0;
        padding-top: 0;
        padding-bottom: 0;
        opacity: 0;
      }
      
      .sidebar-widget-container.collapsed {
        border-bottom: none;
      }
    `;
  }

  /**
   * Render the sidebar widget structure with collapsible header
   */
  renderSidebarStructure(title, content) {
    return `
      <div class="sidebar-widget-container ${this.isCollapsed ? 'collapsed' : ''}">
        <div class="sidebar-widget-header" id="widget-header">
          <h3 class="sidebar-widget-title">
            <span class="sidebar-widget-icon">${this.widgetIcon}</span>
            <span>${title}</span>
          </h3>
          <div class="sidebar-collapse-btn ${this.isCollapsed ? 'collapsed' : ''}" id="collapse-btn">
            ▼
          </div>
        </div>
        <div class="sidebar-widget-content ${this.isCollapsed ? 'collapsed' : ''}" id="widget-content">
          ${content}
        </div>
      </div>
    `;
  }

  /**
   * Setup sidebar-specific event listeners
   */
  setupSidebarEventListeners() {
    const header = this.shadowRoot.getElementById('widget-header');
    const collapseBtn = this.shadowRoot.getElementById('collapse-btn');
    
    if (header) {
      header.addEventListener('click', () => this.toggleCollapse());
    }
    
    if (collapseBtn) {
      collapseBtn.addEventListener('click', (e) => {
        e.stopPropagation();
        this.toggleCollapse();
      });
    }
  }

  /**
   * Toggle collapsed state
   */
  toggleCollapse() {
    this.isCollapsed = !this.isCollapsed;
    this.updateCollapseState();
    
    // Save collapsed state to localStorage
    localStorage.setItem(`sidebar-widget-${this.widgetName}-collapsed`, this.isCollapsed);
    
    // Dispatch event for parent components
    this.dispatchEvent(new CustomEvent('sidebar-widget-collapsed', {
      detail: { widgetName: this.widgetName, isCollapsed: this.isCollapsed },
      bubbles: true
    }));
  }

  /**
   * Update UI to reflect collapsed state
   */
  updateCollapseState() {
    const container = this.shadowRoot.querySelector('.sidebar-widget-container');
    const content = this.shadowRoot.getElementById('widget-content');
    const collapseBtn = this.shadowRoot.getElementById('collapse-btn');
    
    if (container && content && collapseBtn) {
      if (this.isCollapsed) {
        container.classList.add('collapsed');
        content.classList.add('collapsed');
        collapseBtn.classList.add('collapsed');
      } else {
        container.classList.remove('collapsed');
        content.classList.remove('collapsed');
        collapseBtn.classList.remove('collapsed');
      }
    }
  }

  /**
   * Load collapsed state from localStorage
   */
  loadCollapseState() {
    const savedState = localStorage.getItem(`sidebar-widget-${this.widgetName}-collapsed`);
    if (savedState !== null) {
      this.isCollapsed = savedState === 'true';
    }
  }

  /**
   * Initialize sidebar widget
   */
  connectedCallback() {
    super.connectedCallback();
    this.loadCollapseState();
  }

  /**
   * Override setupEventListeners to include sidebar functionality
   */
  setupEventListeners() {
    super.setupEventListeners();
    this.setupSidebarEventListeners();
  }
}

// Export for use by other widgets
if (typeof module !== 'undefined' && module.exports) {
  module.exports = SidebarWidget;
} else if (typeof window !== 'undefined') {
  window.SidebarWidget = SidebarWidget;
}