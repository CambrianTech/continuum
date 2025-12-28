/**
 * ThemeWidget - Theme customization with AI assistance
 *
 * Visual theme editor with embedded AI assistant for designing themes,
 * choosing colors, and customizing workspace appearance.
 * Think Mac Terminal's theme selector, but with AI help.
 */

import { BaseWidget } from './BaseWidget';
import { DATA_COMMANDS } from '@commands/data/shared/DataCommandConstants';
import type { FileLoadParams, FileLoadResult } from '../../commands/file/load/shared/FileLoadTypes';
import { Commands } from '../../system/core/shared/Commands';
import { FILE_COMMANDS } from '../../commands/file/shared/FileCommandConstants';
import { ThemeDiscoveryService } from './themes/ThemeDiscoveryService';
import { ThemeRegistry } from './themes/ThemeTypes';
import type { DataListResult, DataListParams } from '../../commands/data/list/shared/DataListTypes';
import type { DataUpdateParams, DataUpdateResult } from '../../commands/data/update/shared/DataUpdateTypes';
import type { UserStateEntity } from '../../system/data/entities/UserStateEntity';
import { LocalStorageStateManager } from '../../system/core/browser/LocalStorageStateManager';
import { PositronWidgetState } from './services/state/PositronWidgetState';

export class ThemeWidget extends BaseWidget {
  private currentTheme: string = 'base';
  private themeStyleElement: HTMLStyleElement | null = null;
  private themeDiscovery: ThemeDiscoveryService;

  constructor() {
    super({
      widgetName: 'ThemeWidget',
      template: undefined,
      styles: undefined,
      enableAI: false,
      enableDatabase: false,
      enableRouterEvents: false,
      enableScreenshots: false
    });

    // Initialize dynamic theme discovery service
    this.themeDiscovery = new ThemeDiscoveryService();
  }

  protected async onWidgetInitialize(): Promise<void> {
    console.log('🎨 ThemeWidget: Initializing...');

    try {
      // Discover all available themes dynamically
      await this.themeDiscovery.discoverThemes();

      // Get saved theme from localStorage (single source of truth)
      const savedTheme = LocalStorageStateManager.isAvailable()
        ? LocalStorageStateManager.getTheme()
        : null;

      this.currentTheme = savedTheme || 'base';
      console.log(`🎨 ThemeWidget: Using theme '${this.currentTheme}' from localStorage`);

    } catch (error) {
      console.error('❌ ThemeWidget: Theme discovery failed:', error);
      this.currentTheme = 'base';
    }

    // Emit Positron context for AI awareness
    this.emitPositronContext();

    console.log('✅ ThemeWidget: Initialization complete');
  }

  /**
   * Emit Positron context for AI awareness
   */
  private emitPositronContext(): void {
    const themes = ThemeRegistry.getAllThemes();

    PositronWidgetState.emit(
      {
        widgetType: 'theme',
        section: 'theme-selector',
        title: 'Theme Customization',
        metadata: {
          currentTheme: this.currentTheme,
          availableThemes: themes.length,
          themeNames: themes.map(t => t.name)
        }
      },
      { action: 'configuring', target: 'workspace appearance' }
    );
  }

  protected async renderWidget(): Promise<void> {
    console.log(`🎨 ThemeWidget: renderWidget() - currentTheme: ${this.currentTheme}`);

    // Load theme CSS if not already in DOM (uses this.currentTheme set from localStorage in init)
    if (!this.themeStyleElement) {
      console.log(`🎨 ThemeWidget: Loading theme '${this.currentTheme}' CSS`);
      await this.setTheme(this.currentTheme);
    }

    const styles = `
      :host {
        display: flex;
        width: 100%;
        height: 100%;
        overflow: hidden;
      }

      .theme-layout {
        display: flex;
        flex: 1;
        width: 100%;
        height: 100%;
      }

      .theme-main {
        flex: 1;
        overflow-y: auto;
        padding: 20px 24px;
        min-width: 0;
      }

      .theme-container {
        width: 100%;
      }

      .theme-header {
        margin-bottom: 24px;
      }

      .theme-title {
        font-size: 24px;
        font-weight: 600;
        color: var(--content-accent, #00d4ff);
        margin: 0 0 8px 0;
      }

      .theme-subtitle {
        color: rgba(255, 255, 255, 0.6);
        font-size: 14px;
      }

      .theme-section {
        background: rgba(15, 20, 25, 0.8);
        border: 1px solid rgba(0, 212, 255, 0.2);
        border-radius: 8px;
        padding: 20px;
        margin-bottom: 16px;
      }

      .section-title {
        font-size: 16px;
        font-weight: 600;
        color: var(--content-accent, #00d4ff);
        margin: 0 0 16px 0;
        padding-bottom: 8px;
        border-bottom: 1px solid var(--border-accent, rgba(0, 212, 255, 0.2));
      }

      .theme-grid {
        display: grid;
        grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
        gap: 12px;
      }

      .theme-card {
        background: rgba(0, 10, 15, 0.8);
        border: 2px solid rgba(0, 212, 255, 0.2);
        border-radius: 8px;
        padding: 12px;
        cursor: pointer;
        transition: all 0.2s ease;
        text-align: center;
      }

      .theme-card:hover {
        border-color: rgba(0, 212, 255, 0.5);
        background: rgba(0, 212, 255, 0.05);
        transform: translateY(-2px);
      }

      .theme-card.active {
        border-color: var(--content-accent, #00d4ff);
        background: rgba(0, 212, 255, 0.1);
        box-shadow: 0 0 12px rgba(0, 212, 255, 0.3);
      }

      .theme-preview {
        width: 100%;
        height: 60px;
        border-radius: 4px;
        margin-bottom: 8px;
        display: flex;
        align-items: center;
        justify-content: center;
        font-family: monospace;
        font-size: 11px;
      }

      .theme-name {
        font-size: 13px;
        font-weight: 500;
        color: white;
      }

      .theme-description {
        font-size: 11px;
        color: rgba(255, 255, 255, 0.5);
        margin-top: 4px;
      }

      .current-theme-display {
        display: flex;
        align-items: center;
        gap: 12px;
        padding: 16px;
        background: rgba(0, 212, 255, 0.1);
        border: 1px solid rgba(0, 212, 255, 0.3);
        border-radius: 8px;
        margin-bottom: 20px;
      }

      .current-theme-label {
        color: rgba(255, 255, 255, 0.6);
        font-size: 13px;
      }

      .current-theme-name {
        color: var(--content-accent, #00d4ff);
        font-weight: 600;
        font-size: 16px;
      }

      .info-box {
        background: rgba(0, 212, 255, 0.1);
        border: 1px solid rgba(0, 212, 255, 0.3);
        border-radius: 6px;
        padding: 12px 16px;
        margin-bottom: 20px;
        font-size: 13px;
        color: rgba(255, 255, 255, 0.8);
      }

    `;

    // Get available themes
    const themes = ThemeRegistry.getAllThemes();

    const themeCardsHtml = themes.map(theme => `
      <div class="theme-card ${theme.name === this.currentTheme ? 'active' : ''}"
           data-theme="${theme.name}"
           title="${theme.description}">
        <div class="theme-preview" style="background: ${theme.preview?.backgroundColor || '#0a0f14'}; color: ${theme.preview?.textColor || '#00d4ff'}; border: 1px solid ${theme.preview?.primaryColor || '#00d4ff'};">
          Aa
        </div>
        <div class="theme-name">${theme.displayName}</div>
        <div class="theme-description">${theme.category}</div>
      </div>
    `).join('');

    const template = `
      <div class="theme-layout">
        <div class="theme-main">
          <div class="theme-container">
            <div class="theme-header">
              <h1 class="theme-title">Theme</h1>
              <p class="theme-subtitle">Customize your workspace appearance</p>
            </div>

            <div class="info-box">
              <strong>AI Theme Design:</strong> Ask the AI assistant for help choosing colors,
              creating custom themes, or matching a specific aesthetic.
            </div>

            <div class="current-theme-display">
              <span class="current-theme-label">Current Theme:</span>
              <span class="current-theme-name">${this.currentTheme}</span>
            </div>

            <div class="theme-section">
              <h2 class="section-title">Available Themes</h2>
              <div class="theme-grid">
                ${themeCardsHtml}
              </div>
            </div>
          </div>
        </div>
      </div>
    `;

    this.shadowRoot!.innerHTML = `<style>${styles}</style>${template}`;

    // Setup event listeners
    this.setupThemeControls();

    console.log('✅ ThemeWidget: Rendered');
  }

  protected async onWidgetCleanup(): Promise<void> {
    // KEEP theme CSS in document.head - it should persist across tab changes
    // Just clear our reference so we can re-acquire it on next init
    this.themeStyleElement = null;

    console.log('✅ ThemeWidget: Cleanup complete (theme CSS preserved in document.head)');
  }

  /**
   * Switch theme - API for external control
   */
  async setTheme(themeName: string): Promise<void> {
    console.log(`🎨 ThemeWidget: Switching to theme '${themeName}'`);
    this.currentTheme = themeName;

    // Reload all theme CSS and inject into document head
    try {
      const combinedCSS = await this.loadAllThemeCSS();

      // Inject updated theme CSS into document head
      await this.injectThemeIntoDocumentHead(combinedCSS);

      // Save theme preference to UserState for persistence
      await this.saveThemeToUserState(themeName);

      // Re-render widget to show updated theme name
      await this.renderWidget();

      // Update Positron context with new theme
      this.emitPositronContext();

      console.log('✅ ThemeWidget: Theme switched, injected globally, and saved to UserState');

    } catch (error) {
      console.error('❌ ThemeWidget: Failed to switch theme:', error);
    }
  }

  /**
   * Inject theme CSS into document head for global widget access
   * Updates existing theme style or creates new one
   */
  private async injectThemeIntoDocumentHead(combinedCSS: string): Promise<void> {
    try {
      console.log('🎨 ThemeWidget: Injecting theme CSS into document head...');

      // Check for existing theme style elements in DOM (may exist from previous widget instance)
      const existingStyles = document.querySelectorAll('style[id^="jtag-theme-"]');
      existingStyles.forEach(el => el.remove());

      // Create new theme style element
      this.themeStyleElement = document.createElement('style');
      this.themeStyleElement.id = `jtag-theme-${this.currentTheme}`;
      this.themeStyleElement.textContent = combinedCSS;

      document.head.appendChild(this.themeStyleElement);

      console.log(`✅ ThemeWidget: Theme '${this.currentTheme}' CSS injected (${combinedCSS.length} chars)`);

      // Dispatch theme change event
      this.dispatchEvent(new CustomEvent('theme-changed', {
        detail: { themeName: this.currentTheme },
        bubbles: true
      }));

    } catch (error) {
      console.error('❌ ThemeWidget: Failed to inject theme CSS:', error);
      throw error;
    }
  }

  /**
   * Load ALL theme CSS (base + current theme) for injection into templateCSS
   */
  private async loadAllThemeCSS(): Promise<string> {
    try {
      console.log('🎨 ThemeWidget: Loading ALL theme CSS (base + theme)');

      // Load base CSS files from themes/base/
      const baseStyles = await this.loadDirectoryStyles('base');

      // Load theme-specific CSS files from themes/current-theme-name/
      const themeStyles = this.currentTheme !== 'base'
        ? await this.loadDirectoryStyles(this.currentTheme)
        : '';

      // Combine base + theme styles
      const combinedCSS = baseStyles + themeStyles;

      console.log(`✅ ThemeWidget: Combined theme CSS loaded (${combinedCSS.length} chars)`);
      return combinedCSS;

    } catch (error) {
      console.error('❌ ThemeWidget: Failed to load all theme CSS:', error);
      return '';
    }
  }

  /**
   * Load all CSS files from a theme directory using BaseWidget's existing asset delivery
   */
  private async loadDirectoryStyles(directoryName: string): Promise<string> {
    try {
      // Get list of files to load for this directory
      const cssFiles = await this.getDirectoryFiles(directoryName);
      let combinedStyles = '';

      for (const fileName of cssFiles) {
        try {
          // Use BaseWidget's protected executeCommand method - same as loadResource does internally
          const filePath = `widgets/shared/themes/${directoryName}/${fileName}`;
          console.log(`🎨 ThemeWidget: Loading ${filePath} via BaseWidget executeCommand`);

          const result = await Commands.execute<FileLoadParams, FileLoadResult>(FILE_COMMANDS.LOAD, {
            filepath: filePath
          });

          // Handle nested JTAG response structure (same as BaseWidget loadResource)
          const fileData = (result as FileLoadResult & { commandResult?: FileLoadResult }).commandResult ?? result;
          if (result.success && fileData.success && fileData.content) {
            combinedStyles += `\n/* === ${directoryName}/${fileName} === */\n${fileData.content}\n`;
            console.log(`✅ ThemeWidget: Loaded ${directoryName}/${fileName} (${fileData.bytesRead} bytes)`);
          } else {
            console.log(`⚠️ ThemeWidget: ${directoryName}/${fileName} not found - skipping`);
          }
        } catch (error) {
          console.warn(`⚠️ ThemeWidget: Could not load ${directoryName}/${fileName}:`, error);
        }
      }

      return combinedStyles;
    } catch (error) {
      console.error(`❌ ThemeWidget: Failed to load directory styles for '${directoryName}':`, error);
      return '';
    }
  }

  /**
   * Get list of CSS files for a theme from its manifest
   */
  private async getDirectoryFiles(directoryName: string): Promise<string[]> {
    // Get files from theme manifest in registry
    const themeManifest = ThemeRegistry.getTheme(directoryName);
    if (themeManifest) {
      return themeManifest.files;
    }

    // Fallback to standard theme.css if no manifest found
    console.warn(`⚠️ ThemeWidget: No manifest found for theme '${directoryName}', using fallback`);
    return ['theme.css'];
  }

  /**
   * Get current theme name
   */
  getCurrentTheme(): string {
    return this.currentTheme;
  }

  /**
   * List available themes
   */
  async getAvailableThemes(): Promise<string[]> {
    // Return themes from dynamic registry
    const themes = ThemeRegistry.getAllThemes();
    return themes.map(theme => theme.name);
  }

  /**
   * Set up theme switching controls and event handlers
   */
  private setupThemeControls(): void {
    // Handle theme card clicks
    this.shadowRoot?.querySelectorAll('.theme-card').forEach(card => {
      card.addEventListener('click', async () => {
        const themeName = (card as HTMLElement).dataset.theme;
        if (themeName && themeName !== this.currentTheme) {
          console.log(`🎨 ThemeWidget: Theme card clicked - switching to '${themeName}'`);
          // Use setTheme directly - it handles CSS loading, persistence to localStorage AND UserState
          await this.setTheme(themeName);
        }
      });
    });

    console.log('✅ ThemeWidget: Theme controls set up successfully');
  }

  /**
   * Save theme preference to UserState (initialized by JTAGClient during connection)
   */
  private async saveThemeToUserState(themeName: string): Promise<void> {
    try {
      console.log(`🔧 ThemeWidget: Saving theme '${themeName}' to UserState`);

      // 1. Save to localStorage immediately for instant persistence
      if (LocalStorageStateManager.isAvailable()) {
        const success = LocalStorageStateManager.setTheme(themeName);
        if (success) {
          console.log(`✅ ThemeWidget: Theme '${themeName}' saved to localStorage`);
        } else {
          console.warn('⚠️ ThemeWidget: Failed to save theme to localStorage');
        }
      }

      // 2. Update UserState (initialized by JTAGClient during connection)
      const { JTAGClient } = await import('../../system/core/client/shared/JTAGClient');
      const jtagClient = await JTAGClient.sharedInstance;
      const userStateId = jtagClient.getUserStateId();

      if (!userStateId) {
        console.warn('⚠️ ThemeWidget: No UserState available - theme saved to localStorage only');
        return;
      }

      console.log(`🔧 ThemeWidget: Updating UserState ${userStateId.substring(0, 8)}...`);

      // Update existing UserState's preferences
      await Commands.execute<DataUpdateParams, DataUpdateResult<UserStateEntity>>(DATA_COMMANDS.UPDATE, {
        collection: 'UserState',
        id: userStateId,
        backend: 'browser',
        data: {
          preferences: {
            theme: themeName
          },
          updatedAt: new Date()
        }
      });

      console.log(`✅ ThemeWidget: Theme '${themeName}' saved to UserState`);


    } catch (error) {
      console.error('❌ ThemeWidget: Failed to save theme using hybrid persistence:', error);
    }
  }

  /**
   * Load theme preference using hybrid persistence (localStorage first, then UserState)
   */
  private async loadThemeFromUserState(): Promise<string | null> {
    try {
      console.log('🔧 ThemeWidget: Loading theme using hybrid persistence');

      // 1. Try localStorage first for instant response
      if (LocalStorageStateManager.isAvailable()) {
        const localTheme = LocalStorageStateManager.getTheme();
        if (localTheme) {
          console.log(`✅ ThemeWidget: Loaded theme '${localTheme}' from localStorage`);
          return localTheme;
        }
        console.log('ℹ️ ThemeWidget: No theme found in localStorage, trying UserState');
      }

      // 2. Fall back to UserState from localStorage for persistence
      // Get persistent device identity (encrypted in localStorage)
      const { BrowserDeviceIdentity } = await import('../../system/core/browser/BrowserDeviceIdentity');
      const identity = await BrowserDeviceIdentity.getOrCreateIdentity();

      console.log(`🔧 ThemeWidget: Loading theme for device ${identity.deviceId.substring(0, 12)}...`);

      // Find the user's UserState in localStorage (get most recent first)
      const userStates = await Commands.execute<DataListParams, DataListResult<UserStateEntity>>(DATA_COMMANDS.LIST, {
        collection: 'UserState',
        filter: {
          userId: identity.userId,
          deviceId: identity.deviceId
        },
        orderBy: [{ field: 'updatedAt', direction: 'desc' }],
        limit: 1
      } as Partial<DataListParams>);

      if (userStates.success && userStates.items && userStates.items.length > 0) {
        const userState = userStates.items[0];

        // Use index signature to access theme field with proper type guards
        const preferences = userState.preferences as Record<string, unknown>;
        const savedTheme = preferences.theme;

        if (typeof savedTheme === 'string') {
          console.log(`✅ ThemeWidget: Loaded theme '${savedTheme}' from UserState database`);

          // Sync back to localStorage for faster future access
          if (LocalStorageStateManager.isAvailable()) {
            LocalStorageStateManager.setTheme(savedTheme);
            console.log(`🔄 ThemeWidget: Synced theme '${savedTheme}' to localStorage`);
          }

          return savedTheme;
        }
      }

      console.log('ℹ️ ThemeWidget: No saved theme found in either localStorage or UserState');
      return null;

    } catch (error) {
      console.error('❌ ThemeWidget: Failed to load theme from UserState:', error);
      return null;
    }
  }
}

// Registration handled by centralized BROWSER_WIDGETS registry
