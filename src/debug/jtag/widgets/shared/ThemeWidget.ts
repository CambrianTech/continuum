/**
 * ThemeWidget - Centralized theme management for all widgets
 * 
 * Single responsibility: Load and inject theme CSS into document head
 * All other widgets just consume the CSS custom properties
 * Extends BaseWidget to reuse existing CSS loading infrastructure
 */

import { BaseWidget } from './BaseWidget';
import type { FileLoadParams, FileLoadResult } from '../../commands/file/load/shared/FileLoadTypes';

export class ThemeWidget extends BaseWidget {
  private currentTheme: string = 'base';
  private themeStyleElement: HTMLStyleElement | null = null;

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
  }

  protected async onWidgetInitialize(): Promise<void> {
    console.log('🎨 ThemeWidget: Initializing with BaseWidget infrastructure...');
    
    // Load all theme CSS and inject into DOCUMENT HEAD (not shadow DOM) 
    // so CSS custom properties can be accessed by all widgets
    try {
      const combinedCSS = await this.loadAllThemeCSS();
      
      // Inject theme CSS into document head for global access
      await this.injectThemeIntoDocumentHead(combinedCSS);
      
      // Still keep widget's own CSS in templateCSS for the widget itself
      console.log('✅ ThemeWidget: Theme CSS injected into document head for global widget access');
    } catch (error) {
      console.error('❌ ThemeWidget: Failed to load and inject theme CSS:', error);
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
    
    // Set up theme switching event handlers
    this.setupThemeControls();
    
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
   * Load and inject theme CSS into document head
   * Loads from themes/base/ directory + themes/current-theme-name/ directory
   */
  private async loadTheme(themeName: string): Promise<void> {
    try {
      // Remove existing theme
      if (this.themeStyleElement) {
        this.themeStyleElement.remove();
      }

      console.log(`🎨 ThemeWidget: Loading base styles + theme '${themeName}'`);

      // Load base CSS files from themes/base/
      const baseStyles = await this.loadDirectoryStyles('base');
      
      // Load theme-specific CSS files from themes/current-theme-name/
      const themeStyles = themeName !== 'base' 
        ? await this.loadDirectoryStyles(themeName)
        : '';
      
      // Combine base + theme styles
      const combinedCSS = baseStyles + themeStyles;
      
      // Inject into document head
      this.themeStyleElement = document.createElement('style');
      this.themeStyleElement.id = `jtag-theme-${themeName}`;
      this.themeStyleElement.textContent = combinedCSS;
      document.head.appendChild(this.themeStyleElement);
      
      console.log(`✅ ThemeWidget: Theme '${themeName}' loaded and injected`);
      
      // Dispatch theme change event
      this.dispatchEvent(new CustomEvent('theme-changed', {
        detail: { themeName },
        bubbles: true
      }));
      
    } catch (error) {
      console.error(`❌ ThemeWidget: Failed to load theme '${themeName}':`, error);
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
   * Get list of CSS files in a theme directory
   * For now, use known file names. In future could fetch from server API
   */
  private async getDirectoryFiles(directoryName: string): Promise<string[]> {
    // Consistent naming convention: each theme has a theme.css file
    // Base theme also includes base.css for foundational styles
    const knownFiles: Record<string, string[]> = {
      'base': ['base.css', 'theme.css'],
      'light': ['theme.css'],
      'cyberpunk': ['theme.css'],
      'retro-mac': ['theme.css'],
      'monochrome': ['theme.css'],
      'classic': ['theme.css']
    };

    // Return known files for directory, or fallback to standard theme.css
    return knownFiles[directoryName] || ['theme.css'];
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
    // Return all available theme variants
    return ['base', 'light', 'cyberpunk', 'retro-mac', 'monochrome', 'classic'];
  }

  /**
   * Set up theme switching controls and event handlers
   */
  private setupThemeControls(): void {
    const themeSelector = this.shadowRoot?.querySelector('#theme-selector') as HTMLSelectElement;
    const applyButton = this.shadowRoot?.querySelector('#apply-theme') as HTMLButtonElement;
    
    if (!themeSelector || !applyButton) {
      console.warn('🎨 ThemeWidget: Theme controls not found in shadow DOM');
      return;
    }
    
    // Set current theme as selected
    themeSelector.value = this.currentTheme;
    
    // Handle Apply button click
    applyButton.addEventListener('click', async () => {
      const selectedTheme = themeSelector.value;
      if (selectedTheme && selectedTheme !== this.currentTheme) {
        console.log(`🎨 ThemeWidget: Applying theme switch from '${this.currentTheme}' to '${selectedTheme}'`);
        await this.setTheme(selectedTheme);
      }
    });
    
    // Handle dropdown change (immediate apply)
    themeSelector.addEventListener('change', async (event) => {
      const selectedTheme = (event.target as HTMLSelectElement).value;
      if (selectedTheme && selectedTheme !== this.currentTheme) {
        console.log(`🎨 ThemeWidget: Auto-applying theme switch to '${selectedTheme}'`);
        await this.setTheme(selectedTheme);
      }
    });
    
    console.log('✅ ThemeWidget: Theme controls set up successfully');
  }
}

// Register the custom element
customElements.define('theme-widget', ThemeWidget);