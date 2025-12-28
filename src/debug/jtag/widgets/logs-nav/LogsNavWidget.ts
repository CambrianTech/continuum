/**
 * LogsNavWidget - Navigation for log files in sidebar
 *
 * Shows available log files organized by category (system, persona, session).
 * Clicking a log opens it in the LogViewerWidget.
 * Emits events to coordinate with LogViewerWidget in center panel.
 *
 * Structure:
 * - public/logs-nav-widget.html - Template container
 * - public/logs-nav-widget.scss - Styles (compiled to .css)
 * - LogsNavWidget.ts - Logic (this file)
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
      template: 'logs-nav-widget.html',
      styles: 'logs-nav-widget.css',
      enableAI: false,
      enableDatabase: false,
      enableRouterEvents: false,
      enableScreenshots: false
    });
  }

  /**
   * Override path resolution - directory is 'logs-nav' (kebab-case)
   */
  protected resolveResourcePath(filename: string): string {
    return `widgets/logs-nav/public/${filename}`;
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
    // Inject loaded template and styles into shadow DOM
    if (this.shadowRoot && (this.templateHTML || this.templateCSS)) {
      const styleTag = this.templateCSS ? `<style>${this.templateCSS}</style>` : '';
      this.shadowRoot.innerHTML = styleTag + (this.templateHTML || '');
    }

    // Render dynamic content
    this.renderContent();
    this.setupEventListeners();
  }

  private renderContent(): void {
    const contentContainer = this.shadowRoot?.querySelector('.logs-content');
    if (!contentContainer) return;

    if (this.isLoading) {
      contentContainer.innerHTML = '<div class="loading">Loading logs...</div>';
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

    contentContainer.innerHTML = `
      ${categoriesHtml}
      <button class="refresh-btn" data-action="refresh">Refresh</button>
    `;
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
