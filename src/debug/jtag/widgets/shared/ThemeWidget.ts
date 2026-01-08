/**
 * ThemeWidget - Theme customization with AI assistance
 *
 * Visual theme editor with embedded AI assistant for designing themes,
 * choosing colors, and customizing workspace appearance.
 * Think Mac Terminal's theme selector, but with AI help.
 *
 * Structure:
 * - public/theme-widget.html - Template container
 * - public/theme-widget.scss - Styles (compiled to .css)
 * - ThemeWidget.ts - Logic (this file)
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
      template: 'theme-widget.html',
      styles: 'theme-widget.css',
      enableAI: false,
      enableDatabase: false,
      enableRouterEvents: false,
      enableScreenshots: false
    });

    // Initialize dynamic theme discovery service
    this.themeDiscovery = new ThemeDiscoveryService();
  }

  /**
   * Override path resolution - ThemeWidget is in widgets/shared/, public folder is there too
   */
  protected resolveResourcePath(filename: string): string {
    return `widgets/shared/public/${filename}`;
  }

  protected async onWidgetInitialize(): Promise<void> {
    this.verbose() && console.log('🎨 ThemeWidget: Initializing...');

    try {
      // Discover all available themes dynamically
      await this.themeDiscovery.discoverThemes();

      // Get saved theme from localStorage (single source of truth)
      const savedTheme = LocalStorageStateManager.isAvailable()
        ? LocalStorageStateManager.getTheme()
        : null;

      this.currentTheme = savedTheme || 'base';
      this.verbose() && console.log(`🎨 ThemeWidget: Using theme '${this.currentTheme}' from localStorage`);

    } catch (error) {
      console.error('❌ ThemeWidget: Theme discovery failed:', error);
      this.currentTheme = 'base';
    }

    // Emit Positron context for AI awareness
    this.emitPositronContext();

    this.verbose() && console.log('✅ ThemeWidget: Initialization complete');
  }

  /**
   * Emit Positron context for AI awareness
   * NOTE: Removed emit - MainWidget handles context. Widgets should RECEIVE, not emit.
   */
  private emitPositronContext(): void {
    // No-op - context cascade fix
  }

  protected async renderWidget(): Promise<void> {
    this.verbose() && console.log(`🎨 ThemeWidget: renderWidget() - currentTheme: ${this.currentTheme}`);

    // Load theme CSS if not already in DOM (uses this.currentTheme set from localStorage in init)
    if (!this.themeStyleElement) {
      this.verbose() && console.log(`🎨 ThemeWidget: Loading theme '${this.currentTheme}' CSS`);
      await this.setTheme(this.currentTheme);
    }

    // Inject loaded template and styles into shadow DOM
    if (this.shadowRoot && (this.templateHTML || this.templateCSS)) {
      const styleTag = this.templateCSS ? `<style>${this.templateCSS}</style>` : '';
      this.shadowRoot.innerHTML = styleTag + (this.templateHTML || '');
    }

    // Render dynamic content
    this.renderContent();

    // Setup event listeners
    this.setupThemeControls();

    this.verbose() && console.log('✅ ThemeWidget: Rendered');
  }

  private renderContent(): void {
    // Update current theme display
    const themeNameEl = this.shadowRoot?.querySelector('#current-theme-name');
    if (themeNameEl) {
      themeNameEl.textContent = this.currentTheme;
    }

    // Render theme cards
    const themeGrid = this.shadowRoot?.querySelector('#theme-grid');
    if (!themeGrid) return;

    const themes = ThemeRegistry.getAllThemes();

    themeGrid.innerHTML = themes.map(theme => `
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
  }

  protected async onWidgetCleanup(): Promise<void> {
    // KEEP theme CSS in document.head - it should persist across tab changes
    // Just clear our reference so we can re-acquire it on next init
    this.themeStyleElement = null;

    this.verbose() && console.log('✅ ThemeWidget: Cleanup complete (theme CSS preserved in document.head)');
  }

  /**
   * Switch theme - API for external control
   */
  async setTheme(themeName: string): Promise<void> {
    this.verbose() && console.log(`🎨 ThemeWidget: Switching to theme '${themeName}'`);
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

      this.verbose() && console.log('✅ ThemeWidget: Theme switched, injected globally, and saved to UserState');

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
      this.verbose() && console.log('🎨 ThemeWidget: Injecting theme CSS into document head...');

      // Check for existing theme style elements in DOM (may exist from previous widget instance)
      const existingStyles = document.querySelectorAll('style[id^="jtag-theme-"]');
      existingStyles.forEach(el => el.remove());

      // Create new theme style element
      this.themeStyleElement = document.createElement('style');
      this.themeStyleElement.id = `jtag-theme-${this.currentTheme}`;
      this.themeStyleElement.textContent = combinedCSS;

      document.head.appendChild(this.themeStyleElement);

      this.verbose() && console.log(`✅ ThemeWidget: Theme '${this.currentTheme}' CSS injected (${combinedCSS.length} chars)`);

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
      this.verbose() && console.log('🎨 ThemeWidget: Loading ALL theme CSS (base + theme)');

      // Load base CSS files from themes/base/
      const baseStyles = await this.loadDirectoryStyles('base');

      // Load theme-specific CSS files from themes/current-theme-name/
      const themeStyles = this.currentTheme !== 'base'
        ? await this.loadDirectoryStyles(this.currentTheme)
        : '';

      // Combine base + theme styles
      const combinedCSS = baseStyles + themeStyles;

      this.verbose() && console.log(`✅ ThemeWidget: Combined theme CSS loaded (${combinedCSS.length} chars)`);
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
          this.verbose() && console.log(`🎨 ThemeWidget: Loading ${filePath} via BaseWidget executeCommand`);

          const result = await Commands.execute<FileLoadParams, FileLoadResult>(FILE_COMMANDS.LOAD, {
            filepath: filePath
          });

          // Handle nested JTAG response structure (same as BaseWidget loadResource)
          const fileData = (result as FileLoadResult & { commandResult?: FileLoadResult }).commandResult ?? result;
          if (result.success && fileData.success && fileData.content) {
            combinedStyles += `\n/* === ${directoryName}/${fileName} === */\n${fileData.content}\n`;
            this.verbose() && console.log(`✅ ThemeWidget: Loaded ${directoryName}/${fileName} (${fileData.bytesRead} bytes)`);
          } else {
            this.verbose() && console.log(`⚠️ ThemeWidget: ${directoryName}/${fileName} not found - skipping`);
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
          this.verbose() && console.log(`🎨 ThemeWidget: Theme card clicked - switching to '${themeName}'`);
          // Use setTheme directly - it handles CSS loading, persistence to localStorage AND UserState
          await this.setTheme(themeName);
        }
      });
    });

    this.verbose() && console.log('✅ ThemeWidget: Theme controls set up successfully');
  }

  /**
   * Save theme preference to UserState (initialized by JTAGClient during connection)
   */
  private async saveThemeToUserState(themeName: string): Promise<void> {
    try {
      this.verbose() && console.log(`🔧 ThemeWidget: Saving theme '${themeName}' to UserState`);

      // 1. Save to localStorage immediately for instant persistence
      if (LocalStorageStateManager.isAvailable()) {
        const success = LocalStorageStateManager.setTheme(themeName);
        if (success) {
          this.verbose() && console.log(`✅ ThemeWidget: Theme '${themeName}' saved to localStorage`);
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

      this.verbose() && console.log(`🔧 ThemeWidget: Updating UserState ${userStateId.substring(0, 8)}...`);

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

      this.verbose() && console.log(`✅ ThemeWidget: Theme '${themeName}' saved to UserState`);


    } catch (error) {
      console.error('❌ ThemeWidget: Failed to save theme using hybrid persistence:', error);
    }
  }

  /**
   * Load theme preference using hybrid persistence (localStorage first, then UserState)
   */
  private async loadThemeFromUserState(): Promise<string | null> {
    try {
      this.verbose() && console.log('🔧 ThemeWidget: Loading theme using hybrid persistence');

      // 1. Try localStorage first for instant response
      if (LocalStorageStateManager.isAvailable()) {
        const localTheme = LocalStorageStateManager.getTheme();
        if (localTheme) {
          this.verbose() && console.log(`✅ ThemeWidget: Loaded theme '${localTheme}' from localStorage`);
          return localTheme;
        }
        this.verbose() && console.log('ℹ️ ThemeWidget: No theme found in localStorage, trying UserState');
      }

      // 2. Fall back to UserState from localStorage for persistence
      // Get persistent device identity (encrypted in localStorage)
      const { BrowserDeviceIdentity } = await import('../../system/core/browser/BrowserDeviceIdentity');
      const identity = await BrowserDeviceIdentity.getOrCreateIdentity();

      this.verbose() && console.log(`🔧 ThemeWidget: Loading theme for device ${identity.deviceId.substring(0, 12)}...`);

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
          this.verbose() && console.log(`✅ ThemeWidget: Loaded theme '${savedTheme}' from UserState database`);

          // Sync back to localStorage for faster future access
          if (LocalStorageStateManager.isAvailable()) {
            LocalStorageStateManager.setTheme(savedTheme);
            this.verbose() && console.log(`🔄 ThemeWidget: Synced theme '${savedTheme}' to localStorage`);
          }

          return savedTheme;
        }
      }

      this.verbose() && console.log('ℹ️ ThemeWidget: No saved theme found in either localStorage or UserState');
      return null;

    } catch (error) {
      console.error('❌ ThemeWidget: Failed to load theme from UserState:', error);
      return null;
    }
  }
}

// Registration handled by centralized BROWSER_WIDGETS registry
