/**
 * LogsNavWidget - Navigation for log files in sidebar
 *
 * Shows available log files organized by category (system, persona, session).
 * Clicking a log opens it in the LogViewerWidget.
 * Emits events to coordinate with LogViewerWidget in center panel.
 */

import { BaseWidget } from '../shared/BaseWidget';
import { Commands } from '../../system/core/shared/Commands';
import { Events } from '../../system/core/shared/Events';
import type { LogInfo, LogsListResult } from '../../commands/logs/list/shared/LogsListTypes';

export const LOGS_NAV_EVENTS = {
  LOG_SELECTED: 'logs:log:selected'
} as const;

export interface LogSelectedPayload {
  logPath: string;
  logName: string;
  category: string;
}

export class LogsNavWidget extends BaseWidget {
  private logs: LogInfo[] = [];
  private selectedLog: string = '';
  private isLoading = true;
  private expandedCategories: Set<string> = new Set(['persona', 'system']);

  constructor() {
    super({
      widgetName: 'LogsNavWidget',
      template: undefined,
      styles: undefined,
      enableAI: false,
      enableDatabase: false,
      enableRouterEvents: false,
      enableScreenshots: false
    });
  }

  protected async onWidgetInitialize(): Promise<void> {
    console.log('LogsNavWidget: Initializing...');
    await this.loadLogs();

    // Listen for log selection from LogViewerWidget (bidirectional sync)
    Events.subscribe(LOGS_NAV_EVENTS.LOG_SELECTED, (payload: LogSelectedPayload) => {
      if (payload.logPath !== this.selectedLog) {
        this.selectedLog = payload.logPath;
        this.updateActiveState();
      }
    });
  }

  private async loadLogs(): Promise<void> {
    this.isLoading = true;
    this.renderWidget();

    try {
      const result = await Commands.execute('logs/list', {
        includeStats: true
      } as any) as LogsListResult;

      if (result.success) {
        this.logs = result.logs;
      }
    } catch (error) {
      console.error('LogsNavWidget: Failed to load logs:', error);
    }

    this.isLoading = false;
    this.renderWidget();
  }

  protected async renderWidget(): Promise<void> {
    const styles = `
      :host {
        display: block;
      }

      .logs-nav-container {
        padding: var(--spacing-md, 12px);
      }

      .nav-title {
        font-size: 11px;
        font-weight: 600;
        text-transform: uppercase;
        letter-spacing: 0.5px;
        color: var(--content-secondary, rgba(255, 255, 255, 0.5));
        margin-bottom: var(--spacing-sm, 8px);
        padding: 0 var(--spacing-sm, 8px);
      }

      .loading {
        padding: var(--spacing-md, 12px);
        color: var(--content-secondary, rgba(255, 255, 255, 0.5));
        font-size: 12px;
      }

      .category {
        margin-bottom: var(--spacing-sm, 8px);
      }

      .category-header {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs, 4px);
        padding: var(--spacing-xs, 4px) var(--spacing-sm, 8px);
        cursor: pointer;
        color: var(--content-secondary, rgba(255, 255, 255, 0.6));
        font-size: 11px;
        font-weight: 600;
        text-transform: uppercase;
        letter-spacing: 0.5px;
        border-radius: var(--border-radius-sm, 4px);
        transition: all 0.15s ease;
      }

      .category-header:hover {
        background: var(--sidebar-hover, rgba(0, 212, 255, 0.1));
        color: var(--content-primary, white);
      }

      .category-chevron {
        font-size: 10px;
        transition: transform 0.15s ease;
      }

      .category.expanded .category-chevron {
        transform: rotate(90deg);
      }

      .category-count {
        margin-left: auto;
        font-size: 10px;
        color: var(--content-secondary, rgba(255, 255, 255, 0.4));
      }

      .category-logs {
        display: none;
        padding-left: var(--spacing-md, 12px);
      }

      .category.expanded .category-logs {
        display: block;
      }

      .log-item {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm, 8px);
        padding: var(--spacing-xs, 4px) var(--spacing-sm, 8px);
        border-radius: var(--border-radius-sm, 6px);
        cursor: pointer;
        transition: all 0.15s ease;
        color: var(--content-secondary, rgba(255, 255, 255, 0.7));
        font-size: 13px;
      }

      .log-item:hover {
        background: var(--sidebar-hover, rgba(0, 212, 255, 0.1));
        color: var(--content-primary, white);
      }

      .log-item.active {
        background: var(--sidebar-active, rgba(0, 212, 255, 0.15));
        color: var(--content-accent, #00d4ff);
      }

      .log-icon {
        font-size: 14px;
        width: 18px;
        text-align: center;
      }

      .log-name {
        flex: 1;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
      }

      .log-size {
        font-size: 10px;
        color: var(--content-secondary, rgba(255, 255, 255, 0.4));
      }

      .log-item.active .log-size {
        color: var(--content-accent, #00d4ff);
        opacity: 0.7;
      }

      .active-indicator {
        width: 6px;
        height: 6px;
        border-radius: 50%;
        background: var(--status-success, #00ff64);
      }

      .refresh-btn {
        display: block;
        width: 100%;
        margin-top: var(--spacing-md, 12px);
        padding: var(--spacing-sm, 8px);
        background: transparent;
        border: 1px solid var(--border-subtle, rgba(0, 212, 255, 0.2));
        border-radius: var(--border-radius-sm, 6px);
        color: var(--content-secondary, rgba(255, 255, 255, 0.6));
        font-size: 12px;
        cursor: pointer;
        transition: all 0.15s ease;
      }

      .refresh-btn:hover {
        background: var(--sidebar-hover, rgba(0, 212, 255, 0.1));
        border-color: var(--content-accent, #00d4ff);
        color: var(--content-primary, white);
      }
    `;

    if (this.isLoading) {
      this.shadowRoot!.innerHTML = `
        <style>${styles}</style>
        <div class="logs-nav-container">
          <div class="nav-title">Log Files</div>
          <div class="loading">Loading logs...</div>
        </div>
      `;
      return;
    }

    // Group logs by category
    const categories = this.groupLogsByCategory();

    const categoriesHtml = Object.entries(categories).map(([category, logs]) => {
      const isExpanded = this.expandedCategories.has(category);
      const categoryIcon = this.getCategoryIcon(category);

      const logsHtml = logs.map(log => {
        const isActive = this.selectedLog === log.name;
        const sizeStr = log.sizeMB < 1 ? `${Math.round(log.sizeMB * 1024)}KB` : `${log.sizeMB.toFixed(1)}MB`;

        return `
          <div class="log-item ${isActive ? 'active' : ''}" data-log="${log.name}" data-category="${category}">
            <span class="log-icon">${log.isActive ? '' : ''}</span>
            <span class="log-name">${log.logType || log.name}</span>
            ${log.isActive ? '<span class="active-indicator"></span>' : ''}
            <span class="log-size">${sizeStr}</span>
          </div>
        `;
      }).join('');

      return `
        <div class="category ${isExpanded ? 'expanded' : ''}" data-category="${category}">
          <div class="category-header">
            <span class="category-chevron"></span>
            <span class="category-icon">${categoryIcon}</span>
            <span class="category-name">${this.formatCategoryName(category)}</span>
            <span class="category-count">${logs.length}</span>
          </div>
          <div class="category-logs">
            ${logsHtml}
          </div>
        </div>
      `;
    }).join('');

    const template = `
      <div class="logs-nav-container">
        <div class="nav-title">Log Files</div>
        ${categoriesHtml}
        <button class="refresh-btn" data-action="refresh">Refresh</button>
      </div>
    `;

    this.shadowRoot!.innerHTML = `<style>${styles}</style>${template}`;
    this.setupEventListeners();
  }

  private groupLogsByCategory(): Record<string, LogInfo[]> {
    const grouped: Record<string, LogInfo[]> = {};

    for (const log of this.logs) {
      const category = log.category || 'other';
      if (!grouped[category]) {
        grouped[category] = [];
      }
      grouped[category].push(log);
    }

    // Sort logs within each category by name
    for (const logs of Object.values(grouped)) {
      logs.sort((a, b) => a.name.localeCompare(b.name));
    }

    return grouped;
  }

  private getCategoryIcon(category: string): string {
    switch (category) {
      case 'system': return '';
      case 'persona': return '';
      case 'session': return '';
      case 'external': return '';
      default: return '';
    }
  }

  private formatCategoryName(category: string): string {
    return category.charAt(0).toUpperCase() + category.slice(1);
  }

  private setupEventListeners(): void {
    // Category toggle
    this.shadowRoot?.querySelectorAll('.category-header').forEach(header => {
      header.addEventListener('click', (e) => {
        const category = (header.parentElement as HTMLElement).dataset.category;
        if (category) {
          if (this.expandedCategories.has(category)) {
            this.expandedCategories.delete(category);
          } else {
            this.expandedCategories.add(category);
          }
          this.renderWidget();
        }
      });
    });

    // Log item click
    this.shadowRoot?.querySelectorAll('.log-item').forEach(item => {
      item.addEventListener('click', (e) => {
        const logName = (item as HTMLElement).dataset.log;
        const category = (item as HTMLElement).dataset.category;
        if (logName && logName !== this.selectedLog) {
          this.selectedLog = logName;
          this.updateActiveState();

          // Emit event for LogViewerWidget
          Events.emit(LOGS_NAV_EVENTS.LOG_SELECTED, {
            logPath: logName,
            logName: logName.split('/').pop() || logName,
            category
          } as LogSelectedPayload);
        }
      });
    });

    // Refresh button
    this.shadowRoot?.querySelector('.refresh-btn')?.addEventListener('click', () => {
      this.loadLogs();
    });
  }

  private updateActiveState(): void {
    this.shadowRoot?.querySelectorAll('.log-item').forEach(item => {
      const logName = (item as HTMLElement).dataset.log;
      item.classList.toggle('active', logName === this.selectedLog);
    });
  }

  protected async onWidgetCleanup(): Promise<void> {
    console.log('LogsNavWidget: Cleanup complete');
  }
}

// Registration handled by centralized BROWSER_WIDGETS registry
