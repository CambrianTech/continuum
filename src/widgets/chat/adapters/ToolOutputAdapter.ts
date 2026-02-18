/**
 * ToolOutputAdapter - Renders tool execution results as rich, expandable cards
 *
 * When AIs use code/* tools, the results are stored as ChatMessageEntity with
 * metadata.toolResult = true. This adapter renders them as compact summaries
 * with expandable detail views instead of flat text like "write: 10 lines".
 *
 * Uses native <details> element for expand/collapse (no JS needed).
 * Per-tool rendering via strategy map (not switch statements).
 */

import type { ChatMessageEntity, MediaItem } from '../../../system/data/entities/ChatMessageEntity';
import { AbstractMessageAdapter } from './AbstractMessageAdapter';
import { Events } from '../../../system/core/shared/Events';

// ────────────────────────────────────────────────────────────
// Types
// ────────────────────────────────────────────────────────────

interface ToolOutputContentData {
  readonly toolName: string;
  readonly toolAction: string;
  readonly toolCategory: string;
  readonly success: boolean;
  readonly summary: string;
  readonly parameters: Record<string, unknown>;
  readonly fullData: unknown;
  readonly error?: string;
  readonly media?: readonly MediaItem[];
  readonly messageId: string;
}

interface ToolRenderer {
  renderCompact(data: ToolOutputContentData): string;
  renderExpanded(data: ToolOutputContentData): string;
}

// ────────────────────────────────────────────────────────────
// Utility
// ────────────────────────────────────────────────────────────

function escapeHtml(str: string): string {
  return str
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

function shortenPath(filePath: string): string {
  if (!filePath) return '';
  const parts = String(filePath).split('/');
  return parts.length > 3 ? '.../' + parts.slice(-3).join('/') : String(filePath);
}

function guessLanguage(filePath: string): string {
  if (!filePath) return 'plaintext';
  const ext = String(filePath).split('.').pop()?.toLowerCase();
  const map: Record<string, string> = {
    ts: 'typescript', tsx: 'typescript', js: 'javascript', jsx: 'javascript',
    rs: 'rust', py: 'python', json: 'json', html: 'html', css: 'css',
    md: 'markdown', toml: 'toml', yaml: 'yaml', yml: 'yaml', sh: 'bash',
    swift: 'swift', go: 'go', java: 'java', c: 'c', cpp: 'cpp', h: 'c',
  };
  return map[ext || ''] || 'plaintext';
}

function truncate(str: string, max: number): string {
  return str.length > max ? str.slice(0, max) + '...' : str;
}

function stringifyData(data: unknown, maxLines = 200): string {
  const raw = typeof data === 'string' ? data : JSON.stringify(data, null, 2);
  if (!raw) return '';
  const lines = raw.split('\n');
  if (lines.length <= maxLines) return escapeHtml(raw);
  return escapeHtml(lines.slice(0, maxLines).join('\n')) + `\n\n... (${lines.length - maxLines} more lines)`;
}

function extractField(data: unknown, ...keys: string[]): unknown {
  if (!data || typeof data !== 'object') return undefined;
  const obj = data as Record<string, unknown>;
  for (const key of keys) {
    if (obj[key] !== undefined) return obj[key];
  }
  return undefined;
}

// ────────────────────────────────────────────────────────────
// Per-Tool Renderers
// ────────────────────────────────────────────────────────────

class WriteToolRenderer implements ToolRenderer {
  renderCompact(data: ToolOutputContentData): string {
    const filePath = extractField(data.parameters, 'filePath', 'path', 'file_path') as string || '';
    const bytes = extractField(data.fullData, 'bytesWritten', 'bytes_written', 'size');
    const suffix = typeof bytes === 'number' ? ` (${bytes} bytes)` : '';
    return `<span class="tool-file-path">${escapeHtml(shortenPath(filePath))}</span>${suffix}`;
  }

  renderExpanded(data: ToolOutputContentData): string {
    const filePath = extractField(data.parameters, 'filePath', 'path', 'file_path') as string || '';
    const content = data.parameters.content as string;
    if (content) {
      const lang = guessLanguage(filePath);
      return `<pre class="tool-output-pre"><code class="language-${lang}">${escapeHtml(content)}</code></pre>`;
    }
    return `<pre class="tool-output-pre">${stringifyData(data.fullData)}</pre>`;
  }
}

class ReadToolRenderer implements ToolRenderer {
  renderCompact(data: ToolOutputContentData): string {
    const filePath = extractField(data.parameters, 'filePath', 'path', 'file_path') as string || '';
    const content = typeof data.fullData === 'string' ? data.fullData : '';
    const lineCount = content ? content.split('\n').length : 0;
    const suffix = lineCount > 0 ? ` (${lineCount} lines)` : '';
    return `<span class="tool-file-path">${escapeHtml(shortenPath(filePath))}</span>${suffix}`;
  }

  renderExpanded(data: ToolOutputContentData): string {
    const filePath = extractField(data.parameters, 'filePath', 'path', 'file_path') as string || '';
    const content = typeof data.fullData === 'string' ? data.fullData : '';
    if (content) {
      const lang = guessLanguage(filePath);
      return `<pre class="tool-output-pre"><code class="language-${lang}">${stringifyData(content)}</code></pre>`;
    }
    return `<pre class="tool-output-pre">${stringifyData(data.fullData)}</pre>`;
  }
}

class EditToolRenderer implements ToolRenderer {
  renderCompact(data: ToolOutputContentData): string {
    const filePath = extractField(data.parameters, 'filePath', 'path', 'file_path') as string || '';
    const editMode = extractField(data.parameters, 'editMode', 'editType', 'mode') as string || '';
    const modeLabel = editMode ? ` (${editMode})` : '';
    return `<span class="tool-file-path">${escapeHtml(shortenPath(filePath))}</span><span class="tool-edit-mode">${escapeHtml(modeLabel)}</span>`;
  }

  renderExpanded(data: ToolOutputContentData): string {
    const raw = typeof data.fullData === 'string' ? data.fullData : JSON.stringify(data.fullData, null, 2) || '';
    // Check if the output looks like a diff
    if (raw.includes('---') || raw.includes('@@') || raw.includes('+++')) {
      return `<pre class="tool-output-pre tool-diff-output">${this.renderDiffLines(raw)}</pre>`;
    }
    return `<pre class="tool-output-pre">${stringifyData(data.fullData)}</pre>`;
  }

  private renderDiffLines(diff: string): string {
    return diff.split('\n').map(line => {
      if (line.startsWith('+') && !line.startsWith('+++')) {
        return `<span class="diff-add">${escapeHtml(line)}</span>`;
      }
      if (line.startsWith('-') && !line.startsWith('---')) {
        return `<span class="diff-remove">${escapeHtml(line)}</span>`;
      }
      if (line.startsWith('@@')) {
        return `<span class="diff-hunk">${escapeHtml(line)}</span>`;
      }
      return escapeHtml(line);
    }).join('\n');
  }
}

class VerifyToolRenderer implements ToolRenderer {
  renderCompact(data: ToolOutputContentData): string {
    if (data.success) {
      const duration = extractField(data.fullData, 'duration', 'durationMs', 'elapsed');
      const durationStr = typeof duration === 'number' ? ` (${(duration / 1000).toFixed(1)}s)` : '';
      return `<span class="verify-pass">Build succeeded${durationStr}</span>`;
    }
    const errorCount = extractField(data.fullData, 'errorCount', 'failedCount', 'errors');
    return `<span class="verify-fail">${typeof errorCount === 'number' ? `${errorCount} error(s)` : 'Build failed'}</span>`;
  }

  renderExpanded(data: ToolOutputContentData): string {
    if (data.error) {
      return `<pre class="tool-output-pre tool-error-output">${escapeHtml(data.error)}</pre>`;
    }
    return `<pre class="tool-output-pre">${stringifyData(data.fullData)}</pre>`;
  }
}

class GitToolRenderer implements ToolRenderer {
  renderCompact(data: ToolOutputContentData): string {
    const operation = extractField(data.parameters, 'operation', 'subcommand', 'op') as string || 'status';

    if (operation === 'commit') {
      const hash = extractField(data.fullData, 'hash', 'commitHash', 'sha') as string || '';
      const message = data.parameters.message as string || '';
      const shortHash = hash ? hash.slice(0, 7) : '';
      return `commit ${shortHash ? `<span class="git-hash">${escapeHtml(shortHash)}</span> ` : ''}&mdash; "${escapeHtml(truncate(message, 50))}"`;
    }

    return `<span class="tool-summary-text">${escapeHtml(operation)}</span>`;
  }

  renderExpanded(data: ToolOutputContentData): string {
    return `<pre class="tool-output-pre">${stringifyData(data.fullData)}</pre>`;
  }
}

class SearchToolRenderer implements ToolRenderer {
  renderCompact(data: ToolOutputContentData): string {
    const matches = extractField(data.fullData, 'matches', 'count', 'total');
    const query = data.parameters.query || data.parameters.pattern || '';
    if (typeof matches === 'number') {
      return `${matches} match${matches !== 1 ? 'es' : ''} for "<span class="tool-search-query">${escapeHtml(truncate(String(query), 30))}</span>"`;
    }
    return `<span class="tool-summary-text">${escapeHtml(data.summary)}</span>`;
  }

  renderExpanded(data: ToolOutputContentData): string {
    return `<pre class="tool-output-pre">${stringifyData(data.fullData)}</pre>`;
  }
}

class TreeToolRenderer implements ToolRenderer {
  renderCompact(data: ToolOutputContentData): string {
    const content = typeof data.fullData === 'string' ? data.fullData : '';
    const entries = content ? content.split('\n').filter((l: string) => l.trim()).length : 0;
    const dir = extractField(data.parameters, 'directory', 'dir', 'path') as string || '.';
    return `<span class="tool-file-path">${escapeHtml(shortenPath(dir))}</span> (${entries} entries)`;
  }

  renderExpanded(data: ToolOutputContentData): string {
    return `<pre class="tool-output-pre tool-tree-output">${stringifyData(data.fullData)}</pre>`;
  }
}

class DiffToolRenderer implements ToolRenderer {
  renderCompact(data: ToolOutputContentData): string {
    const filePath = extractField(data.parameters, 'filePath', 'path', 'file_path') as string || '';
    return `<span class="tool-file-path">${escapeHtml(shortenPath(filePath))}</span>`;
  }

  renderExpanded(data: ToolOutputContentData): string {
    const raw = typeof data.fullData === 'string' ? data.fullData : JSON.stringify(data.fullData, null, 2) || '';
    return `<pre class="tool-output-pre tool-diff-output">${new EditToolRenderer().renderExpanded(data).replace(/<\/?pre[^>]*>/g, '')}</pre>`;
  }
}

// ────────────────────────────────────────────────────────────
// Shell Tool Renderers
// ────────────────────────────────────────────────────────────

class ShellExecuteToolRenderer implements ToolRenderer {
  renderCompact(data: ToolOutputContentData): string {
    const cmd = extractField(data.parameters, 'cmd', 'command') as string || '';
    const fd = data.fullData as Record<string, unknown> | undefined;
    const status = (fd?.status as string) || (data.success ? 'completed' : 'failed');
    const exitCode = fd?.exitCode as number | undefined;
    const exitStr = typeof exitCode === 'number' ? ` exit ${exitCode}` : '';

    const statusClass = status === 'completed' ? 'shell-status-ok'
      : status === 'running' ? 'shell-status-running'
      : 'shell-status-fail';

    return `<span class="shell-cmd">$ ${escapeHtml(truncate(cmd, 60))}</span> `
      + `<span class="${statusClass}">${escapeHtml(status)}${exitStr}</span>`;
  }

  renderExpanded(data: ToolOutputContentData): string {
    const fd = data.fullData as Record<string, unknown> | undefined;
    const stdout = (fd?.stdout as string) || '';
    const stderr = (fd?.stderr as string) || '';
    const cmd = extractField(data.parameters, 'cmd', 'command') as string || '';

    let html = `<div class="shell-prompt">$ ${escapeHtml(cmd)}</div>`;

    if (stdout) {
      html += `<pre class="tool-output-pre shell-stdout">${stringifyData(stdout)}</pre>`;
    }
    if (stderr) {
      html += `<pre class="tool-output-pre tool-error-output shell-stderr">${stringifyData(stderr)}</pre>`;
    }
    if (!stdout && !stderr) {
      const status = (fd?.status as string) || '';
      if (status === 'running') {
        html += `<div class="shell-running-hint">Running... use code/shell/watch to stream output</div>`;
      } else if (data.error) {
        html += `<pre class="tool-output-pre tool-error-output">${escapeHtml(data.error)}</pre>`;
      } else {
        html += `<pre class="tool-output-pre">${stringifyData(data.fullData)}</pre>`;
      }
    }
    return html;
  }
}

class ShellWatchToolRenderer implements ToolRenderer {
  renderCompact(data: ToolOutputContentData): string {
    const fd = data.fullData as Record<string, unknown> | undefined;
    const lines = fd?.lines as Array<{ text: string; classification: string }> | undefined;
    const finished = fd?.finished as boolean | undefined;
    const lineCount = lines?.length ?? 0;
    const suffix = finished ? ' (finished)' : ' (streaming)';
    return `<span class="tool-summary-text">${lineCount} line${lineCount !== 1 ? 's' : ''}${suffix}</span>`;
  }

  renderExpanded(data: ToolOutputContentData): string {
    const fd = data.fullData as Record<string, unknown> | undefined;
    const lines = fd?.lines as Array<{ text: string; classification: string }> | undefined;
    if (!lines || lines.length === 0) {
      return '<div class="shell-running-hint">No output yet</div>';
    }
    const rendered = lines.map(line => {
      const cls = line.classification || 'raw';
      return `<span class="shell-line-${cls}">${escapeHtml(line.text)}</span>`;
    }).join('\n');
    return `<pre class="tool-output-pre shell-watch-output">${rendered}</pre>`;
  }
}

class ShellStatusToolRenderer implements ToolRenderer {
  renderCompact(data: ToolOutputContentData): string {
    const fd = data.fullData as Record<string, unknown> | undefined;
    const cwd = fd?.cwd as string || '';
    const active = fd?.activeExecutions as number ?? 0;
    return `<span class="tool-file-path">${escapeHtml(shortenPath(cwd))}</span> `
      + `<span class="tool-summary-text">${active} active</span>`;
  }

  renderExpanded(data: ToolOutputContentData): string {
    return `<pre class="tool-output-pre">${stringifyData(data.fullData)}</pre>`;
  }
}

class ShellKillToolRenderer implements ToolRenderer {
  renderCompact(data: ToolOutputContentData): string {
    const execId = extractField(data.parameters, 'executionId') as string || '';
    return `<span class="tool-summary-text">killed ${escapeHtml(truncate(execId, 12))}</span>`;
  }

  renderExpanded(data: ToolOutputContentData): string {
    return `<pre class="tool-output-pre">${stringifyData(data.fullData)}</pre>`;
  }
}

class DefaultToolRenderer implements ToolRenderer {
  renderCompact(data: ToolOutputContentData): string {
    return `<span class="tool-summary-text">${escapeHtml(data.summary)}</span>`;
  }

  renderExpanded(data: ToolOutputContentData): string {
    return `<pre class="tool-output-pre">${stringifyData(data.fullData)}</pre>`;
  }
}

// ────────────────────────────────────────────────────────────
// ToolOutputAdapter
// ────────────────────────────────────────────────────────────

export class ToolOutputAdapter extends AbstractMessageAdapter<ToolOutputContentData> {
  private renderers = new Map<string, ToolRenderer>();
  private defaultRenderer = new DefaultToolRenderer();

  constructor() {
    super('text', { enableInteractions: true });

    this.renderers.set('code/write', new WriteToolRenderer());
    this.renderers.set('code/read', new ReadToolRenderer());
    this.renderers.set('code/edit', new EditToolRenderer());
    this.renderers.set('code/verify', new VerifyToolRenderer());
    this.renderers.set('code/git', new GitToolRenderer());
    this.renderers.set('code/search', new SearchToolRenderer());
    this.renderers.set('code/tree', new TreeToolRenderer());
    this.renderers.set('code/diff', new DiffToolRenderer());
    this.renderers.set('code/undo', new DefaultToolRenderer());
    this.renderers.set('code/history', new DefaultToolRenderer());

    // Shell command renderers
    this.renderers.set('code/shell/execute', new ShellExecuteToolRenderer());
    this.renderers.set('code/shell/watch', new ShellWatchToolRenderer());
    this.renderers.set('code/shell/status', new ShellStatusToolRenderer());
    this.renderers.set('code/shell/kill', new ShellKillToolRenderer());
    this.renderers.set('code/shell/sentinel', new DefaultToolRenderer());
  }

  parseContent(message: ChatMessageEntity): ToolOutputContentData | null {
    const meta = message.metadata;
    if (!meta?.toolResult) return null;

    const toolName = (meta.toolName as string) || 'unknown';
    const segments = toolName.split('/');

    return {
      toolName,
      toolAction: segments[segments.length - 1] || toolName,
      toolCategory: segments[0] || 'unknown',
      success: meta.success !== false,
      summary: message.content?.text || '',
      parameters: (meta.parameters as Record<string, unknown>) || {},
      fullData: meta.fullData,
      error: meta.error as string | undefined,
      media: message.content?.media,
      messageId: message.id,
    };
  }

  renderContent(data: ToolOutputContentData, _currentUserId: string): string {
    const renderer = this.renderers.get(data.toolName) ?? this.defaultRenderer;
    const statusClass = data.success ? 'tool-success' : 'tool-failure';
    const statusIcon = data.success ? '' : '<span class="tool-status-icon">&#10060;</span>';
    const icon = this.getToolIcon(data.toolCategory);

    const compactHtml = renderer.renderCompact(data);
    const expandedHtml = renderer.renderExpanded(data);
    const mediaHtml = this.renderInlineMedia(data.media);

    return `
      <details class="tool-output-card ${statusClass}" data-tool-name="${escapeHtml(data.toolName)}">
        <summary class="tool-output-summary">
          <span class="tool-icon">${icon}</span>
          <span class="tool-name">${escapeHtml(data.toolName)}</span>
          <span class="tool-compact-info">${compactHtml}</span>
          ${statusIcon}
        </summary>
        <div class="tool-output-detail">
          ${expandedHtml}
          ${mediaHtml}
          <div class="tool-output-actions">
            <button class="tool-action-btn" data-action="tool-open-tab" data-message-id="${data.messageId}" data-tool-name="${escapeHtml(data.toolName)}" title="Open in tab">Open</button>
            <button class="tool-action-btn" data-action="tool-copy" data-message-id="${data.messageId}" title="Copy output">Copy</button>
          </div>
        </div>
      </details>
    `;
  }

  async handleContentLoading(_element: HTMLElement): Promise<void> {
    // Tool outputs are synchronous text — no async loading needed
  }

  getContentClasses(): string[] {
    return ['tool-output-adapter'];
  }

  // ────────────────────────────────────────────────────────────
  // Helpers
  // ────────────────────────────────────────────────────────────

  private getToolIcon(category: string): string {
    const icons: Record<string, string> = {
      code: '&#9654;',            // play/arrow for code operations
      screenshot: '&#128247;',    // camera
      ai: '&#129302;',            // robot
      collaboration: '&#128101;', // people
      data: '&#128451;',          // file cabinet
    };
    return icons[category] || '&#128295;'; // wrench default
  }

  private renderInlineMedia(media?: readonly MediaItem[]): string {
    if (!media || media.length === 0) return '';
    const images = media.filter(m => m.type === 'image');
    if (images.length === 0) return '';

    return images.map((item, idx) => {
      const url = item.url ?? (item.base64 ? `data:${item.mimeType ?? 'image/png'};base64,${item.base64}` : '');
      if (!url) return '';
      const alt = item.alt ?? item.description ?? `Tool output ${idx + 1}`;
      return `<div class="tool-output-image"><img src="${url}" alt="${escapeHtml(alt)}" class="tool-inline-image" loading="lazy" /></div>`;
    }).join('');
  }

  // ────────────────────────────────────────────────────────────
  // Static action handlers (used by MessageEventDelegator)
  // ────────────────────────────────────────────────────────────

  static handleCopy(target: HTMLElement): void {
    const card = target.closest('.tool-output-card');
    const pre = card?.querySelector('.tool-output-pre');
    if (pre?.textContent && typeof navigator !== 'undefined' && navigator.clipboard) {
      navigator.clipboard.writeText(pre.textContent).then(() => {
        const original = target.textContent;
        target.textContent = 'Copied!';
        setTimeout(() => { target.textContent = original; }, 1500);
      });
    }
  }

  static handleOpenInTab(target: HTMLElement): void {
    const card = target.closest('.tool-output-card');
    const toolName = target.dataset.toolName || 'Tool Output';
    const messageId = target.dataset.messageId || '';

    // Get the expanded content directly from the DOM (already rendered in <pre>)
    const pre = card?.querySelector('.tool-output-pre');
    const content = pre?.textContent || '';

    // Store content for LogViewerWidget to pick up
    ToolOutputAdapter.storeInlineContent(messageId, content, toolName);

    // Open a diagnostics-log tab — MainWidget routes to LogViewerWidget
    Events.emit('content:opened', {
      contentType: 'diagnostics-log',
      entityId: `tool:${messageId}`,
      uniqueId: `tool:${messageId}`,
      title: toolName,
      setAsCurrent: true
    });
  }

  // ────────────────────────────────────────────────────────────
  // Transient content store — LogViewerWidget reads from this
  // ────────────────────────────────────────────────────────────

  private static _contentStore = new Map<string, { content: string; toolName: string }>();

  static storeInlineContent(key: string, content: string, toolName: string): void {
    ToolOutputAdapter._contentStore.set(key, { content, toolName });
  }

  static getInlineContent(key: string): { content: string; toolName: string } | undefined {
    const entry = ToolOutputAdapter._contentStore.get(key);
    if (entry) ToolOutputAdapter._contentStore.delete(key); // One-time read, prevent leak
    return entry;
  }

  // ────────────────────────────────────────────────────────────
  // CSS
  // ────────────────────────────────────────────────────────────

  getCSS(): string {
    return `
      /* ToolOutputAdapter — Rich tool result cards */
      .tool-output-adapter {
        margin: 2px 0;
      }

      .tool-output-card {
        border: 1px solid rgba(175, 184, 193, 0.3);
        border-radius: 6px;
        margin: 4px 0;
        font-family: 'SF Mono', Monaco, 'Cascadia Code', Consolas, monospace;
        font-size: 13px;
        overflow: hidden;
      }

      .tool-output-card.tool-success {
        border-left: 3px solid #2ea043;
        background: rgba(46, 160, 67, 0.04);
      }

      .tool-output-card.tool-failure {
        border-left: 3px solid #d73a49;
        background: rgba(215, 58, 73, 0.04);
      }

      .tool-output-summary {
        display: flex;
        align-items: center;
        gap: 8px;
        padding: 6px 10px;
        cursor: pointer;
        user-select: none;
        font-weight: 500;
        color: rgba(255, 255, 255, 0.85);
        list-style: none;
      }

      .tool-output-summary::-webkit-details-marker {
        display: none;
      }

      .tool-output-summary::before {
        content: '\\25B6';
        font-size: 10px;
        transition: transform 0.15s;
        opacity: 0.5;
      }

      .tool-output-card[open] > .tool-output-summary::before {
        transform: rotate(90deg);
      }

      .tool-output-summary:hover {
        background: rgba(175, 184, 193, 0.1);
      }

      .tool-output-card[open] .tool-output-summary {
        border-bottom: 1px solid rgba(175, 184, 193, 0.2);
      }

      .tool-icon {
        flex-shrink: 0;
        opacity: 0.6;
        font-size: 12px;
      }

      .tool-name {
        color: #58a6ff;
        font-weight: 600;
        white-space: nowrap;
        font-size: 12px;
      }

      .tool-compact-info {
        flex: 1;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        color: rgba(255, 255, 255, 0.6);
      }

      .tool-status-icon {
        flex-shrink: 0;
      }

      .tool-file-path {
        color: #d2a8ff;
      }

      .tool-edit-mode {
        color: rgba(255, 255, 255, 0.4);
        font-style: italic;
        margin-left: 4px;
      }

      .tool-search-query {
        color: #ffa657;
        font-style: italic;
      }

      .git-hash {
        color: #d2a8ff;
        font-family: 'SF Mono', Monaco, Consolas, monospace;
      }

      .verify-pass { color: #3fb950; font-weight: 600; }
      .verify-fail { color: #f85149; font-weight: 600; }

      .tool-summary-text {
        color: rgba(255, 255, 255, 0.6);
      }

      /* Expanded detail view */
      .tool-output-detail {
        padding: 8px 10px;
        max-height: 400px;
        overflow-y: auto;
      }

      .tool-output-pre {
        background: #161b22;
        color: #c9d1d9;
        padding: 10px;
        border-radius: 4px;
        overflow-x: auto;
        font-size: 12px;
        line-height: 1.5;
        margin: 0;
        white-space: pre-wrap;
        word-break: break-all;
      }

      .tool-error-output {
        border: 1px solid rgba(248, 81, 73, 0.3);
        background: rgba(248, 81, 73, 0.06);
      }

      /* Diff highlighting */
      .diff-add { color: #3fb950; }
      .diff-remove { color: #f85149; }
      .diff-hunk { color: #d2a8ff; font-weight: 600; }

      /* Tree output */
      .tool-tree-output {
        color: #8b949e;
      }

      /* Action buttons */
      .tool-output-actions {
        display: flex;
        justify-content: flex-end;
        padding: 6px 0 0 0;
        gap: 6px;
      }

      .tool-action-btn {
        background: rgba(175, 184, 193, 0.1);
        border: 1px solid rgba(175, 184, 193, 0.2);
        border-radius: 4px;
        padding: 2px 8px;
        font-size: 11px;
        cursor: pointer;
        color: rgba(255, 255, 255, 0.5);
        font-family: inherit;
      }

      .tool-action-btn:hover {
        background: rgba(175, 184, 193, 0.2);
        color: rgba(255, 255, 255, 0.8);
      }

      /* Shell execute output */
      .shell-cmd {
        color: #ffa657;
        font-family: 'SF Mono', Monaco, 'Cascadia Code', Consolas, monospace;
      }

      .shell-status-ok { color: #3fb950; font-weight: 600; }
      .shell-status-running { color: #58a6ff; font-weight: 600; }
      .shell-status-fail { color: #f85149; font-weight: 600; }

      .shell-prompt {
        padding: 6px 10px;
        background: #0d1117;
        border-radius: 4px 4px 0 0;
        color: #ffa657;
        font-family: 'SF Mono', Monaco, 'Cascadia Code', Consolas, monospace;
        font-size: 12px;
        font-weight: 600;
        border-bottom: 1px solid rgba(175, 184, 193, 0.15);
      }

      .shell-stdout {
        border-radius: 0 0 4px 4px;
        margin-top: 0;
      }

      .shell-stderr {
        margin-top: 4px;
      }

      .shell-running-hint {
        color: #58a6ff;
        font-size: 12px;
        font-style: italic;
        padding: 8px 0;
      }

      /* Shell watch classified lines */
      .shell-watch-output {
        line-height: 1.6;
      }

      .shell-line-error { color: #f85149; }
      .shell-line-warning { color: #d29922; }
      .shell-line-success { color: #3fb950; }
      .shell-line-info { color: #58a6ff; }
      .shell-line-verbose { color: #8b949e; }
      .shell-line-raw { color: #c9d1d9; }

      /* Inline images from tool output */
      .tool-output-image {
        margin: 8px 0;
      }

      .tool-inline-image {
        display: block;
        max-width: 100%;
        max-height: 300px;
        border: 1px solid rgba(175, 184, 193, 0.2);
        border-radius: 4px;
      }
    `;
  }
}
