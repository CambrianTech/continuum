/**
 * MainPanel - Main Content Area Widget
 * 
 * Contains content header with tabs, version info, status buttons,
 * and the main content view that displays different page widgets.
 */

import { BaseWidget } from '../shared/BaseWidget';
import { ContentInfoManager, ContentInfo } from './shared/ContentTypes';

export class MainWidget extends BaseWidget {
  private currentPath = '/chat/general'; // Current open room/path
  private contentManager: ContentInfoManager;
  private currentContent: ContentInfo | null = null;
  
  constructor() {
    super({
      widgetName: 'MainWidget',
      template: 'main-panel.html',
      styles: 'main-panel.css',
      enableAI: false,
      enableDatabase: true, // Enable database for room/content management
      enableRouterEvents: true,
      enableScreenshots: false
    });
    
    // Initialize content manager with widget context
    this.contentManager = new ContentInfoManager(this);
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
    // Show theme modal/overlay instead of switching tabs
    this.showThemeModal();
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

    // Content view always shows chat for now
    // Theme is handled via modal, not content switching
    contentView.innerHTML = '<chat-widget></chat-widget>';
  }

  private showThemeModal(): void {
    // Remove existing dropdown
    const existingDropdown = this.shadowRoot?.querySelector('.theme-slide-panel');
    if (existingDropdown) {
      existingDropdown.remove();
      return; // Toggle behavior - close if already open
    }

    // Create slide-down panel from the top
    const slidePanel = document.createElement('div');
    slidePanel.className = 'theme-slide-panel';
    slidePanel.innerHTML = `
      <theme-widget></theme-widget>
    `;

    // Add fast smooth slide-down styles
    const style = document.createElement('style');
    style.textContent = `
      .theme-slide-panel {
        position: fixed;
        top: 0;
        right: 20px;
        z-index: 2000;
        width: 320px;
        background: rgba(0, 10, 15, 0.98);
        border: 2px solid #00d4ff;
        border-top: none;
        border-radius: 0 0 8px 8px;
        box-shadow: 
          0 8px 24px rgba(0, 212, 255, 0.4),
          inset 0 1px 0 rgba(0, 212, 255, 0.2);
        overflow: hidden;
        
        /* Fast smooth slide from top */
        animation: slide-down-fast 0.2s cubic-bezier(0.23, 1, 0.32, 1);
        transform-origin: top center;
      }
      
      @keyframes slide-down-fast {
        0% {
          transform: translateY(-100%);
          opacity: 0;
        }
        100% {
          transform: translateY(0);
          opacity: 1;
        }
      }
      
      /* Cyberpunk border glow */
      .theme-slide-panel::before {
        content: '';
        position: absolute;
        top: 0;
        left: -2px;
        right: -2px;
        bottom: -2px;
        background: linear-gradient(180deg, #00d4ff, rgba(0, 212, 255, 0.1));
        border-radius: 0 0 8px 8px;
        z-index: -1;
        opacity: 0.6;
      }
    `;

    // Prevent panel clicks from propagating (stops click-outside detection)
    slidePanel.addEventListener('click', (event) => {
      const target = event.target as HTMLElement;
      
      // Handle Apply button - close panel after applying
      if (target.id === 'apply-theme') {
        setTimeout(() => {
          slidePanel.remove();
          style.remove();
          document.removeEventListener('click', handleClickOutside);
        }, 200);
      } else {
        // For all other clicks inside panel, stop propagation to prevent closing
        event.stopPropagation();
      }
    });

    // Append to shadow root for proper positioning
    this.shadowRoot?.appendChild(style);
    this.shadowRoot?.appendChild(slidePanel);

    // Close panel function
    const closePanel = () => {
      slidePanel.remove();
      style.remove();
      document.removeEventListener('click', handleClickOutside);
    };

    // Click-outside detection
    const handleClickOutside = (event: Event) => {
      closePanel();
    };

    // Add click-outside detection with delay
    setTimeout(() => {
      document.addEventListener('click', handleClickOutside);
    }, 200);

    // Listen for theme-cancelled event from Cancel button
    slidePanel.addEventListener('theme-cancelled', closePanel);
    
    // Listen for theme-applied event from Apply button
    slidePanel.addEventListener('theme-applied', (event) => {
      console.log('🎨 MainPanel: Theme applied, closing panel');
      closePanel();
    });

    console.log('🎨 MainPanel: Theme slide panel opened');
  }

  /**
   * Navigate to a different path (e.g., /chat/academy, /chat/user-123)
   */
  async navigateToPath(newPath: string): Promise<void> {
    const [, pathType, roomId] = newPath.split('/');
    
    // Check if room exists, create if needed (especially for user chats)
    if (pathType === 'chat' && roomId) {
      await this.ensureRoomExists(roomId);
    }
    
    // Update current path and clear cached content
    this.currentPath = newPath;
    this.currentContent = null;
    
    // Load new content information
    await this.loadCurrentContent();
    
    // Subscribe to events for this path
    await this.subscribeToPathEvents(newPath);
    
    // Re-render to update tabs and content for new path
    await this.renderWidget();
    
    const contentName = this.currentContent ? (this.currentContent as any).displayName : 'unknown';
    console.log(`🔄 MainPanel: Navigated to path: ${newPath} (${contentName})`);
  }

  /**
   * Ensure room exists using proper data classes and database daemon
   */
  private async ensureRoomExists(roomId: string): Promise<void> {
    try {
      // Use content manager to get or create room
      const content = await this.contentManager.getContentByPath(`/chat/${roomId}`);
      
      if (!content) {
        // Determine room type based on roomId
        const roomType = roomId.startsWith('user-') ? 'user_chat' : 'private';
        await this.contentManager.createRoom(roomId, roomType);
        console.log(`🏠 MainPanel: Created new room: ${roomId} (${roomType})`);
      } else {
        console.log(`✅ MainPanel: Room exists: ${roomId} (${content.displayName})`);
      }
    } catch (error) {
      console.error(`❌ MainPanel: Failed to ensure room ${roomId} exists:`, error);
    }
  }

  /**
   * Subscribe to events for the current path
   */
  private async subscribeToPathEvents(path: string): Promise<void> {
    // Events already have path built in, so just subscribe to this path
    console.log(`📡 MainPanel: Subscribing to events for path: ${path}`);
    // The actual event subscription would happen here
    // For now, just log that we're subscribing to path-based events
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
   * Load content information for current path
   */
  private async loadCurrentContent(): Promise<void> {
    try {
      this.currentContent = await this.contentManager.getContentByPath(this.currentPath);
      console.log(`📄 MainPanel: Loaded content info for ${this.currentPath}:`, this.currentContent?.displayName);
    } catch (error) {
      console.error(`❌ MainPanel: Failed to load content for ${this.currentPath}:`, error);
      this.currentContent = null;
    }
  }

  /**
   * Get content tabs HTML - database-driven based on current path
   */
  private async getContentTabs(): Promise<string> {
    // Ensure current content is loaded
    if (!this.currentContent) {
      await this.loadCurrentContent();
    }
    
    // Use content info from database
    if (this.currentContent) {
      const displayName = this.currentContent.displayName || this.currentContent.name;
      const contentType = this.currentContent.type;
      
      return `
        <div class="content-tab active" data-tab="${contentType}" data-content-id="${this.currentContent.id}">
          ${displayName}
        </div>
      `;
    }
    
    // Fallback for loading states
    const [, pathType, roomId] = this.currentPath.split('/');
    const fallbackName = roomId ? roomId.charAt(0).toUpperCase() + roomId.slice(1) : 'Chat';
    
    return `
      <div class="content-tab active" data-tab="${pathType}">
        ${fallbackName}
      </div>
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

// Registration handled by centralized BROWSER_WIDGETS registry