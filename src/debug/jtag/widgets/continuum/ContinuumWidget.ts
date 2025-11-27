/**
 * ContinuumWidget - Main Desktop Interface Widget
 * 
 * Encompasses the entire JTAG desktop layout including sidebar and main content area.
 * This is the top-level widget that contains all other interface components.
 */

import { BaseWidget } from '../shared/BaseWidget';
import { Commands } from '../../system/core/shared/Commands';
import { Events } from '../../system/core/shared/Events';
import { FILE_COMMANDS } from '../../commands/file/shared/FileCommandConstants';
import type { FileLoadParams, FileLoadResult } from '../../commands/file/load/shared/FileLoadTypes';
import type { ContinuumStatus } from '../../commands/continuum/set/shared/ContinuumSetTypes';

export class ContinuumWidget extends BaseWidget {
  private currentStatus: ContinuumStatus | null = null;

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

    // Set initial favicon (default blue dot)
    this.updateFavicon();

    // Load external scripts into shadow DOM for complete encapsulation
    await this.loadExternalScripts();

    // Listen for continuum:status events from continuum/set command
    Events.subscribe('continuum:status', (status: ContinuumStatus) => {
      this.handleStatusUpdate(status);
    });

    console.log('✅ ContinuumWidget: Desktop interface initialized with status listener');
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
        Commands.execute<FileLoadParams, FileLoadResult>(FILE_COMMANDS.LOAD, {
          filepath: 'widgets/shared/themes/base/base.css'
        }),
        Commands.execute<FileLoadParams, FileLoadResult>(FILE_COMMANDS.LOAD, {
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

  /**
   * Handle continuum:status events from continuum/set command
   */
  private handleStatusUpdate(status: ContinuumStatus): void {
    console.log('✨ ContinuumWidget: Received status update:', status);

    // Handle clear request
    if (status.clear) {
      this.currentStatus = null;
      this.updateStatusDisplay();
      console.log('🔄 ContinuumWidget: Status cleared, returning to system default');
      return;
    }

    // Check priority - only override if new status has higher or equal priority
    if (this.currentStatus && this.getPriorityLevel(status.priority) < this.getPriorityLevel(this.currentStatus.priority)) {
      console.log(`⚠️ ContinuumWidget: Ignoring lower priority status (${status.priority} < ${this.currentStatus.priority})`);
      return;
    }

    // Store new status
    this.currentStatus = status;
    this.updateStatusDisplay();
  }

  /**
   * Update the visual display of the status in the widget
   * Note: Visual status is shown by ContinuumEmoterWidget in sidebar
   * ContinuumWidget only needs to update the favicon
   */
  private updateStatusDisplay(): void {
    // Update favicon to match current status (or ground state if null)
    this.updateFavicon();
    console.log('✅ ContinuumWidget: Favicon updated');
  }

  /**
   * Convert priority string to numeric level for comparison
   */
  private getPriorityLevel(priority: 'low' | 'medium' | 'high' | 'critical'): number {
    const levels: Record<string, number> = {
      low: 1,
      medium: 2,
      high: 3,
      critical: 4
    };
    return levels[priority] ?? 2;  // Default to medium
  }

  /**
   * Update browser favicon to match current status
   * Creates a dynamic favicon with colored dot + optional emoji
   */
  private updateFavicon(): void {
    try {
      // Determine favicon content based on current status
      const color = this.currentStatus?.color || 'var(--color-system, #0066cc)';
      const emoji = this.currentStatus?.emoji || '';

      // Create canvas for favicon
      const canvas = document.createElement('canvas');
      canvas.width = 32;
      canvas.height = 32;
      const ctx = canvas.getContext('2d');

      if (!ctx) {
        console.warn('⚠️ ContinuumWidget: Cannot create canvas context for favicon');
        return;
      }

      // Parse CSS color if needed (handle var(--color-system) case)
      let faviconColor = color;
      if (color.startsWith('var(')) {
        // Default to blue for system color
        faviconColor = '#0066cc';
      }

      if (emoji) {
        // Draw emoji as favicon
        ctx.font = '28px Arial';
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';
        ctx.fillText(emoji, 16, 16);
      } else {
        // Draw colored dot as favicon (HAL 9000 style)
        ctx.fillStyle = faviconColor;
        ctx.beginPath();
        ctx.arc(16, 16, 12, 0, Math.PI * 2);
        ctx.fill();

        // Add glow effect
        ctx.shadowBlur = 8;
        ctx.shadowColor = faviconColor;
        ctx.fill();
      }

      // Convert canvas to data URL
      const faviconUrl = canvas.toDataURL('image/png');

      // Update favicon link in document head
      let faviconLink = document.querySelector('link[rel="icon"]') as HTMLLinkElement;
      if (!faviconLink) {
        faviconLink = document.createElement('link');
        faviconLink.rel = 'icon';
        document.head.appendChild(faviconLink);
      }
      faviconLink.href = faviconUrl;

      console.log(`🎨 ContinuumWidget: Favicon updated (${emoji || 'dot'}, ${faviconColor})`);
    } catch (error) {
      console.error('❌ ContinuumWidget: Failed to update favicon:', error);
    }
  }
}

// Registration handled by centralized BROWSER_WIDGETS registry