/**
 * SystemNavWidget - Quick navigation to system views
 *
 * Provides navigation to Settings, Theme, Help, and Logs.
 * Sits above Rooms in the left sidebar.
 * Clicking opens the corresponding view in the center panel.
 */

import { BaseWidget } from '../shared/BaseWidget';
import { Commands } from '../../system/core/shared/Commands';
import type { ContentType } from '../../system/data/entities/UserStateEntity';

interface NavItem {
  id: ContentType;
  icon: string;
  label: string;
  description: string;
}

export class SystemNavWidget extends BaseWidget {
  private activeItem: ContentType | null = null;

  constructor() {
    super({
      widgetName: 'SystemNavWidget',
      template: undefined,
      styles: undefined,
      enableAI: false,
      enableDatabase: false,
      enableRouterEvents: false,
      enableScreenshots: false
    });
  }

  protected async onWidgetInitialize(): Promise<void> {
    console.log('SystemNavWidget: Initializing...');
  }

  private getNavItems(): NavItem[] {
    return [
      { id: 'settings', icon: '⚙️', label: 'Settings', description: 'AI providers & configuration' },
      { id: 'theme', icon: '🎨', label: 'Theme', description: 'Customize appearance' },
      { id: 'help', icon: '❓', label: 'Help', description: 'Documentation & guides' },
      { id: 'diagnostics-log', icon: '📋', label: 'Logs', description: 'System logs & debugging' }
    ];
  }

  protected async renderWidget(): Promise<void> {
    const items = this.getNavItems();

    const styles = `
      :host {
        display: block;
      }

      .system-nav {
        display: flex;
        flex-direction: column;
        gap: 2px;
        padding: 8px;
        background: rgba(0, 0, 0, 0.2);
        border-radius: 8px;
        border: 1px solid rgba(0, 212, 255, 0.15);
      }

      .nav-header {
        font-size: 10px;
        text-transform: uppercase;
        letter-spacing: 1px;
        color: rgba(255, 255, 255, 0.4);
        padding: 4px 8px 8px;
        border-bottom: 1px solid rgba(0, 212, 255, 0.1);
        margin-bottom: 4px;
      }

      .nav-item {
        display: flex;
        align-items: center;
        gap: 10px;
        padding: 8px 10px;
        border-radius: 6px;
        cursor: pointer;
        transition: all 0.15s ease;
        color: rgba(255, 255, 255, 0.7);
      }

      .nav-item:hover {
        background: rgba(0, 212, 255, 0.15);
        color: white;
      }

      .nav-item.active {
        background: rgba(0, 212, 255, 0.2);
        color: #00d4ff;
        border-left: 2px solid #00d4ff;
        margin-left: -2px;
      }

      .nav-icon {
        font-size: 14px;
        width: 20px;
        text-align: center;
        flex-shrink: 0;
      }

      .nav-content {
        flex: 1;
        min-width: 0;
      }

      .nav-label {
        font-size: 12px;
        font-weight: 500;
      }

      .nav-description {
        font-size: 10px;
        color: rgba(255, 255, 255, 0.4);
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
      }

      .nav-item:hover .nav-description {
        color: rgba(255, 255, 255, 0.6);
      }
    `;

    const itemsHtml = items.map(item => `
      <div class="nav-item ${this.activeItem === item.id ? 'active' : ''}" data-content-type="${item.id}">
        <span class="nav-icon">${item.icon}</span>
        <div class="nav-content">
          <div class="nav-label">${item.label}</div>
          <div class="nav-description">${item.description}</div>
        </div>
      </div>
    `).join('');

    this.shadowRoot!.innerHTML = `
      <style>${styles}</style>
      <div class="system-nav">
        <div class="nav-header">System</div>
        ${itemsHtml}
      </div>
    `;

    this.setupEventListeners();
  }

  private setupEventListeners(): void {
    this.shadowRoot?.querySelectorAll('.nav-item').forEach(item => {
      item.addEventListener('click', async (e) => {
        const target = e.currentTarget as HTMLElement;
        const contentType = target.dataset.contentType as ContentType;

        if (contentType) {
          await this.openContent(contentType);
        }
      });
    });
  }

  private async openContent(contentType: ContentType): Promise<void> {
    try {
      // Update visual state
      this.activeItem = contentType;
      this.renderWidget();

      // Open content in center panel
      await Commands.execute('collaboration/content/open', {
        contentType
      } as any);

      console.log(`SystemNavWidget: Opened ${contentType}`);
    } catch (error) {
      console.error(`SystemNavWidget: Failed to open ${contentType}:`, error);
    }
  }

  protected async onWidgetCleanup(): Promise<void> {
    console.log('SystemNavWidget: Cleanup');
  }
}

// Registration handled by centralized BROWSER_WIDGETS registry
