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
  unsafeCSS,
  type TemplateResult,
  type CSSResultGroup,
} from '../shared/ReactiveWidget';
import { styles as GRID_OVERVIEW_STYLES } from './public/grid-overview.styles';
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
import { normalizeGridNodes, GRID_STATUS_COLORS } from './GridDataNormalizer';

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
  gpuTemperatureC?: number;
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

// Use shared GRID_STATUS_COLORS from GridDataNormalizer

const LATENCY_SERIES: ContinuumChartSeries[] = [
  { key: 'latencyMs', color: 'rgba(0, 212, 255, 0.9)', label: 'Latency (ms)' },
];

// ── Component ───────────────────────────────────────────────────────────────

export class GridOverviewWidget extends ReactiveWidget {
  static override styles = [
    ReactiveWidget.styles,
    unsafeCSS(GRID_OVERVIEW_STYLES),
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
    // Show local machine immediately — don't wait on commands that may timeout
    const initial = new Map<string, GridNode>();
    initial.set('local', {
      nodeId: 'local',
      nodeName: 'This Machine',
      status: 'online' as GridNodeStatus,
      latencyMs: 0,
      transport: 'local',
      address: '127.0.0.1',
      capabilities: ['compute'],
      pinging: false,
      latencyHistory: [],
    });
    this._nodes = initial;

    // Load remote grid nodes in background (may timeout if Rust core offline)
    try {
      const result = await this.executeCommand<any, any>('grid/nodes', {});
      const normalized = normalizeGridNodes(result);

      if (normalized.length > 0) {
        const updated = new Map(this._nodes);
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

    // Compute totals for zone summary
    const totalVramGb = nodes.reduce((sum, n) => sum + (n.gpu ? n.gpu.vramMb / 1024 : 0), 0);
    const totalGpus = nodes.filter(n => n.gpu).length;

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
          <div class="header-actions">
            <button class="action-btn primary" @click=${() => this._pairNode()}>+ Pair Node</button>
            <button class="action-btn" @click=${() => this._loadInitialState()}>Refresh</button>
          </div>
        </div>

        ${this._renderTransportBar(transports)}

        ${nodes.length > 0 ? html`
          <div class="zone-header">
            <div class="zone-ring inner"></div>
            <div class="zone-info">
              <span class="zone-name">My Grid</span>
              <span class="zone-trust">Owner</span>
            </div>
            <div class="zone-stats">
              <span class="zone-stat">${onlineCount} nodes</span>
              <span class="zone-stat">${totalGpus} GPUs</span>
              <span class="zone-stat">${totalVramGb.toFixed(0)}GB VRAM</span>
            </div>
          </div>

          <div class="node-grid">
            ${nodes
              .sort((a, b) => {
                // Online first, then by name
                if (a.status !== b.status) return a.status === 'online' ? -1 : 1;
                return a.nodeName.localeCompare(b.nodeName);
              })
              .map(n => this._renderNodeCard(n))}
          </div>

          <div class="zone-future">
            <div class="zone-ring middle"></div>
            <span class="zone-future-label">Team Grid</span>
            <span class="zone-future-hint">Share compute with trusted teams</span>
          </div>
          <div class="zone-future">
            <div class="zone-ring outer"></div>
            <span class="zone-future-label">Public Grid</span>
            <span class="zone-future-hint">Trade compute via forge-alloy contracts</span>
          </div>
        ` : html`
          <div class="empty-state">
            <div class="empty-state-title">No Grid Nodes</div>
            <div class="empty-state-hint">
              Add a compute node to start forging models on remote GPUs.
            </div>
            <button class="action-btn primary" style="margin-top: 12px;" @click=${() => this._pairNode()}>
              + Pair Node
            </button>
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
              <div class="transport-dot" style="background: ${active ? GRID_STATUS_COLORS.online : GRID_STATUS_COLORS.offline};"></div>
              ${t.charAt(0).toUpperCase() + t.slice(1)}
            </div>
          `;
        })}
      </div>
    `;
  }

  // ── Node card ───────────────────────────────────────────────────────────

  private _renderNodeCard(node: GridNode): TemplateResult {
    const gpuPct = node.gpuUtilization ?? 0;
    const vramTotalGb = node.gpu ? (node.gpu.vramMb / 1024) : 0;
    const vramUsedGb = node.gpuMemoryUsedMb ? (node.gpuMemoryUsedMb / 1024) : 0;
    const vramPct = vramTotalGb > 0 ? Math.round((vramUsedGb / vramTotalGb) * 100) : 0;
    const tempC = node.gpuTemperatureC ?? 0;
    const isLocal = node.nodeId === 'local';
    const sshCmd = `ssh ${node.address}`;

    return html`
      <div class="node-card ${node.status}">
        <div class="node-card-header">
          <div class="node-name-row">
            <div class="status-dot" style="background: ${GRID_STATUS_COLORS[node.status]};"></div>
            <span class="node-name">${node.nodeName}</span>
            ${node.latencyMs > 0 ? html`<span class="node-latency">${node.latencyMs}ms</span>` : nothing}
          </div>
          <div class="node-badges">
            <span class="node-transport-badge">${node.transport}</span>
            ${isLocal ? html`<span class="node-local-badge">LOCAL</span>` : nothing}
          </div>
        </div>

        ${node.gpu ? html`
          <div class="node-gpu-name">${node.gpu.name}</div>
          <div class="node-gauges">
            <div class="gauge-row">
              <span class="gauge-label">GPU</span>
              <div class="gauge-track"><div class="gauge-fill gpu" style="width:${gpuPct}%"></div></div>
              <span class="gauge-value">${gpuPct}%</span>
            </div>
            <div class="gauge-row">
              <span class="gauge-label">VRAM</span>
              <div class="gauge-track"><div class="gauge-fill vram" style="width:${vramPct}%"></div></div>
              <span class="gauge-value">${vramUsedGb.toFixed(1)}/${vramTotalGb.toFixed(0)}G</span>
            </div>
            ${tempC > 0 ? html`
              <div class="gauge-row">
                <span class="gauge-label">TEMP</span>
                <div class="gauge-track"><div class="gauge-fill temp ${tempC > 80 ? 'hot' : tempC > 60 ? 'warm' : ''}" style="width:${Math.min(100, tempC)}%"></div></div>
                <span class="gauge-value">${tempC}C</span>
              </div>
            ` : nothing}
          </div>
        ` : html`
          <div class="node-no-gpu">No GPU detected</div>
        `}

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

        <div class="node-address">${node.address}</div>

        <div class="node-actions">
          <button class="ping-btn" ?disabled=${node.pinging}
            @click=${() => this._pingNode(node.nodeId)}>
            ${node.pinging ? 'Pinging...' : 'Ping'}
          </button>
          ${!isLocal ? html`
            <button class="ping-btn ssh-btn" @click=${() => navigator.clipboard.writeText(sshCmd)}
              title="Copy: ${sshCmd}">SSH</button>
            <button class="ping-btn remove-btn" @click=${() => this._removeNode(node.nodeId)}>Remove</button>
          ` : nothing}
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

  private async _pairNode(): Promise<void> {
    try {
      await this.executeCommand<any, any>('grid/pair', {});
      // Refresh after pairing
      await this._loadInitialState();
    } catch (err) {
      console.error('[GridOverview] Pair failed:', err);
    }
  }

  private _removeNode(nodeId: string): void {
    const updated = new Map(this._nodes);
    updated.delete(nodeId);
    this._nodes = updated;
    // TODO: persist removal via grid/trust revoke
  }

  private async _pingNode(nodeId: string): Promise<void> {
    const node = this._nodes.get(nodeId);
    if (!node) return;

    // Set pinging state
    const updated = new Map(this._nodes);
    updated.set(nodeId, { ...node, pinging: true });
    this._nodes = updated;

    try {
      // 5 second timeout — don't hang forever on unreachable nodes
      // eslint-disable-next-line @typescript-eslint/no-explicit-any -- grid/ping has dynamic result shape
      const pingPromise = this.executeCommand<any, any>('grid/ping', { nodeId });
      const timeoutPromise = new Promise<never>((_, reject) =>
        setTimeout(() => reject(new Error('Ping timeout (5s)')), 5000)
      );
      const result = await Promise.race([pingPromise, timeoutPromise]);
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
