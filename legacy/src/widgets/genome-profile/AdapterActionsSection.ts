/**
 * AdapterActionsSection — Sidebar widget for adapter management actions
 *
 * Quick-action panel for adapter management:
 *   - Prune duplicates (keep latest per domain)
 *   - Disk usage summary
 *   - Domain breakdown
 *
 * Reads the current genome-profile-widget's data via events,
 * or fetches its own if standalone.
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
import { GenomeLayers, type GenomeLayerInfo } from '../../commands/genome/layers/shared/GenomeLayersTypes';
import { GenomeAdapterPrune } from '../../commands/genome/adapter-prune/shared/GenomeAdapterPruneTypes';
import {
  AI_LEARNING_EVENTS,
} from '../../system/events/shared/AILearningEvents';

interface DomainSummary {
  domain: string;
  count: number;
  avgMaturity: number;
  totalSizeMB: number;
}

const STYLES = `
  :host {
    display: block;
    padding: 8px 10px;
    font-size: 11px;
    color: var(--content-primary, #e0e0e0);
    overflow-y: auto;
  }

  .no-data {
    text-align: center;
    color: var(--content-secondary, #777);
    font-style: italic;
    padding: 12px 0;
  }

  .actions-bar {
    display: flex;
    gap: 6px;
    margin-bottom: 8px;
  }

  .action-btn {
    flex: 1;
    background: rgba(0, 212, 255, 0.08);
    border: 1px solid rgba(0, 212, 255, 0.25);
    border-radius: 4px;
    color: rgba(0, 212, 255, 0.9);
    font-size: 10px;
    font-weight: 600;
    padding: 5px 8px;
    cursor: pointer;
    text-align: center;
    transition: all 0.15s;
  }

  .action-btn:hover {
    background: rgba(0, 212, 255, 0.15);
    border-color: rgba(0, 212, 255, 0.5);
  }

  .action-btn:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .action-btn.danger {
    border-color: rgba(255, 80, 80, 0.25);
    color: rgba(255, 80, 80, 0.9);
    background: rgba(255, 80, 80, 0.05);
  }

  .action-btn.danger:hover {
    background: rgba(255, 80, 80, 0.12);
    border-color: rgba(255, 80, 80, 0.5);
  }

  .disk-summary {
    display: flex;
    justify-content: space-between;
    padding: 4px 0;
    margin-bottom: 6px;
    border-bottom: 1px solid rgba(60, 80, 100, 0.2);
    font-size: 10px;
    color: var(--content-secondary, #999);
  }

  .disk-value {
    color: rgba(0, 255, 200, 0.8);
    font-weight: 600;
    font-family: monospace;
  }

  .domain-label {
    font-size: 10px;
    font-weight: 700;
    color: var(--content-secondary, #888);
    text-transform: uppercase;
    letter-spacing: 0.5px;
    margin: 8px 0 4px 0;
  }

  .domain-row {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 3px 0;
    border-bottom: 1px solid rgba(60, 80, 100, 0.1);
  }

  .domain-name {
    flex: 1;
    font-weight: 600;
    color: rgba(0, 212, 255, 0.8);
  }

  .domain-count {
    color: var(--content-secondary, #999);
    font-size: 10px;
  }

  .domain-maturity {
    width: 40px;
    text-align: right;
    font-family: monospace;
    font-size: 10px;
  }

  .domain-size {
    width: 50px;
    text-align: right;
    color: var(--content-secondary, #777);
    font-size: 10px;
  }

  .prune-result {
    padding: 6px 8px;
    border-radius: 4px;
    background: rgba(0, 255, 200, 0.08);
    border: 1px solid rgba(0, 255, 200, 0.2);
    font-size: 10px;
    color: rgba(0, 255, 200, 0.9);
    margin-bottom: 8px;
  }
`;

export class AdapterActionsSection extends ReactiveWidget {
  static override styles = [unsafeCSS(STYLES)] as CSSResultGroup;

  @reactive() private _layers: GenomeLayerInfo[] = [];
  @reactive() private _loading: boolean = false;
  @reactive() private _pruning: boolean = false;
  @reactive() private _pruneResult: string = '';
  @reactive() private _userId: string = '';

  private _cleanups: (() => void)[] = [];

  constructor() {
    super({ widgetName: 'AdapterActionsSection' });
  }

  protected override async onFirstRender(): Promise<void> {
    super.onFirstRender();

    // Refresh when training completes (new adapter may exist)
    this._cleanups.push(
      Events.subscribe(AI_LEARNING_EVENTS.TRAINING_COMPLETE, () => {
        if (this._userId) this._fetchLayers();
      }),
    );

    // Try to get userId from parent genome-profile-widget
    this._resolveUserId();
  }

  override disconnectedCallback(): void {
    super.disconnectedCallback();
    this._cleanups.forEach(fn => fn());
    this._cleanups = [];
  }

  /**
   * Resolve the userId from attributes or URL.
   */
  private _resolveUserId(): void {
    const attrId = this.getAttribute('entity-id') || this.getAttribute('persona-id');
    if (attrId) {
      this._userId = attrId;
      this._fetchLayers();
      return;
    }

    // Extract UUID from URL path (genome or genome-profile)
    const match = window.location.pathname.match(/\/genome(?:-profile)?\/([0-9a-f-]{36})/);
    if (match) {
      this._userId = match[1];
      this._fetchLayers();
    }
  }

  private async _fetchLayers(): Promise<void> {
    if (!this._userId) return;
    this._loading = true;
    try {
      const result = await GenomeLayers.execute({ personaId: this._userId });
      if (result.success) {
        this._layers = result.layers;
      }
    } catch (err) {
      console.error('AdapterActionsSection: Failed to load layers:', err);
    } finally {
      this._loading = false;
    }
  }

  private async _pruneAll(): Promise<void> {
    if (!this._userId) return;
    this._pruning = true;
    this._pruneResult = '';
    try {
      const result = await GenomeAdapterPrune.execute({
        personaId: this._userId,
        keepLatest: 1,
      });
      if (result.success) {
        this._pruneResult = `Pruned ${result.prunedCount ?? 0} adapter(s), freed ${(result.reclaimedMB ?? 0).toFixed(1)} MB`;
        this._fetchLayers();
      }
    } catch (err) {
      console.error('AdapterActionsSection: Prune failed:', err);
    } finally {
      this._pruning = false;
    }
  }

  private get _domains(): DomainSummary[] {
    const map = new Map<string, { count: number; totalMat: number; totalSize: number }>();
    for (const l of this._layers) {
      const d = map.get(l.domain) ?? { count: 0, totalMat: 0, totalSize: 0 };
      d.count++;
      d.totalMat += l.maturity;
      d.totalSize += l.sizeMB ?? 0;
      map.set(l.domain, d);
    }
    return [...map.entries()].map(([domain, d]) => ({
      domain,
      count: d.count,
      avgMaturity: d.count > 0 ? d.totalMat / d.count : 0,
      totalSizeMB: d.totalSize,
    })).sort((a, b) => b.count - a.count);
  }

  protected override renderContent(): TemplateResult {
    if (this._loading && this._layers.length === 0) {
      return html`<div class="no-data">Loading...</div>`;
    }

    if (!this._userId) {
      return html`<div class="no-data">No persona selected.</div>`;
    }

    if (this._layers.length === 0) {
      return html`<div class="no-data">No adapters found.</div>`;
    }

    const totalSizeMB = this._layers.reduce((s, l) => s + (l.sizeMB ?? 0), 0);
    const sizeDisplay = totalSizeMB >= 1024
      ? `${(totalSizeMB / 1024).toFixed(1)} GB`
      : `${Math.round(totalSizeMB)} MB`;
    const duplicateCount = this._layers.length - new Set(this._layers.map(l => l.domain)).size;

    return html`
      <div class="disk-summary">
        <span>${this._layers.length} adapters</span>
        <span class="disk-value">${sizeDisplay}</span>
      </div>

      ${this._pruneResult ? html`<div class="prune-result">${this._pruneResult}</div>` : nothing}

      <div class="actions-bar">
        <button class="action-btn" ?disabled=${this._pruning || duplicateCount === 0}
                @click=${() => this._pruneAll()}>
          ${this._pruning ? 'Pruning...' : `Prune (${duplicateCount} dups)`}
        </button>
      </div>

      ${this._domains.length > 0 ? html`
        <div class="domain-label">Domains</div>
        ${this._domains.map(d => html`
          <div class="domain-row">
            <span class="domain-name">${d.domain}</span>
            <span class="domain-count">${d.count}x</span>
            <span class="domain-maturity">${Math.round(d.avgMaturity * 100)}%</span>
            <span class="domain-size">${d.totalSizeMB.toFixed(0)} MB</span>
          </div>
        `)}
      ` : nothing}
    `;
  }
}

if (typeof customElements !== 'undefined' && !customElements.get('adapter-actions-section')) {
  customElements.define('adapter-actions-section', AdapterActionsSection);
}
