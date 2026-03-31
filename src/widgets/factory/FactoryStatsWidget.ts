/**
 * FactoryStatsWidget — Right sidebar for the factory view
 *
 * Shows:
 * - Download totals and trends (sparkline)
 * - Model comparison by improvement %
 * - Device target coverage
 * - Grid node status
 * - Forge-alloy attestation summary
 */

import {
  ReactiveWidget,
  html,
  css,
  reactive,
  type TemplateResult,
  type CSSResultGroup,
} from '../shared/ReactiveWidget';
import { nothing } from 'lit';

interface ModelStat {
  name: string;
  downloads: number;
  improvement?: number;
  domain: string;
  variant: string;
  hasAlloy: boolean;
}

interface DeviceCoverage {
  device: string;
  models: number;
  bestModel: string;
}

export class FactoryStatsWidget extends ReactiveWidget {

  @reactive() private _models: ModelStat[] = [];
  @reactive() private _totalDownloads = 0;
  @reactive() private _filter: 'all' | 'forged' | 'compacted' | 'gguf' = 'all';
  @reactive() private _sortBy: 'downloads' | 'improvement' | 'name' = 'downloads';

  override connectedCallback(): void {
    super.connectedCallback();
    this.loadStats();
  }

  private async loadStats(): Promise<void> {
    try {
      const result = await this.executeCommand<any, any>('model/list-published', { includeGguf: true });
      if (result?.models) {
        this._models = result.models.map((m: any) => ({
          name: m.name,
          downloads: m.downloads ?? 0,
          improvement: m.improvement,
          domain: m.domain ?? 'general',
          variant: m.variant ?? 'forged',
          hasAlloy: m.tags?.includes('forge-alloy') ?? false,
        }));
        this._totalDownloads = result.totalDownloads ?? 0;
      }
    } catch {
      // Failed to load
    }
  }

  private get filteredModels(): ModelStat[] {
    let models = this._models;
    if (this._filter !== 'all') {
      models = models.filter(m => m.variant === this._filter);
    }
    switch (this._sortBy) {
      case 'downloads': return [...models].sort((a, b) => b.downloads - a.downloads);
      case 'improvement': return [...models].sort((a, b) => (b.improvement ?? 0) - (a.improvement ?? 0));
      case 'name': return [...models].sort((a, b) => a.name.localeCompare(b.name));
    }
  }

  private get deviceCoverage(): DeviceCoverage[] {
    // Derive from model names/tags
    const devices: Record<string, { count: number; best: string }> = {
      'iPhone / Phone': { count: 0, best: '' },
      'MacBook Air 8GB': { count: 0, best: '' },
      'MacBook Pro 32GB': { count: 0, best: '' },
      'RTX 3090 24GB': { count: 0, best: '' },
      'RTX 5090 32GB': { count: 0, best: '' },
    };

    for (const m of this._models) {
      if (m.variant === 'gguf' || m.name.includes('GGUF')) {
        devices['iPhone / Phone'].count++;
        devices['MacBook Air 8GB'].count++;
        if (!devices['iPhone / Phone'].best) devices['iPhone / Phone'].best = m.name;
        if (!devices['MacBook Air 8GB'].best) devices['MacBook Air 8GB'].best = m.name;
      }
      if (m.variant === 'forged' || m.variant === 'compacted') {
        devices['RTX 5090 32GB'].count++;
        devices['RTX 3090 24GB'].count++;
        if (!devices['RTX 5090 32GB'].best) devices['RTX 5090 32GB'].best = m.name;
        if (!devices['RTX 3090 24GB'].best) devices['RTX 3090 24GB'].best = m.name;
      }
      if (m.name.includes('mlx')) {
        devices['MacBook Pro 32GB'].count++;
        if (!devices['MacBook Pro 32GB'].best) devices['MacBook Pro 32GB'].best = m.name;
      }
    }

    return Object.entries(devices).map(([device, data]) => ({
      device,
      models: data.count,
      bestModel: data.best,
    }));
  }

  static override styles: CSSResultGroup = [
    ReactiveWidget.styles,
    css`
      :host {
        display: block;
        width: 100%;
        height: 100%;
        overflow-y: auto;
        color: var(--content-primary, #e0e6ed);
        font-size: 12px;
      }

      .stats {
        padding: 16px;
        display: flex;
        flex-direction: column;
        gap: 20px;
      }

      .section-title {
        font-size: 11px;
        font-weight: 700;
        text-transform: uppercase;
        letter-spacing: 0.06em;
        color: var(--content-secondary, #8a92a5);
        margin-bottom: 8px;
      }

      /* ── Total Downloads ─────────────────────── */

      .total-card {
        background: linear-gradient(135deg, rgba(0, 212, 255, 0.08), rgba(0, 255, 200, 0.08));
        border: 1px solid rgba(0, 212, 255, 0.2);
        border-radius: 8px;
        padding: 16px;
        text-align: center;
      }

      .total-number {
        font-size: 32px;
        font-weight: 800;
        background: linear-gradient(135deg, #00d4ff, #00ffc8);
        -webkit-background-clip: text;
        -webkit-text-fill-color: transparent;
        background-clip: text;
        line-height: 1.2;
      }

      .total-label {
        font-size: 11px;
        color: var(--content-secondary, #8a92a5);
        text-transform: uppercase;
        letter-spacing: 0.06em;
      }

      .total-models {
        font-size: 12px;
        color: var(--content-secondary, #8a92a5);
        margin-top: 4px;
      }

      /* ── Filters ─────────────────────────────── */

      .filter-row {
        display: flex;
        gap: 4px;
        flex-wrap: wrap;
      }

      .filter-btn {
        padding: 3px 8px;
        font-size: 10px;
        font-weight: 600;
        border: 1px solid var(--border-color, rgba(255,255,255,0.12));
        border-radius: 3px;
        background: transparent;
        color: var(--content-secondary, #8a92a5);
        cursor: pointer;
        transition: all 0.15s;
      }

      .filter-btn:hover {
        border-color: var(--accent-primary, #00d4ff);
        color: var(--accent-primary, #00d4ff);
      }

      .filter-btn.active {
        background: rgba(0, 212, 255, 0.15);
        border-color: var(--accent-primary, #00d4ff);
        color: var(--accent-primary, #00d4ff);
      }

      /* ── Leaderboard ─────────────────────────── */

      .leaderboard {
        display: flex;
        flex-direction: column;
        gap: 4px;
      }

      .lb-entry {
        display: flex;
        align-items: center;
        gap: 8px;
        padding: 6px 8px;
        border-radius: 4px;
        transition: background 0.15s;
      }

      .lb-entry:hover {
        background: rgba(255,255,255,0.03);
      }

      .lb-rank {
        font-size: 10px;
        font-weight: 700;
        color: var(--content-secondary, #8a92a5);
        min-width: 18px;
      }

      .lb-rank.gold { color: #ffd700; }
      .lb-rank.silver { color: #c0c0c0; }
      .lb-rank.bronze { color: #cd7f32; }

      .lb-bar-container {
        flex: 1;
        min-width: 0;
      }

      .lb-name {
        font-size: 11px;
        font-weight: 500;
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
      }

      .lb-bar {
        height: 3px;
        background: rgba(255,255,255,0.06);
        border-radius: 2px;
        margin-top: 3px;
        overflow: hidden;
      }

      .lb-bar-fill {
        height: 100%;
        background: linear-gradient(90deg, #00d4ff, #00ffc8);
        border-radius: 2px;
        transition: width 0.3s ease;
      }

      .lb-value {
        font-size: 11px;
        font-weight: 700;
        font-variant-numeric: tabular-nums;
        min-width: 36px;
        text-align: right;
      }

      /* ── Device Coverage ─────────────────────── */

      .device-list {
        display: flex;
        flex-direction: column;
        gap: 6px;
      }

      .device-row {
        display: flex;
        justify-content: space-between;
        align-items: center;
        padding: 6px 8px;
        background: var(--surface-elevated, rgba(255,255,255,0.03));
        border-radius: 4px;
      }

      .device-name {
        font-size: 11px;
        font-weight: 500;
      }

      .device-count {
        font-size: 11px;
        font-weight: 700;
        color: var(--accent-primary, #00d4ff);
      }

      .device-count.zero {
        color: var(--content-tertiary, #5a6070);
      }

      /* ── Alloy Status ────────────────────────── */

      .alloy-summary {
        background: var(--surface-elevated, rgba(255,255,255,0.03));
        border: 1px solid var(--border-color, rgba(255,255,255,0.08));
        border-radius: 6px;
        padding: 12px;
      }

      .alloy-stat-row {
        display: flex;
        justify-content: space-between;
        padding: 3px 0;
        font-size: 11px;
      }

      .alloy-stat-label {
        color: var(--content-secondary, #8a92a5);
      }

      .alloy-stat-value {
        font-weight: 700;
      }

      .alloy-link {
        display: block;
        margin-top: 8px;
        font-size: 10px;
        color: var(--accent-primary, #00d4ff);
        text-decoration: none;
      }

      .alloy-link:hover {
        text-decoration: underline;
      }
    `,
  ];

  protected override render(): TemplateResult {
    return html`
      <div class="stats">
        ${this.renderTotalDownloads()}
        ${this.renderFilters()}
        ${this.renderLeaderboard()}
        ${this.renderDeviceCoverage()}
        ${this.renderAlloyStatus()}
      </div>
    `;
  }

  private renderTotalDownloads(): TemplateResult {
    return html`
      <div class="total-card">
        <div class="total-number">${this._totalDownloads.toLocaleString()}</div>
        <div class="total-label">Total Downloads</div>
        <div class="total-models">${this._models.length} models published</div>
      </div>
    `;
  }

  private renderFilters(): TemplateResult {
    type FilterKey = 'all' | 'forged' | 'compacted' | 'gguf';
    type SortKey = 'downloads' | 'improvement' | 'name';

    const filters: Array<{ key: FilterKey; label: string }> = [
      { key: 'all', label: 'All' },
      { key: 'forged', label: 'Forged' },
      { key: 'compacted', label: 'Compacted' },
      { key: 'gguf', label: 'GGUF' },
    ];

    const sorts: Array<{ key: SortKey; label: string }> = [
      { key: 'downloads', label: 'Downloads' },
      { key: 'improvement', label: 'Improvement' },
      { key: 'name', label: 'Name' },
    ];

    return html`
      <div>
        <div class="section-title">Filter</div>
        <div class="filter-row" style="margin-bottom:6px">
          ${filters.map(f => html`
            <button class="filter-btn ${this._filter === f.key ? 'active' : ''}"
              @click=${() => this._filter = f.key}>${f.label}</button>
          `)}
        </div>
        <div class="filter-row">
          ${sorts.map(s => html`
            <button class="filter-btn ${this._sortBy === s.key ? 'active' : ''}"
              @click=${() => this._sortBy = s.key}>${s.label}</button>
          `)}
        </div>
      </div>
    `;
  }

  private renderLeaderboard(): TemplateResult {
    const models = this.filteredModels.slice(0, 10);
    if (models.length === 0) return html``;

    const maxVal = this._sortBy === 'downloads'
      ? Math.max(...models.map(m => m.downloads))
      : Math.max(...models.map(m => m.improvement ?? 0));

    return html`
      <div>
        <div class="section-title">Leaderboard</div>
        <div class="leaderboard">
          ${models.map((m, i) => {
            const val = this._sortBy === 'downloads' ? m.downloads : (m.improvement ?? 0);
            const pct = maxVal > 0 ? (val / maxVal) * 100 : 0;
            const rankClass = i === 0 ? 'gold' : i === 1 ? 'silver' : i === 2 ? 'bronze' : '';
            const displayVal = this._sortBy === 'downloads'
              ? this.formatCount(m.downloads)
              : m.improvement != null ? `+${m.improvement.toFixed(1)}%` : '--';

            return html`
              <div class="lb-entry">
                <span class="lb-rank ${rankClass}">${i + 1}</span>
                <div class="lb-bar-container">
                  <div class="lb-name">${m.name}</div>
                  <div class="lb-bar">
                    <div class="lb-bar-fill" style="width:${pct}%"></div>
                  </div>
                </div>
                <span class="lb-value">${displayVal}</span>
              </div>
            `;
          })}
        </div>
      </div>
    `;
  }

  private renderDeviceCoverage(): TemplateResult {
    const devices = this.deviceCoverage;

    return html`
      <div>
        <div class="section-title">Device Coverage</div>
        <div class="device-list">
          ${devices.map(d => html`
            <div class="device-row">
              <span class="device-name">${d.device}</span>
              <span class="device-count ${d.models === 0 ? 'zero' : ''}">${d.models} models</span>
            </div>
          `)}
        </div>
      </div>
    `;
  }

  private renderAlloyStatus(): TemplateResult {
    const alloyCount = this._models.filter(m => m.hasAlloy).length;
    const totalCount = this._models.length;

    return html`
      <div>
        <div class="section-title">ForgeAlloy</div>
        <div class="alloy-summary">
          <div class="alloy-stat-row">
            <span class="alloy-stat-label">Models with alloy</span>
            <span class="alloy-stat-value">${alloyCount} / ${totalCount}</span>
          </div>
          <div class="alloy-stat-row">
            <span class="alloy-stat-label">Trust level</span>
            <span class="alloy-stat-value" style="color:#ffaa00">self-attested</span>
          </div>
          <div class="alloy-stat-row">
            <span class="alloy-stat-label">Signing</span>
            <span class="alloy-stat-value" style="color:var(--content-secondary,#8a92a5)">Phase 1 (unsigned)</span>
          </div>
          <a class="alloy-link" href="https://github.com/CambrianTech/forge-alloy" target="_blank">
            ForgeAlloy Spec →
          </a>
        </div>
      </div>
    `;
  }

  private formatCount(n: number): string {
    if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
    if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
    return n.toString();
  }
}

if (!customElements.get('factory-stats-widget')) {
  customElements.define('factory-stats-widget', FactoryStatsWidget);
}
