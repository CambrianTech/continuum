/**
 * ThemeWidget - Centralized theme management for all widgets
 * 
 * Single responsibility: Load and inject theme CSS into document head
 * All other widgets just consume the CSS custom properties
 * Extends BaseWidget to reuse existing CSS loading infrastructure
 */

import { BaseWidget } from './BaseWidget';
import type { FileLoadParams, FileLoadResult } from '../../commands/file/load/shared/FileLoadTypes';
import { Commands } from '../../system/core/shared/Commands';
import { FILE_COMMANDS } from '../../commands/file/shared/FileCommandConstants';
import { THEME_COMMANDS } from '../../commands/theme/shared/ThemeCommandConstants';
import { DATA_COMMANDS } from '../../commands/data/shared/DataCommandConstants';
import { ThemeDiscoveryService } from './themes/ThemeDiscoveryService';
import { ThemeRegistry } from './themes/ThemeTypes';
import type { DataListResult } from '../../commands/data/list/shared/DataListTypes';
import type { UserStateEntity } from '../../system/data/entities/UserStateEntity';
import { stringToUUID } from '../../system/core/types/CrossPlatformUUID';
import { LocalStorageStateManager } from '../../system/core/browser/LocalStorageStateManager';

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

  protected async onWidgetInitialize(): Promise<void> {
    console.log('🎨 ThemeWidget: Initializing with dynamic theme discovery...');

    try {
      // Discover all available themes dynamically (like widget/command discovery)
      await this.themeDiscovery.discoverThemes();

      // Load current theme using dynamic system
      const themeResult = await this.themeDiscovery.loadTheme(this.currentTheme);

      if (themeResult.success) {
        // Inject theme CSS into document head for global access
        await this.injectThemeIntoDocumentHead(themeResult.cssContent);
        console.log('✅ ThemeWidget: Dynamic theme system initialized successfully');
      } else {
        console.error('❌ ThemeWidget: Failed to load theme:', themeResult.error);
        // Fallback: try loading base theme manually if discovery fails
        console.log('🎨 ThemeWidget: Attempting fallback base theme loading...');
        await this.setTheme('base');
      }
    } catch (error) {
      console.error('❌ ThemeWidget: Theme discovery failed:', error);
      // Fallback: try loading base theme manually if discovery fails completely
      console.log('🎨 ThemeWidget: Attempting fallback base theme loading after error...');
      try {
        await this.setTheme('base');
      } catch (fallbackError) {
        console.error('❌ ThemeWidget: Even fallback theme loading failed:', fallbackError);
      }
    }

    console.log('✅ ThemeWidget: BaseWidget initialization complete');
  }

  protected async renderWidget(): Promise<void> {
    console.log('🎨 ThemeWidget: renderWidget() called - using BaseWidget template system');

    // Load saved theme from UserState or fallback to base theme
    if (!this.themeStyleElement) {
      console.log('🎨 ThemeWidget: Loading theme on initial render');
      try {
        const savedTheme = await this.loadThemeFromUserState();
        const themeToLoad = savedTheme || 'base';
        console.log(`🎨 ThemeWidget: Loading theme '${themeToLoad}' (saved: ${savedTheme})`);
        await this.setTheme(themeToLoad);
      } catch (error) {
        console.error('❌ ThemeWidget: Failed to load theme, falling back to base:', error);
        await this.setTheme('base');
      }
    }
    
    // Use external template and styles loaded by BaseWidget (like ChatWidget does)
    const styles = this.templateCSS || '/* No styles loaded */';
    const template = this.templateHTML || '<div>No template loaded</div>';
    
    // Ensure template is a string before processing
    const templateString = typeof template === 'string' ? template : '<div>Template error</div>';
    
    // Replace dynamic content in template
    const dynamicContent = templateString.replace(
      '<!-- Current theme name -->', 
      this.currentTheme
    );

    this.shadowRoot!.innerHTML = `
      <style>${styles}</style>
      ${dynamicContent}
    `;
    
    // Set up theme switching event handlers (now async for dynamic population)
    await this.setupThemeControls();
    
    console.log('✅ ThemeWidget: Rendered using BaseWidget template system like ChatWidget');
  }

  protected async onWidgetCleanup(): Promise<void> {
    // Remove theme CSS when widget is destroyed
    if (this.themeStyleElement) {
      this.themeStyleElement.remove();
      this.themeStyleElement = null;
    }
    console.log('✅ ThemeWidget: Cleanup complete');
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

      console.log('✅ ThemeWidget: Theme switched, injected globally, and saved to UserState');

    } catch (error) {
      console.error('❌ ThemeWidget: Failed to switch theme:', error);
    }
  }

  /**
   * Load and inject theme CSS into document head (DEPRECATED - use themeDiscovery.loadTheme)
   * This method is kept for compatibility but delegates to the dynamic discovery service
   */
  private async loadTheme(themeName: string): Promise<void> {
    console.log(`🎨 ThemeWidget: Loading theme '${themeName}' via dynamic discovery service`);
    
    try {
      // Use dynamic theme discovery service
      const themeResult = await this.themeDiscovery.loadTheme(themeName);
      
      if (themeResult.success) {
        // Inject theme CSS into document head for global access
        await this.injectThemeIntoDocumentHead(themeResult.cssContent);
        console.log(`✅ ThemeWidget: Theme '${themeName}' loaded via discovery service`);
      } else {
        console.error(`❌ ThemeWidget: Failed to load theme '${themeName}':`, themeResult.error);
      }
    } catch (error) {
      console.error(`❌ ThemeWidget: Error loading theme '${themeName}':`, error);
    }
  }

  /**
   * Inject theme CSS into document head for global widget access
   * Removes existing theme styles and adds new ones
   */
  private async injectThemeIntoDocumentHead(combinedCSS: string): Promise<void> {
    try {
      console.log('🎨 ThemeWidget: Injecting theme CSS into document head for global access...');
      console.log('🔧 CLAUDE-DEBUG: combinedCSS length:', combinedCSS.length);
      console.log('🔧 CLAUDE-DEBUG: combinedCSS first 200 chars:', combinedCSS.substring(0, 200));
      
      // Remove existing theme style element
      if (this.themeStyleElement) {
        console.log('🔧 CLAUDE-DEBUG: Removing existing theme style element:', this.themeStyleElement.id);
        this.themeStyleElement.remove();
        this.themeStyleElement = null;
      }
      
      // Create new theme style element and inject into document head
      this.themeStyleElement = document.createElement('style');
      this.themeStyleElement.id = `jtag-theme-${this.currentTheme}`;
      this.themeStyleElement.textContent = combinedCSS;
      
      console.log('🔧 CLAUDE-DEBUG: Created style element with id:', this.themeStyleElement.id);
      console.log('🔧 CLAUDE-DEBUG: About to append to document.head...');
      
      document.head.appendChild(this.themeStyleElement);
      
      // Verify the injection worked
      const verifyElement = document.head.querySelector(`#jtag-theme-${this.currentTheme}`);
      console.log('🔧 CLAUDE-DEBUG: Verification - element exists in document head:', !!verifyElement);
      console.log('🔧 CLAUDE-DEBUG: Verification - element content length:', verifyElement?.textContent?.length || 0);
      
      console.log(`✅ ThemeWidget: Theme '${this.currentTheme}' CSS injected into document head (${combinedCSS.length} chars)`);
      
      // Dispatch theme change event
      this.dispatchEvent(new CustomEvent('theme-changed', {
        detail: { themeName: this.currentTheme },
        bubbles: true
      }));
      
    } catch (error) {
      console.error('❌ ThemeWidget: Failed to inject theme CSS into document head:', error);
      console.error('🔧 CLAUDE-DEBUG: Error stack:', (error as Error).stack);
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
  private async setupThemeControls(): Promise<void> {
    const themeSelector = this.shadowRoot?.querySelector('#theme-selector') as HTMLSelectElement;
    const applyButton = this.shadowRoot?.querySelector('#apply-theme') as HTMLButtonElement;
    const cancelButton = this.shadowRoot?.querySelector('#cancel-theme') as HTMLButtonElement;
    
    if (!themeSelector || !applyButton) {
      console.warn('🎨 ThemeWidget: Theme controls not found in shadow DOM');
      return;
    }
    
    // Populate dropdown with discovered themes dynamically
    await this.populateThemeDropdown(themeSelector);
    
    // Set current theme as selected
    const originalTheme = this.currentTheme;
    themeSelector.value = this.currentTheme;
    
    // Handle Apply button click
    applyButton.addEventListener('click', async () => {
      const selectedTheme = themeSelector.value;
      if (selectedTheme) {
        console.log(`🎨 ThemeWidget: Applying theme '${selectedTheme}' (removing !== check due to dropdown preview)`);
        
        // Use the actual JTAG theme/set command for proper theme switching
        try {
          // Domain-owned: CommandDaemon handles theme switching with optimization
          await Commands.execute(THEME_COMMANDS.SET, {
            themeName: selectedTheme
          });
          console.log(`✅ ThemeWidget: Successfully applied theme '${selectedTheme}' via CommandDaemon`);
        } catch (error) {
          console.error(`❌ ThemeWidget: Failed to apply theme '${selectedTheme}':`, error);
          // Try fallback method
          await this.setTheme(selectedTheme);
        }
        
        // Dispatch custom event to signal theme was applied (for panel closing)
        this.dispatchEvent(new CustomEvent('theme-applied', {
          detail: { themeName: selectedTheme },
          bubbles: true
        }));
      }
    });
    
    // Handle Cancel button click
    if (cancelButton) {
      cancelButton.addEventListener('click', async () => {
        console.log(`🎨 ThemeWidget: Canceling theme changes, reverting to '${originalTheme}'`);
        
        // Revert dropdown to original theme
        themeSelector.value = originalTheme;
        
        // Revert theme if it was changed
        if (this.currentTheme !== originalTheme) {
          await this.setTheme(originalTheme);
        }
        
        // Close the theme modal if we're in one
        this.dispatchEvent(new CustomEvent('theme-cancelled', {
          detail: { originalTheme },
          bubbles: true
        }));
      });
    }
    
    // Handle dropdown change (immediate apply)
    themeSelector.addEventListener('change', async (event) => {
      const selectedTheme = (event.target as HTMLSelectElement).value;
      if (selectedTheme && selectedTheme !== this.currentTheme) {
        console.log(`🎨 ThemeWidget: Auto-applying theme switch to '${selectedTheme}'`);
        await this.setTheme(selectedTheme);
      }
    });
    
    console.log('✅ ThemeWidget: Theme controls set up successfully with Cancel button');
  }

  /**
   * Dynamically populate theme dropdown from discovered themes
   */
  private async populateThemeDropdown(themeSelector: HTMLSelectElement): Promise<void> {
    try {
      // Clear existing options
      themeSelector.innerHTML = '';
      
      // Get all discovered themes from registry
      const themes = ThemeRegistry.getAllThemes();
      
      // Create options for each theme
      themes.forEach(theme => {
        const option = document.createElement('option');
        option.value = theme.name;
        option.textContent = theme.displayName;
        option.title = theme.description;
        themeSelector.appendChild(option);
      });
      
      console.log(`🎨 ThemeWidget: Populated dropdown with ${themes.length} discovered themes`);
      
    } catch (error) {
      console.error('❌ ThemeWidget: Failed to populate theme dropdown:', error);
      // Fallback option if discovery fails
      const fallbackOption = document.createElement('option');
      fallbackOption.value = 'base';
      fallbackOption.textContent = 'Base Theme';
      themeSelector.appendChild(fallbackOption);
    }
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
      await Commands.execute(DATA_COMMANDS.UPDATE, {
        collection: 'UserState',
        id: userStateId,
        backend: 'browser',
        data: {
          preferences: {
            theme: themeName
          },
          updatedAt: new Date().toISOString()
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
      const userStates = await Commands.execute(DATA_COMMANDS.LIST, {
        collection: 'UserState',
        filter: {
          userId: identity.userId,
          deviceId: identity.deviceId
        },
        backend: 'browser', // Use localStorage backend
        orderBy: [{ field: 'updatedAt', direction: 'desc' }],
        limit: 1
      }) as DataListResult<UserStateEntity>;

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

// Register the custom element
// Registration handled by centralized BROWSER_WIDGETS registry