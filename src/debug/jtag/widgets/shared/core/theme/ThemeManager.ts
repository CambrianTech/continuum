/**
 * Theme Management Module
 * 
 * Handles all theme-related operations for BaseWidget:
 * - Theme loading and injection
 * - CSS custom property management
 * - Theme validation
 * - State persistence
 */

import { CSSCustomPropertyValidator, CSSValidationOptions } from '../../utils/CSSValidationUtils';
import type { FileLoadResult } from '../../../../commands/file/load/shared/FileLoadTypes';

export interface ThemeConfig {
  defaultTheme: string;
  enableValidation: boolean;
  enableDebugging: boolean;
  enablePersistence: boolean;
}

export interface ThemeState {
  currentTheme: string;
  isApplied: boolean;
  lastValidation?: Date;
  customProperties: Record<string, string>;
}

export interface ThemeOperations {
  loadTheme: (themeName: string) => Promise<string>;
  validateTheme: (themeCSS: string, widgetCSS: string) => Promise<boolean>;
}

export class ThemeManager {
  private config: ThemeConfig;
  private state: ThemeState;
  private operations: ThemeOperations;
  private widgetName: string;

  constructor(
    widgetName: string,
    config: Partial<ThemeConfig> = {},
    operations: ThemeOperations
  ) {
    this.widgetName = widgetName;
    this.operations = operations;
    
    this.config = {
      defaultTheme: 'cyberpunk',
      enableValidation: true,
      enableDebugging: false,
      enablePersistence: true,
      ...config
    };

    this.state = {
      currentTheme: this.config.defaultTheme,
      isApplied: false,
      customProperties: {}
    };
  }

  /**
   * Apply theme with validation and injection
   */
  async applyTheme(
    themeName: string, 
    customProperties?: Record<string, string>
  ): Promise<boolean> {
    try {
      console.log(`🎨 ${this.widgetName}: Applying theme '${themeName}'...`);
      
      // 1. Load theme CSS
      const themeCSS = await this.operations.loadTheme(themeName);
      
      if (!themeCSS || themeCSS === '/* No theme loaded */') {
        console.warn(`⚠️ ${this.widgetName}: Theme '${themeName}' not found, using default`);
        return false;
      }
      
      // 2. Validate theme if enabled (load widget CSS for comparison)
      if (this.config.enableValidation) {
        await this.validateThemeCompleteness(themeCSS);
      }
      
      // 3. Inject theme CSS into document
      this.injectThemeIntoDocument(themeName, themeCSS);
      
      // 4. Apply custom properties if provided
      if (customProperties) {
        this.applyCustomCSSProperties(customProperties);
        this.state.customProperties = { ...this.state.customProperties, ...customProperties };
      }
      
      // 5. Update state
      this.state.currentTheme = themeName;
      this.state.isApplied = true;
      this.state.lastValidation = new Date();
      
      console.log(`✅ ${this.widgetName}: Theme '${themeName}' applied successfully`);
      return true;
      
    } catch (error) {
      console.error(`❌ ${this.widgetName}: Theme application failed:`, error);
      return false;
    }
  }

  /**
   * Validate theme completeness against widget CSS requirements
   */
  private async validateThemeCompleteness(themeCSS: string): Promise<void> {
    try {
      // This would need to be passed the widget CSS content
      // For now, we'll validate just the theme CSS structure
      const validationOptions: CSSValidationOptions = {
        enableWarnings: this.config.enableValidation,
        enableDebugging: this.config.enableDebugging,
        widgetName: this.widgetName,
        themeName: this.state.currentTheme
      };

      // Note: We need widget CSS to do full validation
      // This is a simplified validation of theme structure
      const definedProperties = CSSCustomPropertyValidator.extractCustomProperties(themeCSS);
      
      if (this.config.enableDebugging) {
        console.log(`🔍 ${this.widgetName}: Theme defines ${definedProperties.length} CSS custom properties`);
        console.log(`   Properties: ${definedProperties.slice(0, 5).join(', ')}${definedProperties.length > 5 ? ` (+${definedProperties.length - 5} more)` : ''}`);
      }

    } catch (error) {
      console.error(`❌ ${this.widgetName}: Theme validation failed:`, error);
    }
  }

  /**
   * Inject theme CSS into document head for site-wide theming
   */
  private injectThemeIntoDocument(themeName: string, themeCSS: string): void {
    const themeId = `jtag-theme-${themeName}`;
    
    // Remove existing theme if present
    const existingTheme = document.getElementById(themeId);
    if (existingTheme) {
      existingTheme.remove();
    }
    
    // Create and inject new theme style element
    const styleElement = document.createElement('style');
    styleElement.id = themeId;
    styleElement.textContent = themeCSS;
    document.head.appendChild(styleElement);
    
    console.log(`🎨 ${this.widgetName}: Theme CSS injected into document head`);
  }

  /**
   * Apply custom CSS properties to document root for site-wide theming
   */
  private applyCustomCSSProperties(customProperties: Record<string, string>): void {
    const documentStyle = document.documentElement.style;
    
    for (const [property, value] of Object.entries(customProperties)) {
      // Ensure property starts with --
      const cssProperty = property.startsWith('--') ? property : `--${property}`;
      documentStyle.setProperty(cssProperty, value);
    }
    
    console.log(`🎨 ${this.widgetName}: Applied ${Object.keys(customProperties).length} custom CSS properties`);
  }

  /**
   * Get current theme state
   */
  getThemeState(): ThemeState {
    return { ...this.state };
  }

  /**
   * Update theme configuration
   */
  updateConfig(newConfig: Partial<ThemeConfig>): void {
    this.config = { ...this.config, ...newConfig };
  }

  /**
   * Check if theme is currently applied
   */
  isThemeApplied(): boolean {
    return this.state.isApplied;
  }

  /**
   * Get currently applied theme name
   */
  getCurrentTheme(): string {
    return this.state.currentTheme;
  }

  /**
   * Clear current theme (remove from document)
   */
  clearTheme(): void {
    const themeId = `jtag-theme-${this.state.currentTheme}`;
    const existingTheme = document.getElementById(themeId);
    if (existingTheme) {
      existingTheme.remove();
      console.log(`🗑️ ${this.widgetName}: Removed theme '${this.state.currentTheme}' from document`);
    }
    
    this.state.isApplied = false;
  }
}