/**
 * ContinuumWidget - Main Desktop Interface Widget
 *
 * Encompasses the entire JTAG desktop layout including sidebar and main content area.
 * This is the top-level widget that contains all other interface components.
 *
 * Uses ReactiveWidget with Lit templates for efficient rendering.
 */

import {
  ReactiveWidget,
  html,
  reactive,
  unsafeCSS,
  type TemplateResult,
  type CSSResultGroup
} from '../shared/ReactiveWidget';
import { Commands } from '../../system/core/shared/Commands';
import { Events } from '../../system/core/shared/Events';
import { FILE_COMMANDS } from '../../commands/file/shared/FileCommandConstants';
import type { FileLoadParams, FileLoadResult } from '../../commands/file/load/shared/FileLoadTypes';
import type { ContinuumStatus } from '../../commands/continuum/set/shared/ContinuumSetTypes';
import { LocalStorageStateManager } from '../../system/core/browser/LocalStorageStateManager';
import { ThemeRegistry } from '../shared/themes/ThemeTypes';
import { positronicBridge } from '../../system/state/PositronicBridge';
import { styles as CONTINUUM_STYLES } from './public/continuum-widget.styles';

export class ContinuumWidget extends ReactiveWidget {
  // Static styles using compiled SCSS
  static override styles = [
    ReactiveWidget.styles,
    unsafeCSS(CONTINUUM_STYLES)
  ] as CSSResultGroup;

  // Reactive state
  @reactive() private currentStatus: ContinuumStatus | null = null;
  @reactive() private currentTheme: string = 'base';

  constructor() {
    super({
      widgetName: 'ContinuumWidget'
    });
  }

  // === LIFECYCLE ===

  protected override async onFirstRender(): Promise<void> {
    super.onFirstRender();
    this.log('Initializing main desktop interface...');

    // Load saved theme from localStorage FIRST
    const savedTheme = LocalStorageStateManager.isAvailable()
      ? LocalStorageStateManager.getTheme()
      : null;
    this.currentTheme = savedTheme || 'base';
    this.log(`Using theme '${this.currentTheme}' from localStorage`);

    // CRITICAL: Load and inject theme CSS FIRST, before any child widgets render
    // This ensures CSS variables are available when sidebar, room-list, etc. initialize
    const themeCSS = await this.loadThemeCSS();
    await this.injectThemeIntoDocumentHead(themeCSS);

    // Set initial favicon (default blue dot)
    this.updateFavicon();

    // Load external scripts (html2canvas)
    await this.loadExternalScripts();

    // Subscribe to continuum:status events
    this.createMountEffect(() => {
      const unsubscribe = Events.subscribe('continuum:status', (status: ContinuumStatus) => {
        this.handleStatusUpdate(status);
      });
      return () => unsubscribe();
    });

    // Initialize Positronic state bridge for AI context awareness
    this.log('Initializing PositronicBridge...');
    try {
      positronicBridge.initialize();
      // Expose on window for debugging
      (window as any).positronicBridge = positronicBridge;
      this.log('PositronicBridge initialized successfully');
    } catch (error) {
      console.error('🌐 ContinuumWidget: PositronicBridge initialization failed:', error);
    }

    this.log('Desktop interface initialized with status listener');
  }

  // === RENDER ===

  protected override renderContent(): TemplateResult {
    return html`
      <div class="desktop-container">
        <!-- Left Sidebar Panel with Resizer -->
        <div class="sidebar-container">
          <sidebar-widget class="left-sidebar"></sidebar-widget>
          <panel-resizer side="left" css-var-prefix="sidebar" container-class="sidebar-container"></panel-resizer>
        </div>

        <!-- Main Content Panel -->
        <main-widget></main-widget>

        <!-- Right Panel with Resizer (Assistant, Logs, Tools) -->
        <div class="right-panel-container">
          <panel-resizer side="right" css-var-prefix="right-panel" container-class="right-panel-container"></panel-resizer>
          <right-panel-widget></right-panel-widget>
        </div>

        <!-- Expand buttons - outside shadow DOMs for proper z-index -->
        <button class="panel-expand-btn left-expand" title="Expand sidebar" @click=${this.handleLeftExpand}>»</button>
        <button class="panel-expand-btn right-expand" title="Expand panel" @click=${this.handleRightExpand}>«</button>
      </div>

      <!-- AI Presence Layer - Positron Cursor (floats above everything) -->
      <positron-cursor-widget></positron-cursor-widget>
    `;
  }

  // === EVENT HANDLERS ===

  private handleLeftExpand = (): void => {
    const resizer = this.shadowRoot?.querySelector('panel-resizer[side="left"]') as any;
    resizer?.expand?.();
  };

  private handleRightExpand = (): void => {
    const resizer = this.shadowRoot?.querySelector('panel-resizer[side="right"]') as any;
    resizer?.expand?.();
  };

  // === THEME LOADING ===

  /**
   * Load theme CSS for document head injection
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
        this.log(`Loading theme '${this.currentTheme}' CSS overlay`);

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

      this.log(`Loaded theme '${this.currentTheme}' CSS (${combinedCss.length} chars)`);
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
   * Inject theme CSS into document head for global widget access
   */
  private async injectThemeIntoDocumentHead(combinedCSS: string): Promise<void> {
    try {
      this.log(`Injecting theme '${this.currentTheme}' CSS into document head...`);

      // Remove any existing theme style elements
      const existingStyles = document.head.querySelectorAll('style[id^="jtag-theme-"]');
      existingStyles.forEach(el => el.remove());

      // Create new theme style element and inject into document head
      const themeStyleElement = document.createElement('style');
      themeStyleElement.id = `jtag-theme-${this.currentTheme}`;
      themeStyleElement.textContent = combinedCSS;

      document.head.appendChild(themeStyleElement);

      this.log(`Theme '${this.currentTheme}' CSS injected (${combinedCSS.length} chars)`);
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
      this.shadowRoot?.appendChild(html2canvasScript);

      // Wait for script to load
      await new Promise<void>((resolve, reject) => {
        html2canvasScript.onload = () => {
          this.log('html2canvas script loaded into shadow DOM');
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

  // === STATUS HANDLING ===

  /**
   * Handle continuum:status events from continuum/set command
   */
  private handleStatusUpdate(status: ContinuumStatus): void {
    this.log('Received status update:', status);

    // Handle clear request
    if (status.clear) {
      this.currentStatus = null;
      this.updateFavicon();
      this.log('Status cleared, returning to system default');
      return;
    }

    // Check priority - only override if new status has higher or equal priority
    if (this.currentStatus && this.getPriorityLevel(status.priority) < this.getPriorityLevel(this.currentStatus.priority)) {
      this.log(`Ignoring lower priority status (${status.priority} < ${this.currentStatus.priority})`);
      return;
    }

    // Store new status (triggers re-render via @reactive)
    this.currentStatus = status;
    this.updateFavicon();
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

      this.log(`Favicon updated (${emoji || 'dot'}, ${faviconColor})`);
    } catch (error) {
      console.error('❌ ContinuumWidget: Failed to update favicon:', error);
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
