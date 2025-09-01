/**
 * SidebarPanel - Modular Sidebar Widget
 * 
 * Left or right sidebar panel containing status view, emoter, and dynamic content.
 * Follows the VSCode/Discord/Slack sidebar pattern.
 */

import { BaseWidget } from '../shared/BaseWidget';

export class SidebarWidget extends BaseWidget {
  
  constructor() {
    super({
      widgetName: 'SidebarWidget',
      template: 'sidebar-panel.html',
      styles: 'sidebar-panel.css',
      enableAI: false,
      enableDatabase: false, 
      enableRouterEvents: true,
      enableScreenshots: false
    });
  }

  protected async onWidgetInitialize(): Promise<void> {
    console.log('🎯 SidebarPanel: Initializing sidebar panel...');
    
    // Initialize status view
    await this.updateStatusView();
    
    // Initialize dynamic list based on current page/context
    await this.updateDynamicList();
    
    console.log('✅ SidebarPanel: Sidebar panel initialized');
  }

  protected async renderWidget(): Promise<void> {
    // Use BaseWidget's template and styles system
    const styles = this.templateCSS || '/* No styles loaded */';
    const template = this.templateHTML || '<div>No template loaded</div>';
    
    // Ensure template is a string
    const templateString = typeof template === 'string' ? template : '<div>Template error</div>';
    
    // Replace dynamic content
    const dynamicContent = templateString
      .replace('<!-- STATUS_CONTENT -->', await this.getStatusContent())
      .replace('<!-- DYNAMIC_LIST_CONTENT -->', await this.getDynamicListContent());

    this.shadowRoot!.innerHTML = `
      <style>${styles}</style>
      ${dynamicContent}
    `;
    
    console.log('✅ SidebarPanel: Sidebar panel rendered');
  }

  protected async onWidgetCleanup(): Promise<void> {
    console.log('🧹 SidebarPanel: Cleanup complete');
  }

  /**
   * Update status view display
   */
  private async updateStatusView(): Promise<void> {
    // Update connection status, user status, etc.
    console.log('🔗 SidebarPanel: Status view updated');
  }

  /**
   * Update dynamic list based on current page context
   */
  private async updateDynamicList(): Promise<void> {
    // Load appropriate widgets for current page/context
    console.log('📋 SidebarPanel: Dynamic list updated');
  }

  /**
   * Get status content HTML
   */
  private async getStatusContent(): Promise<string> {
    return `
      <div class="connection-status connected">CONNECTED</div>
      <div class="user-status">Online</div>
    `;
  }

  /**
   * Get dynamic list content HTML
   */
  private async getDynamicListContent(): Promise<string> {
    // For now, return chat rooms - will be dynamic based on page
    return `
      <div class="list-item active">• general</div>
      <div class="list-item">• academy</div>
      <div class="list-item">• community</div>
    `;
  }

  /**
   * Get available rooms/channels for this context
   */
  async getContextualItems(): Promise<string[]> {
    // Will be dynamic based on page - chat, code editor, etc.
    return ['general', 'academy', 'community'];
  }
}

// Register the custom element
customElements.define('sidebar-widget', SidebarWidget);