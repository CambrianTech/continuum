/**
 * ThemeWidget - Centralized theme management for all widgets
 * 
 * Single responsibility: Load and inject theme CSS into document head
 * All other widgets just consume the CSS custom properties
 * Extends BaseWidget to reuse existing CSS loading infrastructure
 */

import { BaseWidget } from './BaseWidget';
import type { FileLoadParams, FileLoadResult } from '../../commands/file/load/shared/FileLoadTypes';
import { ThemeDiscoveryService } from './themes/ThemeDiscoveryService';
import { ThemeRegistry, ThemeManifest } from './themes/ThemeTypes';

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
    this.themeDiscovery = new ThemeDiscoveryService(this);
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
      }
    } catch (error) {
      console.error('❌ ThemeWidget: Theme discovery failed:', error);
    }
    
    console.log('✅ ThemeWidget: BaseWidget initialization complete');
  }

  protected async renderWidget(): Promise<void> {
    console.log('🎨 ThemeWidget: renderWidget() called - using BaseWidget template system');
    
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
      
      // Re-render widget to show updated theme name
      await this.renderWidget();
      
      console.log('✅ ThemeWidget: Theme switched and injected globally');
      
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
          // Use BaseWidget's protected jtagOperation method - same as loadResource does internally
          const filePath = `widgets/shared/themes/${directoryName}/${fileName}`;
          console.log(`🎨 ThemeWidget: Loading ${filePath} via BaseWidget jtagOperation`);
          
          const result = await this.jtagOperation<FileLoadResult>('file/load', {
            filepath: filePath
          });
          
          // Handle nested JTAG response structure (same as BaseWidget loadResource)
          const fileData = (result as any).commandResult || result;
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
          // Get JTAG client if available (for command execution)
          if (typeof window !== 'undefined' && (window as any).jtagSystem) {
            await (window as any).jtagSystem.commands.themeSet({ themeName: selectedTheme });
            console.log(`✅ ThemeWidget: Successfully applied theme '${selectedTheme}' via JTAG command`);
          } else {
            // Fallback to internal method if JTAG not available
            await this.setTheme(selectedTheme);
            console.log(`✅ ThemeWidget: Applied theme '${selectedTheme}' via internal method`);
          }
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
}

// Register the custom element
// Registration handled by centralized BROWSER_WIDGETS registry