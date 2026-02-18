/**
 * LogsNavWidget - Navigation for log files in sidebar
 *
 * MIGRATED TO ReactiveWidget:
 * - Lit's reactive properties
 * - Declarative templates
 * - Automatic DOM diffing
 */

import { ReactiveWidget, html, css, reactive, type TemplateResult } from '../shared/ReactiveWidget';
import { Commands } from '../../system/core/shared/Commands';
import { Events } from '../../system/core/shared/Events';
import type { LogInfo, LogsListResult } from '../../commands/logs/list/shared/LogsListTypes';

import { LogsList } from '../../commands/logs/list/shared/LogsListTypes';
export const LOGS_NAV_EVENTS = {
  LOG_SELECTED: 'logs:log:selected'
} as const;

export interface LogSelectedPayload {
  logPath: string;
  logName: string;
  category: string;
}

export class LogsNavWidget extends ReactiveWidget {
  @reactive() private logs: LogInfo[] = [];
  @reactive() private selectedLog: string = '';
  @reactive() private isLoading = true;
  @reactive() private expandedCategories: Set<string> = new Set(['persona', 'system']);

  private _eventCleanup?: () => void;

  static override styles = css`
    :host {
      display: block;
    }

    .logs-nav {
      display: flex;
      flex-direction: column;
      gap: var(--spacing-sm, 8px);
      padding: var(--spacing-sm, 8px);
    }

    .loading {
      color: var(--text-secondary, #aaa);
      padding: var(--spacing-md, 12px);
      text-align: center;
    }

    .category {
      border-radius: var(--border-radius-sm, 4px);
      overflow: hidden;
    }

    .category-header {
      display: flex;
      align-items: center;
      gap: var(--spacing-xs, 4px);
      padding: var(--spacing-xs, 4px) var(--spacing-sm, 8px);
      cursor: pointer;
      color: var(--text-secondary, #aaa);
      transition: background-color 0.15s ease;
    }

    .category-header:hover {
      background-color: var(--hover-background, rgba(255, 255, 255, 0.05));
    }

    .category-chevron {
      transition: transform 0.15s ease;
    }

    .category.expanded .category-chevron {
      transform: rotate(90deg);
    }

    .category-icon {
      font-size: 0.9em;
    }

    .category-name {
      flex: 1;
      font-size: 0.85em;
      font-weight: 500;
    }

    .category-count {
      font-size: 0.75em;
      background: var(--badge-background, rgba(255, 255, 255, 0.1));
      padding: 0.1em 0.4em;
      border-radius: 3px;
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
      gap: var(--spacing-xs, 4px);
      padding: var(--spacing-xs, 4px) var(--spacing-sm, 8px);
      cursor: pointer;
      color: var(--text-secondary, #aaa);
      font-size: 0.85em;
      border-radius: var(--border-radius-sm, 4px);
      transition: background-color 0.15s ease;
    }

    .log-item:hover {
      background-color: var(--hover-background, rgba(255, 255, 255, 0.05));
    }

    .log-item.active {
      background-color: var(--active-background, rgba(0, 200, 255, 0.1));
      color: var(--text-primary, #fff);
    }

    .log-icon {
      font-size: 0.9em;
    }

    .log-name {
      flex: 1;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .active-indicator {
      width: 6px;
      height: 6px;
      border-radius: 50%;
      background: var(--success-color, #4caf50);
    }

    .log-size {
      font-size: 0.75em;
      color: var(--text-tertiary, #666);
    }

    .refresh-btn {
      margin-top: var(--spacing-sm, 8px);
      padding: var(--spacing-xs, 4px) var(--spacing-sm, 8px);
      background: var(--button-background, rgba(255, 255, 255, 0.1));
      border: 1px solid var(--border-color, #333);
      border-radius: var(--border-radius-sm, 4px);
      color: var(--text-secondary, #aaa);
      cursor: pointer;
      font-size: 0.85em;
      transition: background-color 0.15s ease;
    }

    .refresh-btn:hover {
      background: var(--hover-background, rgba(255, 255, 255, 0.15));
    }
  `;

  override connectedCallback(): void {
    super.connectedCallback();
    console.log('LogsNavWidget: Initializing...');
    this.loadLogs();

    // Listen for log selection from LogViewerWidget (bidirectional sync)
    this._eventCleanup = Events.subscribe(LOGS_NAV_EVENTS.LOG_SELECTED, (payload: LogSelectedPayload) => {
      if (payload.logPath !== this.selectedLog) {
        this.selectedLog = payload.logPath;
      }
    });
  }

  override disconnectedCallback(): void {
    this._eventCleanup?.();
    console.log('LogsNavWidget: Cleanup complete');
    super.disconnectedCallback();
  }

  private async loadLogs(): Promise<void> {
    this.isLoading = true;

    try {
      const result = await LogsList.execute({
        includeStats: true
      } as any) as LogsListResult;

      if (result.success) {
        this.logs = result.logs;
      }
    } catch (error) {
      console.error('LogsNavWidget: Failed to load logs:', error);
    }

    this.isLoading = false;
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
      case 'system': return '⚙️';
      case 'persona': return '🤖';
      case 'session': return '👤';
      case 'external': return '🌐';
      default: return '📄';
    }
  }

  private formatCategoryName(category: string): string {
    return category.charAt(0).toUpperCase() + category.slice(1);
  }

  private toggleCategory(category: string): void {
    const newSet = new Set(this.expandedCategories);
    if (newSet.has(category)) {
      newSet.delete(category);
    } else {
      newSet.add(category);
    }
    this.expandedCategories = newSet;
  }

  private selectLog(logName: string, category: string): void {
    if (logName !== this.selectedLog) {
      this.selectedLog = logName;

      Events.emit(LOGS_NAV_EVENTS.LOG_SELECTED, {
        logPath: logName,
        logName: logName.split('/').pop() || logName,
        category
      } as LogSelectedPayload);
    }
  }

  private renderLogItem(log: LogInfo, category: string): TemplateResult {
    const isActive = this.selectedLog === log.name;
    const sizeStr = log.sizeMB < 1 ? `${Math.round(log.sizeMB * 1024)}KB` : `${log.sizeMB.toFixed(1)}MB`;

    return html`
      <div
        class="log-item ${isActive ? 'active' : ''}"
        @click=${() => this.selectLog(log.name, category)}
      >
        <span class="log-icon">${log.isActive ? '📝' : '📄'}</span>
        <span class="log-name">${log.logType || log.name}</span>
        ${log.isActive ? html`<span class="active-indicator"></span>` : ''}
        <span class="log-size">${sizeStr}</span>
      </div>
    `;
  }

  private renderCategory(category: string, logs: LogInfo[]): TemplateResult {
    const isExpanded = this.expandedCategories.has(category);
    const categoryIcon = this.getCategoryIcon(category);

    return html`
      <div class="category ${isExpanded ? 'expanded' : ''}">
        <div class="category-header" @click=${() => this.toggleCategory(category)}>
          <span class="category-chevron">▶</span>
          <span class="category-icon">${categoryIcon}</span>
          <span class="category-name">${this.formatCategoryName(category)}</span>
          <span class="category-count">${logs.length}</span>
        </div>
        <div class="category-logs">
          ${logs.map(log => this.renderLogItem(log, category))}
        </div>
      </div>
    `;
  }

  override render(): TemplateResult {
    if (this.isLoading) {
      return html`
        <div class="logs-nav">
          <div class="loading">Loading logs...</div>
        </div>
      `;
    }

    const categories = this.groupLogsByCategory();

    return html`
      <div class="logs-nav">
        ${Object.entries(categories).map(([category, logs]) =>
          this.renderCategory(category, logs)
        )}
        <button class="refresh-btn" @click=${() => this.loadLogs()}>
          🔄 Refresh
        </button>
      </div>
    `;
  }
}

// Registration handled by centralized BROWSER_WIDGETS registry
