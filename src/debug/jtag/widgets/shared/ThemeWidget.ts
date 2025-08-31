/**
 * ThemeWidget - Centralized theme management for all widgets
 * 
 * Single responsibility: Load and inject theme CSS into document head
 * All other widgets just consume the CSS custom properties
 */

export class ThemeWidget extends HTMLElement {
  private currentTheme: string = 'base';
  private themeStyleElement: HTMLStyleElement | null = null;

  constructor() {
    super();
    this.loadTheme(this.currentTheme);
  }

  /**
   * Switch theme - API for external control
   */
  async setTheme(themeName: string): Promise<void> {
    console.log(`🎨 ThemeWidget: Switching to theme '${themeName}'`);
    this.currentTheme = themeName;
    await this.loadTheme(themeName);
  }

  /**
   * Load and inject theme CSS into document head
   */
  private async loadTheme(themeName: string): Promise<void> {
    try {
      // Remove existing theme
      if (this.themeStyleElement) {
        this.themeStyleElement.remove();
      }

      // Load theme CSS
      const response = await fetch(`/dist/widgets/shared/themes/${themeName}.css`);
      if (!response.ok) {
        throw new Error(`Failed to load theme: ${response.status}`);
      }
      
      const themeCSS = await response.text();
      
      // Inject into document head
      this.themeStyleElement = document.createElement('style');
      this.themeStyleElement.id = `jtag-theme-${themeName}`;
      this.themeStyleElement.textContent = themeCSS;
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
   * Get current theme name
   */
  getCurrentTheme(): string {
    return this.currentTheme;
  }

  /**
   * List available themes
   */
  async getAvailableThemes(): Promise<string[]> {
    // Could fetch from server or return static list
    return ['base', 'monochrome', 'classic'];
  }
}

// Register the custom element
customElements.define('theme-widget', ThemeWidget);