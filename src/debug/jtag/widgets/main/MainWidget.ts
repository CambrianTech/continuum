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
    
    // Add event listeners after DOM is created
    this.setupEventListeners();
    
    console.log('✅ MainPanel: Main panel rendered');
  }

  private setupEventListeners(): void {
    // Theme button click
    this.shadowRoot?.getElementById('theme-button')?.addEventListener('click', () => {
      this.openThemeTab();
    });

    // Tab clicks
    this.shadowRoot?.querySelectorAll('.content-tab').forEach(tab => {
      tab.addEventListener('click', () => {
        const tabName = (tab as HTMLElement).dataset.tab;
        if (tabName) {
          this.switchToTab(tabName);
        }
      });
    });
  }

  private openThemeTab(): void {
    // Switch to theme tab and show theme widget content
    this.switchToTab('theme');
  }

  private switchToTab(tabName: string): void {
    // Remove active class from all tabs
    this.shadowRoot?.querySelectorAll('.content-tab').forEach(tab => {
      tab.classList.remove('active');
    });

    // Add active class to selected tab
    const selectedTab = this.shadowRoot?.querySelector(`[data-tab="${tabName}"]`);
    selectedTab?.classList.add('active');

    // Update content view based on tab
    this.updateContentView(tabName);
    
    console.log(`📄 MainPanel: Switched to tab: ${tabName}`);
  }

  private updateContentView(tabName: string): void {
    const contentView = this.shadowRoot?.querySelector('.content-view');
    if (!contentView) return;

    switch (tabName) {
      case 'chat':
        contentView.innerHTML = '<chat-widget></chat-widget>';
        break;
      case 'theme':
        contentView.innerHTML = '<theme-widget></theme-widget>';
        break;
      case 'code':
        contentView.innerHTML = '<div>Code Editor - Coming Soon</div>';
        break;
      case 'academy':
        contentView.innerHTML = '<div>Academy - Coming Soon</div>';
        break;
      default:
        contentView.innerHTML = '<chat-widget></chat-widget>';
    }
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
      <div class="content-tab active" data-tab="chat">Chat</div>
      <div class="content-tab" data-tab="code">Code Editor</div>
      <div class="content-tab" data-tab="academy">Academy</div>
      <div class="content-tab" data-tab="theme">Theme</div>
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
      <button class="status-button" id="theme-button">Theme</button>
      <button class="status-button" id="settings-button">Settings</button>
      <button class="status-button" id="help-button">Help</button>
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