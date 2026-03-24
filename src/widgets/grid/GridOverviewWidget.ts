/**
 * GridOverviewWidget — Full-tab network dashboard
 *
 * Sections:
 *   - Node card grid: name, status, latency, GPU info, trust level, capabilities
 *   - Transport status bar (Tailscale/Reticulum health)
 *   - Active jobs table (training/inference forwarded to remote nodes)
 *   - Routing decision log (recent command routing)
 *   - Per-node ping button
 *
 * All data via GRID_EVENTS + grid/* commands.
 */

import {
  ReactiveWidget,
  html,
  reactive,
  css,
  type TemplateResult,
  type CSSResultGroup,
} from '../shared/ReactiveWidget';
import { nothing } from 'lit';
import { Events } from '../../system/core/shared/Events';
import {
  GRID_EVENTS,
  type GridNodeJoinedEventData,
  type GridNodeLeftEventData,
  type GridNodeHealthChangedEventData,
  type GridRouteDecisionEventData,
  type GridCommandForwardedEventData,
  type GridNodeStatus,
  type GridTransport,
} from '../../system/events/shared/GridEvents';
import '../shared/ContinuumChart';
import type { ContinuumChartSeries } from '../shared/ContinuumChart';
import { normalizeGridNodes } from './GridDataNormalizer';

// ── Types ───────────────────────────────────────────────────────────────────

interface GridNode {
  nodeId: string;
  nodeName: string;
  status: GridNodeStatus;
  latencyMs: number;
  transport: GridTransport;
  address: string;
  capabilities: string[];
  gpu?: { name: string; vramMb: number };
  gpuUtilization?: number;
  gpuMemoryUsedMb?: number;
  pinging: boolean;
  latencyHistory: { step: number; latencyMs: number }[];
}

interface RoutingLogEntry {
  timestamp: number;
  command: string;
  targetNodeName: string;
  reason: string;
  durationMs?: number;
  success?: boolean;
}

const MAX_LOG_ENTRIES = 50;
const MAX_LATENCY_HISTORY = 60;

const STATUS_COLORS: Record<GridNodeStatus, string> = {
  online: 'var(--status-online, #00ff88)',
  degraded: 'var(--status-away, #ffaa00)',
  offline: 'var(--status-offline, #666666)',
};

const LATENCY_SERIES: ContinuumChartSeries[] = [
  { key: 'latencyMs', color: 'rgba(0, 212, 255, 0.9)', label: 'Latency (ms)' },
];

// ── Component ───────────────────────────────────────────────────────────────

export class GridOverviewWidget extends ReactiveWidget {
  static override styles = [
    ReactiveWidget.styles,
    css`
      :host {
        display: block;
        width: 100%;
        height: 100%;
        overflow-y: auto;
        color: var(--content-primary, #e0e6ed);
        font-family: var(--font-primary, sans-serif);
      }

      .grid-dashboard {
        padding: 16px 20px;
        max-width: 1200px;
        margin: 0 auto;
      }

      .dashboard-header {
        display: flex;
        justify-content: space-between;
        align-items: center;
        margin-bottom: 20px;
      }

      .dashboard-title {
        font-size: 18px;
        font-weight: 700;
      }

      .dashboard-subtitle {
        font-size: 11px;
        color: var(--content-secondary, #8a92a5);
        margin-top: 2px;
      }

      .section-label {
        font-size: 10px;
        font-weight: 700;
        color: var(--content-secondary, #8a92a5);
        text-transform: uppercase;
        letter-spacing: 0.8px;
        margin-bottom: 10px;
        margin-top: 24px;
      }

      /* Transport status bar */
      .transport-bar {
        display: flex;
        gap: 12px;
        margin-bottom: 20px;
      }

      .transport-status {
        display: flex;
        align-items: center;
        gap: 6px;
        padding: 6px 12px;
        border-radius: 4px;
        font-size: 11px;
        font-weight: 600;
      }

      .transport-active {
        background: rgba(0, 255, 136, 0.08);
        border: 1px solid rgba(0, 255, 136, 0.2);
        color: rgba(0, 255, 136, 0.9);
      }

      .transport-inactive {
        background: rgba(100, 100, 100, 0.08);
        border: 1px solid rgba(100, 100, 100, 0.15);
        color: var(--content-secondary, #8a92a5);
      }

      .transport-dot {
        width: 6px;
        height: 6px;
        border-radius: 50%;
      }

      /* Node cards */
      .node-grid {
        display: grid;
        grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
        gap: 12px;
        margin-bottom: 20px;
      }

      .node-card {
        background: rgba(15, 20, 25, 0.6);
        border: 1px solid var(--border-subtle, rgba(255, 255, 255, 0.1));
        border-radius: 6px;
        padding: 12px 14px;
        transition: border-color 0.2s ease;
      }

      .node-card:hover {
        border-color: var(--border-accent, rgba(0, 212, 255, 0.4));
      }

      .node-card-header {
        display: flex;
        justify-content: space-between;
        align-items: center;
        margin-bottom: 8px;
      }

      .node-name-row {
        display: flex;
        align-items: center;
        gap: 6px;
      }

      .status-dot {
        width: 8px;
        height: 8px;
        border-radius: 50%;
        flex-shrink: 0;
      }

      .node-name {
        font-weight: 700;
        font-size: 13px;
      }

      .node-transport-badge {
        font-size: 9px;
        font-weight: 700;
        font-family: var(--font-mono, monospace);
        text-transform: uppercase;
        padding: 1px 5px;
        border-radius: 3px;
        background: rgba(0, 212, 255, 0.1);
        color: rgba(0, 212, 255, 0.8);
        border: 1px solid rgba(0, 212, 255, 0.2);
      }

      .node-stats {
        display: grid;
        grid-template-columns: 1fr 1fr;
        gap: 4px 12px;
        font-size: 10px;
        color: var(--content-secondary, #8a92a5);
        margin-bottom: 8px;
      }

      .node-stat-value {
        font-family: var(--font-mono, monospace);
        color: var(--content-primary, #e0e6ed);
        font-weight: 600;
      }

      .node-capabilities {
        display: flex;
        gap: 4px;
        flex-wrap: wrap;
        margin-bottom: 8px;
      }

      .capability-tag {
        font-size: 9px;
        padding: 1px 5px;
        border-radius: 3px;
        background: rgba(255, 255, 255, 0.05);
        color: var(--content-secondary, #8a92a5);
        border: 1px solid rgba(255, 255, 255, 0.08);
      }

      .node-actions {
        display: flex;
        gap: 6px;
        margin-top: 8px;
      }

      .ping-btn {
        padding: 3px 10px;
        font-size: 10px;
        font-weight: 600;
        border-radius: 3px;
        border: 1px solid rgba(0, 212, 255, 0.3);
        background: rgba(0, 212, 255, 0.08);
        color: rgba(0, 212, 255, 0.9);
        cursor: pointer;
        transition: all 0.15s ease;
      }

      .ping-btn:hover {
        background: rgba(0, 212, 255, 0.15);
        border-color: rgba(0, 212, 255, 0.5);
      }

      .ping-btn:disabled {
        opacity: 0.5;
        cursor: not-allowed;
      }

      /* Routing log */
      .routing-log {
        width: 100%;
        border-collapse: collapse;
        font-size: 11px;
      }

      .routing-log th {
        text-align: left;
        padding: 5px 8px;
        font-weight: 700;
        color: var(--content-secondary, #8a92a5);
        text-transform: uppercase;
        letter-spacing: 0.5px;
        font-size: 9px;
        border-bottom: 1px solid var(--border-subtle, rgba(255, 255, 255, 0.1));
      }

      .routing-log td {
        padding: 4px 8px;
        border-bottom: 1px solid rgba(255, 255, 255, 0.04);
      }

      .route-success { color: var(--content-success, #00ff88); }
      .route-fail { color: var(--content-error, #ff5050); }

      .empty-state {
        text-align: center;
        padding: 40px 20px;
        color: var(--content-secondary, #8a92a5);
      }

      .empty-state-title {
        font-size: 16px;
        font-weight: 600;
        margin-bottom: 8px;
      }

      .empty-state-hint {
        font-size: 12px;
        font-style: italic;
      }
    `,
  ] as CSSResultGroup;

  // ── State ───────────────────────────────────────────────────────────────

  @reactive() private _nodes: Map<string, GridNode> = new Map();
  @reactive() private _routingLog: RoutingLogEntry[] = [];

  private _cleanups: (() => void)[] = [];
  private _latencyStep = 0;

  constructor() {
    super({ widgetName: 'GridOverviewWidget' });
  }

  // ── Lifecycle ───────────────────────────────────────────────────────────

  protected override onFirstRender(): void {
    super.onFirstRender();

    this._cleanups.push(
      Events.subscribe(GRID_EVENTS.NODE_JOINED, (data: GridNodeJoinedEventData) => {
        const updated = new Map(this._nodes);
        updated.set(data.nodeId, {
          nodeId: data.nodeId,
          nodeName: data.nodeName,
          status: 'online',
          latencyMs: 0,
          transport: data.transport,
          address: data.address,
          capabilities: data.capabilities,
          gpu: data.gpu,
          pinging: false,
          latencyHistory: [],
        });
        this._nodes = updated;
      }),

      Events.subscribe(GRID_EVENTS.NODE_LEFT, (data: GridNodeLeftEventData) => {
        const updated = new Map(this._nodes);
        updated.delete(data.nodeId);
        this._nodes = updated;
      }),

      Events.subscribe(GRID_EVENTS.NODE_HEALTH_CHANGED, (data: GridNodeHealthChangedEventData) => {
        const existing = this._nodes.get(data.nodeId);
        if (!existing) return;

        this._latencyStep++;
        const history = [...existing.latencyHistory, { step: this._latencyStep, latencyMs: data.latencyMs }];
        if (history.length > MAX_LATENCY_HISTORY) history.shift();

        const updated = new Map(this._nodes);
        updated.set(data.nodeId, {
          ...existing,
          status: data.status,
          latencyMs: data.latencyMs,
          gpuUtilization: data.gpuUtilization ?? existing.gpuUtilization,
          gpuMemoryUsedMb: data.gpuMemoryUsedMb ?? existing.gpuMemoryUsedMb,
          latencyHistory: history,
        });
        this._nodes = updated;
      }),

      Events.subscribe(GRID_EVENTS.ROUTE_DECISION, (data: GridRouteDecisionEventData) => {
        const entry: RoutingLogEntry = {
          timestamp: data.timestamp,
          command: data.command,
          targetNodeName: data.targetNodeName,
          reason: data.reason,
        };
        this._routingLog = [entry, ...this._routingLog].slice(0, MAX_LOG_ENTRIES);
      }),

      Events.subscribe(GRID_EVENTS.COMMAND_FORWARDED, (data: GridCommandForwardedEventData) => {
        const entry: RoutingLogEntry = {
          timestamp: data.timestamp,
          command: data.command,
          targetNodeName: data.targetNodeName,
          reason: data.transport,
          durationMs: data.durationMs,
          success: data.success,
        };
        this._routingLog = [entry, ...this._routingLog].slice(0, MAX_LOG_ENTRIES);
      }),
    );

    this._loadInitialState();
  }

  override disconnectedCallback(): void {
    super.disconnectedCallback();
    this._cleanups.forEach(fn => fn());
    this._cleanups = [];
  }

  private async _loadInitialState(): Promise<void> {
    try {
      const result = await this.executeCommand<any, any>('grid/nodes', {});
      const normalized = normalizeGridNodes(result);

      if (normalized.length > 0) {
        const updated = new Map<string, GridNode>();
        for (const n of normalized) {
          updated.set(n.nodeId, {
            ...n,
            pinging: false,
            latencyHistory: [],
          });
        }
        this._nodes = updated;
      }
    } catch (err) {
      console.warn('[GridOverview] Failed to load grid nodes:', err);
    }
  }

  // ── Render ──────────────────────────────────────────────────────────────

  protected override renderContent(): TemplateResult {
    const nodes = [...this._nodes.values()];
    const onlineCount = nodes.filter(n => n.status === 'online').length;
    const transports = new Set(nodes.map(n => n.transport));

    return html`
      <div class="grid-dashboard">
        <div class="dashboard-header">
          <div>
            <div class="dashboard-title">Grid Overview</div>
            <div class="dashboard-subtitle">
              ${nodes.length > 0
                ? `${onlineCount}/${nodes.length} nodes online`
                : 'No nodes connected'}
            </div>
          </div>
        </div>

        ${this._renderTransportBar(transports)}

        ${nodes.length > 0 ? html`
          <div class="section-label">Nodes</div>
          <div class="node-grid">
            ${nodes
              .sort((a, b) => a.nodeName.localeCompare(b.nodeName))
              .map(n => this._renderNodeCard(n))}
          </div>
        ` : html`
          <div class="empty-state">
            <div class="empty-state-title">No Grid Nodes</div>
            <div class="empty-state-hint">
              Connect nodes via Tailscale or Reticulum to enable distributed compute.
              Grid nodes appear automatically when they join the mesh.
            </div>
          </div>
        `}

        ${this._routingLog.length > 0 ? this._renderRoutingLog() : nothing}
      </div>
    `;
  }

  // ── Transport bar ───────────────────────────────────────────────────────

  private _renderTransportBar(activeTransports: Set<string>): TemplateResult {
    const transports = ['tailscale', 'reticulum'];
    return html`
      <div class="transport-bar">
        ${transports.map(t => {
          const active = activeTransports.has(t);
          return html`
            <div class="transport-status ${active ? 'transport-active' : 'transport-inactive'}">
              <div class="transport-dot" style="background: ${active ? STATUS_COLORS.online : STATUS_COLORS.offline};"></div>
              ${t.charAt(0).toUpperCase() + t.slice(1)}
            </div>
          `;
        })}
      </div>
    `;
  }

  // ── Node card ───────────────────────────────────────────────────────────

  private _renderNodeCard(node: GridNode): TemplateResult {
    return html`
      <div class="node-card">
        <div class="node-card-header">
          <div class="node-name-row">
            <div class="status-dot" style="background: ${STATUS_COLORS[node.status]};"></div>
            <span class="node-name">${node.nodeName}</span>
          </div>
          <span class="node-transport-badge">${node.transport}</span>
        </div>

        <div class="node-stats">
          <span>Latency</span>
          <span class="node-stat-value">${node.latencyMs}ms</span>

          ${node.gpu ? html`
            <span>GPU</span>
            <span class="node-stat-value">${node.gpu.name}</span>
            <span>VRAM</span>
            <span class="node-stat-value">${(node.gpu.vramMb / 1024).toFixed(0)} GB</span>
          ` : nothing}

          ${node.gpuUtilization != null ? html`
            <span>GPU Load</span>
            <span class="node-stat-value">${node.gpuUtilization}%</span>
          ` : nothing}

          ${node.gpuMemoryUsedMb != null ? html`
            <span>GPU Mem</span>
            <span class="node-stat-value">${(node.gpuMemoryUsedMb / 1024).toFixed(1)} GB</span>
          ` : nothing}

          <span>Address</span>
          <span class="node-stat-value" style="font-size: 9px;">${node.address}</span>
        </div>

        ${node.capabilities.length > 0 ? html`
          <div class="node-capabilities">
            ${node.capabilities.map(c => html`<span class="capability-tag">${c}</span>`)}
          </div>
        ` : nothing}

        ${node.latencyHistory.length > 2 ? html`
          <continuum-chart
            .data=${node.latencyHistory}
            .series=${LATENCY_SERIES}
            .xKey=${'step'}
            .size=${'sparkline'}
            .streaming=${true}
            .yRange=${[0, 'auto'] as [number, 'auto']}
            .formatY=${(v: number) => `${v.toFixed(0)}ms`}
          ></continuum-chart>
        ` : nothing}

        <div class="node-actions">
          <button class="ping-btn"
            ?disabled=${node.pinging}
            @click=${() => this._pingNode(node.nodeId)}>
            ${node.pinging ? 'Pinging...' : 'Ping'}
          </button>
        </div>
      </div>
    `;
  }

  // ── Routing log ─────────────────────────────────────────────────────────

  private _renderRoutingLog(): TemplateResult {
    return html`
      <div class="section-label">Routing Log</div>
      <table class="routing-log">
        <thead>
          <tr>
            <th>Time</th>
            <th>Command</th>
            <th>Target</th>
            <th>Reason</th>
            <th>Duration</th>
            <th>Status</th>
          </tr>
        </thead>
        <tbody>
          ${this._routingLog.slice(0, 20).map(entry => html`
            <tr>
              <td style="font-family: var(--font-mono); font-size: 10px;">
                ${new Date(entry.timestamp).toLocaleTimeString()}
              </td>
              <td>${entry.command}</td>
              <td>${entry.targetNodeName}</td>
              <td>${entry.reason}</td>
              <td style="font-family: var(--font-mono);">
                ${entry.durationMs != null ? `${entry.durationMs}ms` : '--'}
              </td>
              <td>
                ${entry.success === true ? html`<span class="route-success">OK</span>` :
                  entry.success === false ? html`<span class="route-fail">FAIL</span>` :
                  html`<span style="color: var(--content-secondary);">--</span>`}
              </td>
            </tr>
          `)}
        </tbody>
      </table>
    `;
  }

  // ── Actions ─────────────────────────────────────────────────────────────

  private async _pingNode(nodeId: string): Promise<void> {
    const node = this._nodes.get(nodeId);
    if (!node) return;

    // Set pinging state
    const updated = new Map(this._nodes);
    updated.set(nodeId, { ...node, pinging: true });
    this._nodes = updated;

    try {
      const result = await this.executeCommand<any, any>('grid/ping', { nodeId });
      if (result.success) {
        const refreshed = this._nodes.get(nodeId);
        if (refreshed) {
          const updated2 = new Map(this._nodes);
          updated2.set(nodeId, {
            ...refreshed,
            latencyMs: result.latencyMs ?? refreshed.latencyMs,
            pinging: false,
          });
          this._nodes = updated2;
        }
      }
    } catch {
      // Reset pinging state
      const refreshed = this._nodes.get(nodeId);
      if (refreshed) {
        const updated2 = new Map(this._nodes);
        updated2.set(nodeId, { ...refreshed, pinging: false });
        this._nodes = updated2;
      }
    }
  }
}

// ── Register ────────────────────────────────────────────────────────────────

if (typeof customElements !== 'undefined' && !customElements.get('grid-overview-widget')) {
  customElements.define('grid-overview-widget', GridOverviewWidget);
}
