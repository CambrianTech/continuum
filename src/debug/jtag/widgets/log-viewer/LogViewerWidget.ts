/**
 * LogViewerWidget - View and analyze log files
 *
 * Features:
 * - Display log content with syntax highlighting
 * - Filter by log level (DEBUG, INFO, WARN, ERROR)
 * - Filter by component/module
 * - Auto-follow (tail) mode for live updates
 * - Line number display
 * - Text is fully selectable/copyable
 *
 * Opened via content/open command with contentType='diagnostics-log'
 * entityId is the log path in format: {uniqueId}/{logType} (e.g., 'helper/hippocampus', 'local/cns')
 */

import { BasePanelWidget } from '../shared/BasePanelWidget';
import { Commands } from '../../system/core/shared/Commands';
import { Events } from '../../system/core/shared/Events';
import { PositronWidgetState } from '../shared/services/state/PositronWidgetState';
import type { LogLine, LogsReadResult } from '../../commands/logs/read/shared/LogsReadTypes';

interface LogViewerData {
  logPath: string;
  logName: string;
  lines: LogLine[];
  totalLines: number;
  hasMore: boolean;
  nextLine: number;
  levelFilter: 'ALL' | 'DEBUG' | 'INFO' | 'WARN' | 'ERROR';
  componentFilter: string;
  autoFollow: boolean;
  isLoading: boolean;
  error?: string;
}

export class LogViewerWidget extends BasePanelWidget {
  private logData: LogViewerData = {
    logPath: '',
    logName: '',
    lines: [],
    totalLines: 0,
    hasMore: false,
    nextLine: 0,
    levelFilter: 'ALL',
    componentFilter: '',
    autoFollow: true,
    isLoading: true
  };

  private refreshInterval?: number;
  private scrollContainer?: HTMLElement;

  constructor() {
    super({
      widgetName: 'LogViewerWidget',
      panelTitle: 'Log Viewer',
      panelSubtitle: 'Analyzing log file...',
      enableDatabase: false,
      additionalStyles: LOG_VIEWER_STYLES
    });
  }

  protected async onPanelInitialize(): Promise<void> {
    // Extract log path from content item metadata
    await this.loadLogPathFromContent();
  }

  /**
   * Called by MainWidget when this widget is activated with a new entityId.
   * This allows cached widgets to reload with different log files.
   */
  public async onActivate(entityId?: string): Promise<void> {
    console.log(`📜 LogViewer: onActivate called with entityId=${entityId}`);

    if (entityId) {
      this.setAttribute('entity-id', entityId);
    }

    // Stop current auto-refresh before switching
    this.stopAutoRefresh();

    // Reload with new log path
    await this.loadLogPathFromContent();
  }

  private async loadLogPathFromContent(): Promise<void> {
    // Try to get log path from content item metadata
    // The DiagnosticsWidget passes logPath in metadata when opening this tab
    const logPath = this.getAttribute('entity-id') ||
                    this.getAttribute('data-entity-id') ||
                    (this as any).entityId ||
                    '.continuum/personas/helper/logs/hippocampus.log'; // Default for testing

    this.logData.logPath = logPath;
    this.logData.logName = logPath.split('/').pop() || 'log';

    // Update panel subtitle
    this.panelConfig.panelTitle = this.logData.logName;
    this.panelConfig.panelSubtitle = logPath;

    await this.loadLog();
    this.startAutoRefresh();
  }

  private async loadLog(): Promise<void> {
    this.logData.isLoading = true;
    this.renderWidget();

    try {
      const result = await Commands.execute('logs/read', {
        log: this.logData.logPath,
        tail: 200, // Get last 200 lines initially
        level: this.logData.levelFilter !== 'ALL' ? this.logData.levelFilter : undefined
      } as any) as LogsReadResult;

      if (result.success) {
        this.logData.lines = result.lines;
        this.logData.totalLines = result.totalLines;
        this.logData.hasMore = result.hasMore;
        this.logData.nextLine = result.nextLine || 0;
        this.logData.error = undefined;
      } else {
        this.logData.error = result.error || 'Failed to load log';
      }
    } catch (error) {
      this.logData.error = error instanceof Error ? error.message : 'Unknown error';
    }

    this.logData.isLoading = false;
    this.renderWidget();
    this.emitPositronContext();

    // Scroll to bottom if auto-follow is enabled
    if (this.logData.autoFollow) {
      this.scrollToBottom();
    }
  }

  /**
   * Emit Positron context for AI awareness
   * NOTE: Removed emit - MainWidget handles context. Widgets should RECEIVE, not emit.
   */
  private emitPositronContext(): void {
    // No-op - context cascade fix
  }

  private startAutoRefresh(): void {
    if (this.refreshInterval) {
      clearInterval(this.refreshInterval);
    }

    if (this.logData.autoFollow) {
      this.refreshInterval = window.setInterval(() => {
        this.refreshLog();
      }, 3000); // Refresh every 3 seconds
    }
  }

  private stopAutoRefresh(): void {
    if (this.refreshInterval) {
      clearInterval(this.refreshInterval);
      this.refreshInterval = undefined;
    }
  }

  private async refreshLog(): Promise<void> {
    if (!this.logData.autoFollow) return;

    try {
      const result = await Commands.execute('logs/read', {
        log: this.logData.logPath,
        tail: 50, // Get just the latest lines
        level: this.logData.levelFilter !== 'ALL' ? this.logData.levelFilter : undefined
      } as any) as LogsReadResult;

      if (result.success) {
        // Merge new lines with existing, keeping last 500
        const newLines = result.lines.filter(
          newLine => !this.logData.lines.some(existing => existing.lineNumber === newLine.lineNumber)
        );

        if (newLines.length > 0) {
          this.logData.lines = [...this.logData.lines, ...newLines].slice(-500);
          this.logData.totalLines = result.totalLines;
          this.renderWidget();

          if (this.logData.autoFollow) {
            this.scrollToBottom();
          }
        }
      }
    } catch (error) {
      // Silent fail on refresh - don't show errors for periodic refreshes
      console.warn('LogViewerWidget: Refresh failed:', error);
    }
  }

  private scrollToBottom(): void {
    if (!this.shadowRoot) return;

    const container = this.shadowRoot.querySelector('.log-content');
    if (container) {
      container.scrollTop = container.scrollHeight;
    }
  }

  protected getStyles(): string {
    return LOG_VIEWER_STYLES;
  }

  protected async renderContent(): Promise<string> {
    if (this.logData.isLoading) {
      return this.createLoading('Loading log file...');
    }

    if (this.logData.error) {
      return this.createInfoBox(`Error: ${this.logData.error}`, 'error');
    }

    return `
      ${this.renderControls()}
      ${this.renderLogContent()}
      ${this.renderStats()}
    `;
  }

  private renderControls(): string {
    return `
      <div class="log-controls">
        <div class="control-group">
          <label>Level:</label>
          <select class="level-select" data-action="filter-level">
            <option value="ALL" ${this.logData.levelFilter === 'ALL' ? 'selected' : ''}>All Levels</option>
            <option value="DEBUG" ${this.logData.levelFilter === 'DEBUG' ? 'selected' : ''}>DEBUG</option>
            <option value="INFO" ${this.logData.levelFilter === 'INFO' ? 'selected' : ''}>INFO</option>
            <option value="WARN" ${this.logData.levelFilter === 'WARN' ? 'selected' : ''}>WARN</option>
            <option value="ERROR" ${this.logData.levelFilter === 'ERROR' ? 'selected' : ''}>ERROR</option>
          </select>
        </div>
        <div class="control-group">
          <label>Component:</label>
          <input type="text" class="component-input" placeholder="Filter by component..."
                 value="${this.logData.componentFilter}" data-action="filter-component" />
        </div>
        <div class="control-group">
          <label class="toggle-label">
            <input type="checkbox" class="auto-follow-toggle" data-action="toggle-follow"
                   ${this.logData.autoFollow ? 'checked' : ''} />
            Auto-follow
          </label>
        </div>
        <button class="btn btn-secondary refresh-btn" data-action="refresh">Refresh</button>
      </div>
    `;
  }

  private renderLogContent(): string {
    const filteredLines = this.filterLines(this.logData.lines);

    if (filteredLines.length === 0) {
      return `
        <div class="log-content empty">
          <p>No log entries found${this.logData.levelFilter !== 'ALL' ? ` for level ${this.logData.levelFilter}` : ''}.</p>
        </div>
      `;
    }

    const linesHtml = filteredLines.map(line => this.renderLogLine(line)).join('');

    return `
      <div class="log-content">
        <div class="log-lines">
          ${linesHtml}
        </div>
      </div>
    `;
  }

  private renderLogLine(line: LogLine): string {
    const levelClass = this.getLevelClass(line.level);
    const timestamp = line.timestamp ? `<span class="log-timestamp">${line.timestamp}</span>` : '';
    const level = line.level ? `<span class="log-level ${levelClass}">[${line.level}]</span>` : '';
    const component = line.component ? `<span class="log-component">[${line.component}]</span>` : '';

    return `
      <div class="log-line ${levelClass}" data-line="${line.lineNumber}">
        <span class="line-number">${line.lineNumber}</span>
        ${timestamp}
        ${level}
        ${component}
        <span class="log-message">${this.escapeHtml(line.content)}</span>
      </div>
    `;
  }

  private filterLines(lines: LogLine[]): LogLine[] {
    return lines.filter(line => {
      // Level filter
      if (this.logData.levelFilter !== 'ALL' && line.level !== this.logData.levelFilter) {
        return false;
      }

      // Component filter
      if (this.logData.componentFilter && line.component) {
        if (!line.component.toLowerCase().includes(this.logData.componentFilter.toLowerCase())) {
          return false;
        }
      }

      return true;
    });
  }

  private getLevelClass(level?: string): string {
    switch (level) {
      case 'ERROR': return 'level-error';
      case 'WARN': return 'level-warn';
      case 'INFO': return 'level-info';
      case 'DEBUG': return 'level-debug';
      default: return '';
    }
  }

  private escapeHtml(text: string): string {
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
  }

  private renderStats(): string {
    const filteredCount = this.filterLines(this.logData.lines).length;

    return `
      <div class="log-stats">
        <span>Showing ${filteredCount} of ${this.logData.totalLines} lines</span>
        ${this.logData.hasMore ? `<span class="more-indicator">More lines available</span>` : ''}
        ${this.logData.autoFollow ? `<span class="follow-indicator">Auto-refreshing</span>` : ''}
      </div>
    `;
  }

  protected async onContentRendered(): Promise<void> {
    if (!this.shadowRoot) return;

    // Level filter
    const levelSelect = this.shadowRoot.querySelector('.level-select') as HTMLSelectElement;
    levelSelect?.addEventListener('change', (e) => {
      const target = e.target as HTMLSelectElement;
      this.logData.levelFilter = target.value as 'ALL' | 'DEBUG' | 'INFO' | 'WARN' | 'ERROR';
      this.loadLog();
    });

    // Component filter
    const componentInput = this.shadowRoot.querySelector('.component-input') as HTMLInputElement;
    componentInput?.addEventListener('input', (e) => {
      const target = e.target as HTMLInputElement;
      this.logData.componentFilter = target.value;
      this.renderWidget();
    });

    // Auto-follow toggle
    const followToggle = this.shadowRoot.querySelector('.auto-follow-toggle') as HTMLInputElement;
    followToggle?.addEventListener('change', (e) => {
      const target = e.target as HTMLInputElement;
      this.logData.autoFollow = target.checked;
      this.startAutoRefresh();
      if (this.logData.autoFollow) {
        this.scrollToBottom();
      }
    });

    // Refresh button
    const refreshBtn = this.shadowRoot.querySelector('.refresh-btn');
    refreshBtn?.addEventListener('click', () => {
      this.loadLog();
    });
  }

  async disconnectedCallback(): Promise<void> {
    if (this.refreshInterval) {
      clearInterval(this.refreshInterval);
    }
    await super.disconnectedCallback();
  }
}

/**
 * Styles specific to LogViewerWidget
 */
const LOG_VIEWER_STYLES = `
  /* Enable text selection for log content (override base theme) */
  .log-content,
  .log-content * {
    user-select: text !important;
    -webkit-user-select: text !important;
  }

  .log-controls {
    display: flex;
    gap: 16px;
    padding: 12px 16px;
    background: rgba(0, 10, 15, 0.7);
    border: 1px solid rgba(0, 212, 255, 0.2);
    border-radius: 6px;
    margin-bottom: 16px;
    flex-wrap: wrap;
    align-items: center;
  }

  .control-group {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .control-group label {
    font-size: 12px;
    color: rgba(255, 255, 255, 0.7);
  }

  .level-select,
  .component-input {
    background: rgba(0, 0, 0, 0.4);
    border: 1px solid rgba(0, 212, 255, 0.3);
    border-radius: 4px;
    color: white;
    padding: 6px 10px;
    font-size: 12px;
    font-family: inherit;
  }

  .level-select:focus,
  .component-input:focus {
    outline: none;
    border-color: var(--content-accent, #00d4ff);
  }

  .toggle-label {
    display: flex;
    align-items: center;
    gap: 6px;
    cursor: pointer;
  }

  .auto-follow-toggle {
    accent-color: var(--content-accent, #00d4ff);
  }

  .log-content {
    background: rgba(0, 5, 10, 0.9);
    border: 1px solid rgba(0, 212, 255, 0.2);
    border-radius: 6px;
    padding: 8px;
    max-height: calc(100vh - 320px);
    overflow-y: auto;
    font-family: 'JetBrains Mono', 'Fira Code', 'Consolas', monospace;
    font-size: 12px;
    line-height: 1.5;
  }

  .log-content.empty {
    display: flex;
    align-items: center;
    justify-content: center;
    min-height: 200px;
    color: rgba(255, 255, 255, 0.5);
  }

  .log-lines {
    display: flex;
    flex-direction: column;
  }

  .log-line {
    display: flex;
    gap: 8px;
    padding: 2px 8px;
    border-radius: 3px;
    transition: background 0.15s ease;
  }

  .log-line:hover {
    background: rgba(0, 212, 255, 0.1);
  }

  .line-number {
    color: rgba(255, 255, 255, 0.3);
    min-width: 50px;
    text-align: right;
    user-select: none;
  }

  .log-timestamp {
    color: rgba(0, 212, 255, 0.7);
    min-width: 80px;
  }

  .log-level {
    min-width: 55px;
    font-weight: 600;
  }

  .log-component {
    color: rgba(255, 255, 255, 0.5);
    min-width: 100px;
  }

  .log-message {
    color: rgba(255, 255, 255, 0.9);
    flex: 1;
    word-break: break-word;
  }

  /* Level colors */
  .level-error .log-level,
  .level-error {
    color: var(--status-busy, #ff5050);
  }

  .level-error .log-message {
    color: var(--status-busy, #ff7070);
  }

  .level-warn .log-level,
  .level-warn {
    color: var(--status-away, #ffcc00);
  }

  .level-warn .log-message {
    color: var(--status-away, #ffd633);
  }

  .level-info .log-level {
    color: var(--content-accent, #00d4ff);
  }

  .level-debug .log-level {
    color: rgba(255, 255, 255, 0.4);
  }

  .level-debug .log-message {
    color: rgba(255, 255, 255, 0.6);
  }

  /* Stats bar */
  .log-stats {
    display: flex;
    gap: 16px;
    padding: 8px 16px;
    margin-top: 12px;
    font-size: 11px;
    color: rgba(255, 255, 255, 0.5);
    border-top: 1px solid rgba(0, 212, 255, 0.1);
  }

  .more-indicator {
    color: var(--status-away, #ffcc00);
  }

  .follow-indicator {
    color: var(--content-success, #00ff64);
  }

  .follow-indicator::before {
    content: '';
    display: inline-block;
    width: 6px;
    height: 6px;
    background: var(--content-success, #00ff64);
    border-radius: 50%;
    margin-right: 6px;
    animation: pulse 1.5s ease infinite;
  }

  @keyframes pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.4; }
  }

  /* Scrollbar styling */
  .log-content::-webkit-scrollbar {
    width: 8px;
  }

  .log-content::-webkit-scrollbar-track {
    background: rgba(0, 0, 0, 0.2);
    border-radius: 4px;
  }

  .log-content::-webkit-scrollbar-thumb {
    background: rgba(0, 212, 255, 0.3);
    border-radius: 4px;
  }

  .log-content::-webkit-scrollbar-thumb:hover {
    background: rgba(0, 212, 255, 0.5);
  }
`;

// Register the custom element
// Registration handled by centralized BROWSER_WIDGETS registry
