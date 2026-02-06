/**
 * TerminalWidget - tmux-like terminal multiplexer for AI shell sessions
 *
 * Shows all AI persona shell sessions with live output streaming.
 * Left panel: execution list (running/completed/failed)
 * Right panel: selected execution's output stream
 * Bottom bar: session info (persona, cwd, controls)
 *
 * Uses code/shell/status to discover sessions,
 * code/shell/watch for live output streaming,
 * code/shell/kill to abort executions.
 */

import { ReactiveWidget, html, css, reactive, type TemplateResult } from '../shared/ReactiveWidget';
import { PositronWidgetState } from '../shared/services/state/PositronWidgetState';
import type { CommandParams, CommandResult } from '@system/core/types/JTAGTypes';

// ────────────────────────────────────────────────────────────
// Types (mirror server-side shell types for browser use)
// ────────────────────────────────────────────────────────────

interface ShellExecution {
  executionId: string;
  cmd: string;
  status: 'running' | 'completed' | 'failed' | 'timed_out' | 'killed';
  personaName: string;
  personaId: string;
  startedAt: number;
  lines: ClassifiedLine[];
  exitCode?: number;
}

interface ClassifiedLine {
  text: string;
  classification: 'error' | 'warning' | 'info' | 'success' | 'verbose' | 'raw';
  timestamp: number;
}

interface ShellStatusResult extends CommandResult {
  success: boolean;
  shellSessionId: string;
  personaId: string;
  cwd: string;
  workspaceRoot: string;
  activeExecutions: number;
  totalExecutions: number;
}

interface ShellWatchResult extends CommandResult {
  success: boolean;
  executionId: string;
  finished: boolean;
  exitCode?: number;
  lines: Array<{ text: string; classification: string }>;
}

// ────────────────────────────────────────────────────────────
// Widget
// ────────────────────────────────────────────────────────────

export class TerminalWidget extends ReactiveWidget {

  // ── Reactive State ──────────────────────────────────────
  @reactive() private executions: ShellExecution[] = [];
  @reactive() private selectedId: string | null = null;
  @reactive() private statusInfo: string = 'No active sessions';
  @reactive() private autoScroll = true;

  // ── Polling ─────────────────────────────────────────────
  private _statusPollTimer: ReturnType<typeof setInterval> | null = null;
  private _watchAbort: AbortController | null = null;

  // ── Styles ──────────────────────────────────────────────
  static override styles = css`
    :host {
      display: block;
      height: 100%;
      font-family: var(--font-mono, 'SF Mono', 'Fira Code', 'Cascadia Code', monospace);
      font-size: 13px;
    }

    .terminal-layout {
      display: grid;
      grid-template-columns: 240px 1fr;
      grid-template-rows: 1fr auto;
      height: 100%;
      background: var(--background-color, #0a0e1a);
      color: var(--text-primary, #e0e0e0);
    }

    /* ── Left Panel: Execution List ──────────────────── */
    .exec-panel {
      grid-row: 1 / 3;
      border-right: 1px solid var(--border-color, #1e2a3a);
      overflow-y: auto;
      padding: 0;
    }

    .exec-panel-header {
      padding: 12px 16px;
      font-size: 11px;
      font-weight: 600;
      text-transform: uppercase;
      letter-spacing: 0.08em;
      color: var(--text-tertiary, #666);
      border-bottom: 1px solid var(--border-color, #1e2a3a);
      position: sticky;
      top: 0;
      background: var(--background-color, #0a0e1a);
      z-index: 1;
    }

    .exec-item {
      padding: 10px 16px;
      cursor: pointer;
      border-left: 3px solid transparent;
      border-bottom: 1px solid var(--border-color, #1e2a3a);
      transition: background-color 0.12s ease;
    }

    .exec-item:hover {
      background: var(--hover-background, rgba(255, 255, 255, 0.03));
    }

    .exec-item.selected {
      border-left-color: var(--accent-color, #00c8ff);
      background: var(--active-background, rgba(0, 200, 255, 0.06));
    }

    .exec-cmd {
      font-size: 12px;
      color: var(--text-primary, #e0e0e0);
      white-space: nowrap;
      overflow: hidden;
      text-overflow: ellipsis;
      margin-bottom: 4px;
    }

    .exec-meta {
      display: flex;
      align-items: center;
      gap: 8px;
      font-size: 10px;
      color: var(--text-tertiary, #666);
    }

    .exec-badge {
      display: inline-flex;
      align-items: center;
      gap: 4px;
      padding: 1px 6px;
      border-radius: 3px;
      font-size: 10px;
      font-weight: 500;
    }

    .exec-badge.running {
      background: rgba(0, 200, 255, 0.15);
      color: #00c8ff;
    }

    .exec-badge.completed {
      background: rgba(0, 200, 100, 0.15);
      color: #00c864;
    }

    .exec-badge.failed, .exec-badge.killed, .exec-badge.timed_out {
      background: rgba(255, 80, 80, 0.15);
      color: #ff5050;
    }

    .exec-persona {
      color: var(--text-tertiary, #666);
    }

    .exec-empty {
      padding: 24px 16px;
      text-align: center;
      color: var(--text-tertiary, #555);
      font-size: 12px;
      line-height: 1.6;
    }

    /* ── Right Panel: Output Stream ──────────────────── */
    .output-panel {
      overflow-y: auto;
      padding: 12px 16px;
      scroll-behavior: smooth;
    }

    .output-line {
      white-space: pre-wrap;
      word-break: break-all;
      line-height: 1.5;
      padding: 0 4px;
    }

    .output-line.error { color: #ff5050; }
    .output-line.warning { color: #ffaa00; }
    .output-line.success { color: #00c864; }
    .output-line.info { color: #00c8ff; }
    .output-line.verbose { color: #666; }
    .output-line.raw { color: var(--text-secondary, #aaa); }

    .output-empty {
      display: flex;
      align-items: center;
      justify-content: center;
      height: 100%;
      color: var(--text-tertiary, #555);
      font-size: 13px;
    }

    /* ── Bottom Bar ──────────────────────────────────── */
    .bottom-bar {
      display: flex;
      align-items: center;
      gap: 16px;
      padding: 8px 16px;
      border-top: 1px solid var(--border-color, #1e2a3a);
      font-size: 11px;
      color: var(--text-tertiary, #666);
      background: var(--surface-color, #0d1220);
    }

    .bottom-bar .status-text {
      flex: 1;
    }

    .bottom-bar button {
      background: transparent;
      border: 1px solid var(--border-color, #1e2a3a);
      color: var(--text-secondary, #aaa);
      padding: 4px 12px;
      border-radius: 4px;
      font-size: 11px;
      font-family: inherit;
      cursor: pointer;
      transition: background-color 0.12s ease, color 0.12s ease;
    }

    .bottom-bar button:hover {
      background: var(--hover-background, rgba(255, 255, 255, 0.05));
      color: var(--text-primary, #e0e0e0);
    }

    .bottom-bar button.danger:hover {
      background: rgba(255, 80, 80, 0.15);
      color: #ff5050;
    }

    .pulse {
      display: inline-block;
      width: 6px;
      height: 6px;
      border-radius: 50%;
      background: #00c8ff;
      animation: pulse-anim 1.5s infinite;
    }

    @keyframes pulse-anim {
      0%, 100% { opacity: 1; }
      50% { opacity: 0.3; }
    }
  `;

  // ── Lifecycle ───────────────────────────────────────────

  protected override onConnect(): void {
    super.onConnect();
    this.emitPositronContext();
    this.startStatusPolling();
  }

  protected override onDisconnect(): void {
    super.onDisconnect();
    this.stopStatusPolling();
    this.stopWatching();
  }

  // ── Positron Context ────────────────────────────────────

  private emitPositronContext(): void {
    PositronWidgetState.emit(
      {
        widgetType: 'terminal',
        title: 'Terminal',
        metadata: {
          activeExecutions: this.executions.filter(e => e.status === 'running').length,
          totalExecutions: this.executions.length,
        }
      },
      { action: 'viewing', target: 'terminal' }
    );
  }

  // ── Status Polling ──────────────────────────────────────

  private startStatusPolling(): void {
    this.pollShellStatus();
    this._statusPollTimer = setInterval(() => this.pollShellStatus(), 5000);
  }

  private stopStatusPolling(): void {
    if (this._statusPollTimer) {
      clearInterval(this._statusPollTimer);
      this._statusPollTimer = null;
    }
  }

  private async pollShellStatus(): Promise<void> {
    try {
      // Get list of all users (personas) that may have shell sessions
      const usersResult = await this.executeCommand<CommandParams, CommandResult & { items?: any[] }>('data/list', {
        collection: 'users',
        filter: { type: 'ai' },
        limit: 50,
      } as any);

      if (!usersResult?.items?.length) {
        this.statusInfo = 'No AI personas active';
        return;
      }

      const activePersonas: string[] = [];

      // Check shell status for each persona
      for (const user of usersResult.items) {
        try {
          const status = await this.executeCommand<CommandParams, ShellStatusResult>(
            'code/shell/status',
            { userId: user.id } as any,
          );
          if (status?.success) {
            activePersonas.push(user.displayName || user.uniqueId);
            this.statusInfo = `${activePersonas.length} active session${activePersonas.length > 1 ? 's' : ''} | ${status.cwd}`;
          }
        } catch {
          // No shell session for this persona — skip
        }
      }

      if (activePersonas.length === 0) {
        this.statusInfo = 'No active shell sessions';
      }
    } catch (err) {
      this.statusInfo = 'Status poll failed';
    }
  }

  // ── Watch Loop ──────────────────────────────────────────

  private async startWatching(executionId: string, personaId: string): Promise<void> {
    this.stopWatching();
    this._watchAbort = new AbortController();

    const exec = this.executions.find(e => e.executionId === executionId);
    if (!exec || exec.status !== 'running') return;

    try {
      while (!this._watchAbort.signal.aborted) {
        const result = await this.executeCommand<CommandParams, ShellWatchResult>(
          'code/shell/watch',
          { executionId, userId: personaId } as any,
        );

        if (!result?.success) break;

        // Append new lines
        if (result.lines?.length) {
          const newLines: ClassifiedLine[] = result.lines.map(l => ({
            text: l.text,
            classification: l.classification as ClassifiedLine['classification'],
            timestamp: Date.now(),
          }));
          exec.lines = [...exec.lines, ...newLines];
          this.requestUpdate();
          this.scrollToBottom();
        }

        if (result.finished) {
          exec.status = result.exitCode === 0 ? 'completed' : 'failed';
          exec.exitCode = result.exitCode;
          this.requestUpdate();
          break;
        }
      }
    } catch {
      // Watch ended (connection lost, abort, etc.)
    }
  }

  private stopWatching(): void {
    if (this._watchAbort) {
      this._watchAbort.abort();
      this._watchAbort = null;
    }
  }

  // ── Actions ─────────────────────────────────────────────

  private selectExecution(executionId: string): void {
    this.selectedId = executionId;
    const exec = this.executions.find(e => e.executionId === executionId);
    if (exec?.status === 'running') {
      this.startWatching(executionId, exec.personaId);
    }
    this.scrollToBottom();
  }

  private async killExecution(): Promise<void> {
    const exec = this.selectedExecution;
    if (!exec || exec.status !== 'running') return;

    try {
      await this.executeCommand<CommandParams, CommandResult>(
        'code/shell/kill',
        { executionId: exec.executionId, userId: exec.personaId } as any,
      );
      exec.status = 'killed';
      this.stopWatching();
      this.requestUpdate();
    } catch (err) {
      console.error('Kill failed:', err);
    }
  }

  private clearCompleted(): void {
    this.executions = this.executions.filter(e => e.status === 'running');
    if (this.selectedId && !this.executions.find(e => e.executionId === this.selectedId)) {
      this.selectedId = null;
    }
  }

  // ── Helpers ─────────────────────────────────────────────

  private get selectedExecution(): ShellExecution | undefined {
    return this.executions.find(e => e.executionId === this.selectedId);
  }

  private scrollToBottom(): void {
    if (!this.autoScroll) return;
    requestAnimationFrame(() => {
      const output = this.shadowRoot?.querySelector('.output-panel');
      if (output) {
        output.scrollTop = output.scrollHeight;
      }
    });
  }

  private formatElapsed(startedAt: number): string {
    const seconds = Math.floor((Date.now() - startedAt) / 1000);
    if (seconds < 60) return `${seconds}s`;
    const minutes = Math.floor(seconds / 60);
    return `${minutes}m ${seconds % 60}s`;
  }

  // ── Render ──────────────────────────────────────────────

  override render(): TemplateResult {
    const selected = this.selectedExecution;
    const runningCount = this.executions.filter(e => e.status === 'running').length;

    return html`
      <div class="terminal-layout">
        <!-- Left Panel: Execution List -->
        <div class="exec-panel">
          <div class="exec-panel-header">
            Executions ${runningCount > 0 ? html`<span class="pulse"></span>` : ''}
          </div>
          ${this.executions.length === 0
            ? html`
              <div class="exec-empty">
                No shell executions yet.<br/>
                AI personas will appear here when they run commands
                via <code>code/shell/execute</code>.
              </div>`
            : this.executions.map(exec => html`
              <div
                class="exec-item ${exec.executionId === this.selectedId ? 'selected' : ''}"
                @click=${() => this.selectExecution(exec.executionId)}
              >
                <div class="exec-cmd">$ ${exec.cmd}</div>
                <div class="exec-meta">
                  <span class="exec-badge ${exec.status}">
                    ${exec.status === 'running' ? html`<span class="pulse"></span>` : ''}
                    ${exec.status}
                  </span>
                  <span class="exec-persona">${exec.personaName}</span>
                  <span>${this.formatElapsed(exec.startedAt)}</span>
                </div>
              </div>
            `)
          }
        </div>

        <!-- Right Panel: Output Stream -->
        <div class="output-panel">
          ${selected
            ? selected.lines.length === 0
              ? html`<div class="output-empty">Waiting for output...</div>`
              : selected.lines.map(line => html`
                <div class="output-line ${line.classification}">${line.text}</div>
              `)
            : html`<div class="output-empty">Select an execution to view output</div>`
          }
        </div>

        <!-- Bottom Bar -->
        <div class="bottom-bar">
          <span class="status-text">${this.statusInfo}</span>
          ${selected?.status === 'running'
            ? html`<button class="danger" @click=${() => this.killExecution()}>Kill</button>`
            : ''}
          <button @click=${() => this.clearCompleted()}>Clear Completed</button>
        </div>
      </div>
    `;
  }
}

// Registration handled by centralized BROWSER_WIDGETS registry
