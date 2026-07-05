/**
 * GridStatusSection — Compact sidebar widget for at-a-glance grid health
 *
 * Shows:
 *   - "3/4 nodes online" with status dot
 *   - Transport indicators (Tailscale/Reticulum)
 *   - Per-node mini status (name + latency)
 *   - Click to open full grid-overview tab
 *
 * Self-contained: subscribes to GRID_EVENTS, no parent coordination needed.
 * Follows TrainingStatusSection / ContinuumMetricsWidget pattern.
 */

import {
  ReactiveWidget,
  html,
  reactive,
  unsafeCSS,
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
  type GridNodeStatus,
  type GridTransport,
} from '../../system/events/shared/GridEvents';
import { ContentService } from '../../system/state/ContentService';
import { normalizeGridNodes, GRID_STATUS_COLORS } from './GridDataNormalizer';
import { styles as GRID_STATUS_STYLES } from './public/grid-status-section.styles';

interface NodeSnapshot {
  nodeId: string;
  nodeName: string;
  status: GridNodeStatus;
  latencyMs: number;
  transport: GridTransport;
  gpu?: { name: string; vramMb: number };
}

// Use shared GRID_STATUS_COLORS from GridDataNormalizer

export class GridStatusSection extends ReactiveWidget {
  static override styles = [unsafeCSS(GRID_STATUS_STYLES)] as CSSResultGroup;

  @reactive() private _nodes: Map<string, NodeSnapshot> = new Map();

  private _cleanups: (() => void)[] = [];

  constructor() {
    super({ widgetName: 'GridStatusSection' });
  }

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
          gpu: data.gpu,
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
        if (existing) {
          const updated = new Map(this._nodes);
          updated.set(data.nodeId, {
            ...existing,
            status: data.status,
            latencyMs: data.latencyMs,
          });
          this._nodes = updated;
        }
      }),
    );

    // Initial load via command (if grid is already populated)
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
        const updated = new Map<string, NodeSnapshot>();
        for (const n of normalized) {
          updated.set(n.nodeId, {
            nodeId: n.nodeId,
            nodeName: n.nodeName,
            status: n.status,
            latencyMs: n.latencyMs,
            transport: n.transport,
            gpu: n.gpu,
          });
        }
        this._nodes = updated;
      }
    } catch (err) {
      console.warn('[GridStatusSection] Failed to load grid nodes:', err);
    }
  }

  protected override renderContent(): TemplateResult {
    const nodes = [...this._nodes.values()];

    if (nodes.length === 0) {
      return html`<div class="empty-state">No grid nodes connected</div>`;
    }

    const onlineCount = nodes.filter(n => n.status === 'online').length;
    const totalCount = nodes.length;
    const allOnline = onlineCount === totalCount;

    // Determine transports in use
    const transports = new Set(nodes.map(n => n.transport));

    // Overall status dot color
    const overallStatus: GridNodeStatus = allOnline ? 'online' : onlineCount > 0 ? 'degraded' : 'offline';

    return html`
      <div class="header" @click=${() => this._openGridOverview()}>
        <div class="summary">
          <div class="status-dot" style="background: ${GRID_STATUS_COLORS[overallStatus]};"></div>
          <span class="summary-text">${onlineCount}/${totalCount} nodes online</span>
        </div>
        <span class="open-arrow">→</span>
      </div>

      <div class="transports">
        ${this._renderTransportBadge('tailscale', transports.has('tailscale'))}
        ${this._renderTransportBadge('reticulum', transports.has('reticulum'))}
      </div>

      <div class="node-list">
        ${nodes
          .sort((a, b) => a.nodeName.localeCompare(b.nodeName))
          .map(n => this._renderNode(n))}
      </div>
    `;
  }

  private _renderTransportBadge(transport: string, active: boolean): TemplateResult {
    const cls = active ? 'transport-active' : 'transport-inactive';
    return html`<span class="transport-badge ${cls}">${transport}</span>`;
  }

  private _renderNode(node: NodeSnapshot): TemplateResult {
    return html`
      <div class="node-row">
        <div class="status-dot" style="background: ${GRID_STATUS_COLORS[node.status]}; width: 5px; height: 5px;"></div>
        <span class="node-name">${node.nodeName}</span>
        ${node.gpu ? html`<span class="node-gpu">${node.gpu.name}</span>` : nothing}
        <span class="node-latency">${node.latencyMs}ms</span>
      </div>
    `;
  }

  private _openGridOverview(): void {
    ContentService.open('grid-overview', undefined, { title: 'Grid' });
  }
}

if (typeof customElements !== 'undefined' && !customElements.get('grid-status-section')) {
  customElements.define('grid-status-section', GridStatusSection);
}
