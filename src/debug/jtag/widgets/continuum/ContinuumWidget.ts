/**
 * ContinuumWidget - Main Desktop Interface Widget
 * 
 * Encompasses the entire JTAG desktop layout including sidebar and main content area.
 * This is the top-level widget that contains all other interface components.
 */

import { BaseWidget } from '../shared/BaseWidget';
import { Commands } from '../../system/core/client/shared/Commands';
import type { FileLoadParams, FileLoadResult } from '../../commands/file/load/shared/FileLoadTypes';

export class ContinuumWidget extends BaseWidget {
  
  constructor() {
    super({
      widgetName: 'ContinuumWidget',
      template: 'continuum-widget.html',
      styles: 'continuum-widget.css',
      enableAI: false,
      enableDatabase: false, 
      enableRouterEvents: true,
      enableScreenshots: false
    });
  }

  protected async onWidgetInitialize(): Promise<void> {
    console.log('🌐 ContinuumWidget: Initializing main desktop interface...');
    
    // Initialize any dynamic content or state
    await this.updateConnectionStatus();
    
    // Load external scripts into shadow DOM for complete encapsulation
    await this.loadExternalScripts();
    
    console.log('✅ ContinuumWidget: Desktop interface initialized');
  }

  protected async renderWidget(): Promise<void> {
    // Use BaseWidget's template and styles system
    const styles = this.templateCSS ?? '/* No styles loaded */';
    const template = this.templateHTML ?? '<div>No template loaded</div>';

    // Load theme CSS and inject into document head for global access (like ThemeWidget does)
    const themeCSS = await this.loadThemeCSS();
    await this.injectThemeIntoDocumentHead(themeCSS);

    // Ensure template is a string
    const templateString = typeof template === 'string' ? template : '<div>Template error</div>';

    // Replace any dynamic content
    const dynamicContent = templateString
      .replace('<!-- CONNECTION_STATUS -->', await this.getConnectionStatusHTML())
      .replace('<!-- CURRENT_TIMESTAMP -->', new Date().toLocaleTimeString());

    this.shadowRoot.innerHTML = `
      <style>${styles}</style>
      ${dynamicContent}
    `;

    console.log('✅ ContinuumWidget: Desktop interface rendered with theme CSS injected into document head');
  }

  protected async onWidgetCleanup(): Promise<void> {
    console.log('🧹 ContinuumWidget: Cleanup complete');
  }

  /**
   * Update connection status display
   */
  private async updateConnectionStatus(): Promise<void> {
    // This could connect to system status, for now just show connected
    console.log('🔗 ContinuumWidget: Connection status updated');
  }

  /**
   * Get connection status HTML
   */
  private async getConnectionStatusHTML(): Promise<string> {
    return '<div id="connection-status" class="status connected">CONNECTED</div>';
  }

  /**
   * Load theme CSS for shadow DOM injection
   */
  private async loadThemeCSS(): Promise<string> {
    try {
      // Load both base layout CSS and theme variables CSS
      const [baseLayoutResult, themeVariablesResult] = await Promise.all([
        Commands.execute<FileLoadParams, FileLoadResult>('file/load', {
          filepath: 'widgets/shared/themes/base/base.css'
        }),
        Commands.execute<FileLoadParams, FileLoadResult>('file/load', {
          filepath: 'widgets/shared/themes/base/theme.css'
        })
      ]);

      // Extract content directly from FileLoadResult
      const baseLayoutCss = baseLayoutResult.content ?? '/* Base layout CSS not found */';
      const themeVariablesCss = themeVariablesResult.content ?? '/* Theme variables CSS not found */';

      // Combine both CSS files - theme variables first, then layout
      const combinedCss = themeVariablesCss + '\n' + baseLayoutCss;

      console.log('🎨 ContinuumWidget: Loaded base theme CSS variables and layout for shadow DOM injection');
      return combinedCss;
    } catch (error) {
      console.error('❌ ContinuumWidget: Failed to load theme CSS:', error);
      return '/* Theme CSS loading failed */';
    }
  }

  /**
   * Get current theme name for theme widget
   */
  getCurrentTheme(): string {
    return 'base';
  }


  /**
   * Inject theme CSS into document head for global widget access (copied from ThemeWidget)
   */
  private async injectThemeIntoDocumentHead(combinedCSS: string): Promise<void> {
    try {
      console.log('🎨 ContinuumWidget: Injecting theme CSS into document head for global access...');

      // Remove existing theme style element
      const existingTheme = document.head.querySelector('#jtag-theme-base');
      if (existingTheme) {
        existingTheme.remove();
      }

      // Create new theme style element and inject into document head
      const themeStyleElement = document.createElement('style');
      themeStyleElement.id = 'jtag-theme-base';
      themeStyleElement.textContent = combinedCSS;

      document.head.appendChild(themeStyleElement);

      console.log(`✅ ContinuumWidget: Base theme CSS injected into document head (${combinedCSS.length} chars)`);

    } catch (error) {
      console.error('❌ ContinuumWidget: Failed to inject theme CSS into document head:', error);
    }
  }

  /**
   * Load external scripts into shadow DOM for complete encapsulation
   */
  private async loadExternalScripts(): Promise<void> {
    try {
      // Create script element for html2canvas
      const html2canvasScript = document.createElement('script');
      html2canvasScript.src = 'https://cdn.jsdelivr.net/npm/html2canvas@1.4.1/dist/html2canvas.min.js';
      html2canvasScript.async = true;
      
      // Add script to shadow DOM
      this.shadowRoot.appendChild(html2canvasScript);
      
      // Wait for script to load
      await new Promise<void>((resolve, reject) => {
        html2canvasScript.onload = () => {
          console.log('📸 ContinuumWidget: html2canvas script loaded into shadow DOM');
          resolve();
        };
        html2canvasScript.onerror = () => {
          console.error('❌ ContinuumWidget: Failed to load html2canvas script');
          reject(new Error('Failed to load html2canvas'));
        };
      });
    } catch (error) {
      console.error('❌ ContinuumWidget: Failed to load external scripts:', error);
    }
  }

  /**
   * List available rooms/channels for sidebar
   */
  async getAvailableRooms(): Promise<string[]> {
    return ['general', 'academy', 'community'];
  }
}

// Registration handled by centralized BROWSER_WIDGETS registry