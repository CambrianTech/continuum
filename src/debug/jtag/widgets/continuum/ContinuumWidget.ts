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
import { LocalStorageStateManager } from '../../system/core/browser/LocalStorageStateManager';
import { ThemeRegistry } from '../shared/themes/ThemeTypes';

export class ContinuumWidget extends BaseWidget {
  private currentStatus: ContinuumStatus | null = null;
  private currentTheme: string = 'base';

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

    // Load saved theme from localStorage FIRST
    const savedTheme = LocalStorageStateManager.isAvailable()
      ? LocalStorageStateManager.getTheme()
      : null;
    this.currentTheme = savedTheme || 'base';
    console.log(`🎨 ContinuumWidget: Using theme '${this.currentTheme}' from localStorage`);

    // CRITICAL: Load and inject theme CSS FIRST, before any child widgets render
    // This ensures CSS variables are available when sidebar, room-list, etc. initialize
    const themeCSS = await this.loadThemeCSS();
    await this.injectThemeIntoDocumentHead(themeCSS);

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

    // Theme CSS already injected in onWidgetInitialize() - no need to reload here

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

    // Set up expand button click handlers
    this.setupExpandButtons();

    console.log('✅ ContinuumWidget: Desktop interface rendered');
  }

  /**
   * Set up click handlers for panel expand buttons
   * These buttons are in ContinuumWidget's shadow DOM (not nested shadow DOMs)
   * so they can properly overlay everything
   */
  private setupExpandButtons(): void {
    const leftExpand = this.shadowRoot?.querySelector('.left-expand');
    const rightExpand = this.shadowRoot?.querySelector('.right-expand');

    if (leftExpand) {
      leftExpand.addEventListener('click', () => {
        const resizer = this.shadowRoot?.querySelector('panel-resizer[side="left"]') as any;
        resizer?.expand?.();
      });
    }

    if (rightExpand) {
      rightExpand.addEventListener('click', () => {
        const resizer = this.shadowRoot?.querySelector('panel-resizer[side="right"]') as any;
        resizer?.expand?.();
      });
    }
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
   * Loads base CSS first, then overlays current theme if not 'base'
   */
  private async loadThemeCSS(): Promise<string> {
    try {
      // Always load base CSS first (provides layout and default variables)
      const [baseLayoutResult, baseThemeResult] = await Promise.all([
        Commands.execute<FileLoadParams, FileLoadResult>(FILE_COMMANDS.LOAD, {
          filepath: 'widgets/shared/themes/base/base.css'
        }),
        Commands.execute<FileLoadParams, FileLoadResult>(FILE_COMMANDS.LOAD, {
          filepath: 'widgets/shared/themes/base/theme.css'
        })
      ]);

      const baseLayoutCss = baseLayoutResult.content ?? '/* Base layout CSS not found */';
      const baseThemeCss = baseThemeResult.content ?? '/* Base theme CSS not found */';

      // Combine base CSS - theme variables first, then layout
      let combinedCss = baseThemeCss + '\n' + baseLayoutCss;

      // If current theme is not 'base', load and overlay its CSS
      if (this.currentTheme !== 'base') {
        console.log(`🎨 ContinuumWidget: Loading theme '${this.currentTheme}' CSS overlay`);

        // Get theme manifest for file list
        const themeManifest = ThemeRegistry.getTheme(this.currentTheme);
        const themeFiles = themeManifest?.files ?? ['theme.css'];

        for (const fileName of themeFiles) {
          try {
            const themeFileResult = await Commands.execute<FileLoadParams, FileLoadResult>(FILE_COMMANDS.LOAD, {
              filepath: `widgets/shared/themes/${this.currentTheme}/${fileName}`
            });
            if (themeFileResult.content) {
              combinedCss += `\n/* === ${this.currentTheme}/${fileName} === */\n${themeFileResult.content}`;
            }
          } catch (err) {
            console.warn(`⚠️ ContinuumWidget: Could not load ${this.currentTheme}/${fileName}:`, err);
          }
        }
      }

      console.log(`🎨 ContinuumWidget: Loaded theme '${this.currentTheme}' CSS (${combinedCss.length} chars)`);
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
    return this.currentTheme;
  }


  /**
   * Inject theme CSS into document head for global widget access (copied from ThemeWidget)
   */
  private async injectThemeIntoDocumentHead(combinedCSS: string): Promise<void> {
    try {
      console.log(`🎨 ContinuumWidget: Injecting theme '${this.currentTheme}' CSS into document head...`);

      // Remove any existing theme style elements
      const existingStyles = document.head.querySelectorAll('style[id^="jtag-theme-"]');
      existingStyles.forEach(el => el.remove());

      // Create new theme style element and inject into document head
      const themeStyleElement = document.createElement('style');
      themeStyleElement.id = `jtag-theme-${this.currentTheme}`;
      themeStyleElement.textContent = combinedCSS;

      document.head.appendChild(themeStyleElement);

      console.log(`✅ ContinuumWidget: Theme '${this.currentTheme}' CSS injected (${combinedCSS.length} chars)`);

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