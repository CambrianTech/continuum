/**
 * MainPanel - Main Content Area Widget
 * 
 * Contains content header with tabs, version info, status buttons,
 * and the main content view that displays different page widgets.
 */

import { BaseWidget } from '../shared/BaseWidget';

export class MainWidget extends BaseWidget {
  
  constructor() {
    super({
      widgetName: 'MainWidget',
      template: 'main-panel.html',
      styles: 'main-panel.css',
      enableAI: false,
      enableDatabase: false, 
      enableRouterEvents: true,
      enableScreenshots: false
    });
  }

  protected async onWidgetInitialize(): Promise<void> {
    console.log('🎯 MainPanel: Initializing main content panel...');
    
    // Initialize content tabs
    await this.initializeContentTabs();
    
    // Initialize version info
    await this.updateVersionInfo();
    
    // Initialize status buttons
    await this.initializeStatusButtons();
    
    console.log('✅ MainPanel: Main panel initialized');
  }

  protected async renderWidget(): Promise<void> {
    // Use BaseWidget's template and styles system
    const styles = this.templateCSS || '/* No styles loaded */';
    const template = this.templateHTML || '<div>No template loaded</div>';
    
    // Ensure template is a string
    const templateString = typeof template === 'string' ? template : '<div>Template error</div>';
    
    // Replace dynamic content
    const dynamicContent = templateString
      .replace('<!-- CONTENT_TABS -->', await this.getContentTabs())
      .replace('<!-- VERSION_INFO -->', await this.getVersionInfo())
      .replace('<!-- STATUS_BUTTONS -->', await this.getStatusButtons());

    this.shadowRoot!.innerHTML = `
      <style>${styles}</style>
      ${dynamicContent}
    `;
    
    console.log('✅ MainPanel: Main panel rendered');
  }

  protected async onWidgetCleanup(): Promise<void> {
    console.log('🧹 MainPanel: Cleanup complete');
  }

  /**
   * Initialize content tabs system
   */
  private async initializeContentTabs(): Promise<void> {
    // Set up tab switching logic
    console.log('📋 MainPanel: Content tabs initialized');
  }

  /**
   * Update version info display
   */
  private async updateVersionInfo(): Promise<void> {
    console.log('🔢 MainPanel: Version info updated');
  }

  /**
   * Initialize status buttons
   */
  private async initializeStatusButtons(): Promise<void> {
    console.log('🔘 MainPanel: Status buttons initialized');
  }

  /**
   * Get content tabs HTML
   */
  private async getContentTabs(): Promise<string> {
    return `
      <div class="content-tab active">Chat</div>
      <div class="content-tab">Code Editor</div>
      <div class="content-tab">Academy</div>
    `;
  }

  /**
   * Get version info HTML
   */
  private async getVersionInfo(): Promise<string> {
    return `
      <div class="version-badge">JTAG v1.0</div>
    `;
  }

  /**
   * Get status buttons HTML
   */
  private async getStatusButtons(): Promise<string> {
    return `
      <button class="status-button">Settings</button>
      <button class="status-button">Help</button>
    `;
  }

  /**
   * Switch to a different content page
   */
  switchToPage(pageName: string): void {
    console.log(`📄 MainPanel: Switching to page: ${pageName}`);
    // Will update the content view to show different widgets
  }
}

// Register the custom element
customElements.define('main-widget', MainWidget);