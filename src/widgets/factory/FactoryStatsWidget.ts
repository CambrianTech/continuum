/**
 * FactoryStatsWidget — Right sidebar for the factory view
 *
 * Rich model tiles (like PersonaTile quality), download gauges,
 * improvement meters, device badges, alloy trust indicators.
 * Filters and sorting. Published models + forge-alloy status.
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
import { Events } from '../../system/core/shared/Events';

interface ModelStat {
  id: string;
  name: string;
  downloads: number;
  likes: number;
  improvement?: number;
  domain: string;
  variant: string;
  baseModel?: string;
  sizeGb?: number;
  hasAlloy: boolean;
}

interface ForgeDraft {
  id: string;
  name: string;
  alloy: Record<string, unknown>;
  status: 'editing' | 'queued' | 'running' | 'completed' | 'failed';
  lastEditedAt: string;
  nodeId?: string;
  jobId?: string;
}

const DRAFTS_COLLECTION = 'forge_drafts';

export class FactoryStatsWidget extends ReactiveWidget {

  @reactive() private _models: ModelStat[] = [];
  @reactive() private _drafts: ForgeDraft[] = [];
  @reactive() private _totalDownloads = 0;
  @reactive() private _filter: 'all' | 'forged' | 'compacted' | 'gguf' = 'all';
  @reactive() private _sortBy: 'downloads' | 'improvement' | 'name' = 'downloads';
  @reactive() private _selectedModel: string | null = null;
  @reactive() private _activeTab: 'published' | 'drafts' = 'published';

  protected override get cacheKey(): string { return 'factory-stats'; }
  protected override get cacheableProperties(): string[] {
    return ['_models', '_drafts', '_totalDownloads', '_filter', '_sortBy', '_activeTab'];
  }

  override connectedCallback(): void {
    super.connectedCallback();
    this.loadStats();
    this.loadDrafts();
    this.listenForDraftUpdates();
  }

  private async loadStats(): Promise<void> {
    try {
      const result = await this.executeCommand<any, any>('model/list-published', { includeGguf: true });
      if (result?.models) {
        this._models = result.models.map((m: any) => ({
          id: m.id,
          name: m.name,
          downloads: m.downloads ?? 0,
          likes: m.likes ?? 0,
          improvement: m.improvement,
          domain: m.domain ?? 'general',
          variant: m.variant ?? 'forged',
          baseModel: m.baseModel,
          sizeGb: m.sizeGb,
          hasAlloy: m.tags?.includes('forge-alloy') ?? false,
        }));
        this._totalDownloads = result.totalDownloads ?? 0;
      }
    } catch { /* */ }
  }

  private async loadDrafts(): Promise<void> {
    try {
      const result = await this.executeCommand<any, any>('data/list', {
        collection: DRAFTS_COLLECTION,
        orderBy: [{ field: 'updatedAt', direction: 'desc' }],
        limit: 20,
      });
      if (result?.items) {
        this._drafts = result.items.map((d: any) => ({
          id: d.id,
          name: d.name ?? d.alloy?.name ?? 'Untitled',
          alloy: d.alloy ?? {},
          status: d.status ?? 'editing',
          lastEditedAt: d.updatedAt ?? d.createdAt ?? '',
          nodeId: d.nodeId,
          jobId: d.jobId,
        }));
      }
    } catch { /* No drafts yet — that's fine */ }
  }

  private listenForDraftUpdates(): void {
    Events.subscribe(`data:${DRAFTS_COLLECTION}:created`, () => this.loadDrafts());
    Events.subscribe(`data:${DRAFTS_COLLECTION}:updated`, () => this.loadDrafts());
    Events.subscribe(`data:${DRAFTS_COLLECTION}:deleted`, () => this.loadDrafts());
  }

  /** Save current console state as a draft — called from forge console via event */
  private async saveDraft(alloy: Record<string, unknown>): Promise<void> {
    const name = (alloy.name as string) ?? 'Untitled';
    await this.executeCommand<any, any>('data/create', {
      collection: DRAFTS_COLLECTION,
      data: { name, alloy, status: 'editing' },
    });
  }

  private async deleteDraft(id: string, e: Event): Promise<void> {
    e.stopPropagation();
    await this.executeCommand<any, any>('data/delete', {
      collection: DRAFTS_COLLECTION,
      id,
    });
  }

  private loadDraftIntoConsole(draft: ForgeDraft): void {
    Events.emit('factory:alloy:load', { alloy: draft.alloy, draftId: draft.id });
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
        padding: 12px;
        display: flex;
        flex-direction: column;
        gap: 16px;
      }

      .section-label {
        font-size: 9px;
        font-weight: 700;
        text-transform: uppercase;
        letter-spacing: 0.08em;
        color: var(--content-secondary, #8a92a5);
        margin-bottom: 6px;
      }

      /* ── Hero Card ──────────────────────────── */

      .hero {
        background: rgba(10, 25, 35, 0.9);
        border: 1px solid rgba(0, 255, 200, 0.2);
        border-radius: 8px;
        padding: 14px;
        text-align: center;
        box-shadow: 0 0 12px rgba(0, 255, 200, 0.05);
      }

      .hero-number {
        font-size: 28px;
        font-weight: 800;
        font-variant-numeric: tabular-nums;
        background: linear-gradient(135deg, #00d4ff, #00ffc8);
        -webkit-background-clip: text;
        -webkit-text-fill-color: transparent;
        background-clip: text;
        line-height: 1.2;
      }

      .hero-label {
        font-size: 9px;
        font-weight: 600;
        text-transform: uppercase;
        letter-spacing: 0.08em;
        color: var(--content-secondary, #8a92a5);
      }

      .hero-sub {
        font-size: 11px;
        color: var(--content-tertiary, #5a6070);
        margin-top: 2px;
      }

      /* ── Panel Tabs (Published / Drafts) ───── */

      .panel-tabs {
        display: flex;
        gap: 2px;
        background: rgba(0,0,0,0.2);
        border-radius: 6px;
        padding: 2px;
      }

      .panel-tab {
        flex: 1;
        padding: 6px;
        font-size: 10px;
        font-weight: 700;
        text-transform: uppercase;
        letter-spacing: 0.06em;
        border: none;
        border-radius: 4px;
        background: transparent;
        color: var(--content-tertiary, #5a6070);
        cursor: pointer;
        transition: all 0.15s;
        text-align: center;
      }

      .panel-tab:hover {
        color: var(--content-secondary, #8a92a5);
      }

      .panel-tab.active {
        background: rgba(0, 212, 255, 0.1);
        color: var(--accent-primary, #00d4ff);
      }

      .draft-count {
        font-size: 9px;
        padding: 1px 5px;
        border-radius: 8px;
        background: rgba(0, 212, 255, 0.2);
        color: var(--accent-primary, #00d4ff);
        margin-left: 2px;
      }

      .empty-drafts {
        text-align: center;
        padding: 24px 12px;
        color: var(--content-tertiary, #5a6070);
        font-size: 11px;
      }

      .empty-icon {
        font-size: 24px;
        margin-bottom: 8px;
        opacity: 0.3;
      }

      .empty-hint {
        font-size: 9px;
        margin-top: 6px;
        font-style: italic;
      }

      .draft-delete {
        font-size: 10px;
        width: 18px;
        height: 18px;
        border: 1px solid rgba(255,255,255,0.08);
        border-radius: 3px;
        background: transparent;
        color: var(--content-tertiary, #5a6070);
        cursor: pointer;
        display: flex;
        align-items: center;
        justify-content: center;
        transition: all 0.15s;
      }

      .draft-delete:hover {
        border-color: #ff4444;
        color: #ff4444;
      }

      /* ── Filter Pills ───────────────────────── */

      .filter-row {
        display: flex;
        gap: 3px;
        flex-wrap: wrap;
      }

      .pill {
        padding: 2px 7px;
        font-size: 9px;
        font-weight: 700;
        letter-spacing: 0.04em;
        border: 1px solid rgba(255,255,255,0.08);
        border-radius: 10px;
        background: transparent;
        color: var(--content-tertiary, #5a6070);
        cursor: pointer;
        transition: all 0.15s;
      }

      .pill:hover {
        border-color: rgba(0, 212, 255, 0.3);
        color: var(--content-secondary, #8a92a5);
      }

      .pill.active {
        background: rgba(0, 212, 255, 0.12);
        border-color: rgba(0, 212, 255, 0.4);
        color: var(--accent-primary, #00d4ff);
      }

      /* ── Model Tile (persona-tile quality) ──── */

      .model-tiles {
        display: flex;
        flex-direction: column;
        gap: 4px;
      }

      .model-tile {
        display: flex;
        align-items: center;
        gap: 10px;
        padding: 8px 10px;
        background: rgba(10, 25, 35, 0.6);
        border: 1px solid rgba(60, 80, 100, 0.3);
        border-radius: 6px;
        cursor: pointer;
        transition: all 0.15s;
      }

      .model-tile:hover {
        border-color: rgba(0, 212, 255, 0.4);
        background: rgba(10, 25, 35, 0.8);
      }

      .model-tile.selected {
        border-color: rgba(0, 255, 200, 0.5);
        box-shadow: 0 0 8px rgba(0, 255, 200, 0.1);
      }

      .model-rank-badge {
        width: 22px;
        height: 22px;
        border-radius: 50%;
        display: flex;
        align-items: center;
        justify-content: center;
        font-size: 9px;
        font-weight: 800;
        flex-shrink: 0;
        background: rgba(60, 80, 100, 0.4);
        color: var(--content-secondary, #8a92a5);
        border: 1px solid rgba(80, 100, 120, 0.4);
      }

      .model-rank-badge.gold {
        background: rgba(255, 215, 0, 0.15);
        border-color: rgba(255, 215, 0, 0.4);
        color: #ffd700;
      }

      .model-rank-badge.silver {
        background: rgba(192, 192, 192, 0.15);
        border-color: rgba(192, 192, 192, 0.4);
        color: #c0c0c0;
      }

      .model-rank-badge.bronze {
        background: rgba(205, 127, 50, 0.15);
        border-color: rgba(205, 127, 50, 0.4);
        color: #cd7f32;
      }

      .model-tile-info {
        flex: 1;
        min-width: 0;
        display: flex;
        flex-direction: column;
        gap: 3px;
      }

      .model-tile-name {
        font-size: 11px;
        font-weight: 600;
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
      }

      .model-tile-meta {
        display: flex;
        gap: 4px;
        align-items: center;
      }

      .tag {
        font-size: 7px;
        font-weight: 700;
        text-transform: uppercase;
        letter-spacing: 0.5px;
        padding: 1px 4px;
        border-radius: 2px;
        font-family: monospace;
      }

      .tag.domain {
        color: rgba(0, 212, 255, 0.8);
        text-shadow: 0 0 4px rgba(0, 212, 255, 0.3);
      }

      .tag.variant {
        color: rgba(0, 255, 200, 0.7);
        text-shadow: 0 0 4px rgba(0, 255, 200, 0.2);
      }

      .tag.alloy {
        color: rgba(255, 170, 0, 0.8);
        text-shadow: 0 0 4px rgba(255, 170, 0, 0.3);
        border: 1px solid rgba(255, 170, 0, 0.3);
        border-radius: 3px;
      }

      /* ── Gauge (download/improvement meter) ── */

      .model-gauge {
        display: flex;
        flex-direction: column;
        align-items: flex-end;
        gap: 2px;
        flex-shrink: 0;
      }

      .gauge-value {
        font-size: 12px;
        font-weight: 700;
        font-variant-numeric: tabular-nums;
        color: var(--content-primary, #e0e6ed);
      }

      .gauge-bar {
        width: 40px;
        height: 3px;
        background: rgba(20, 30, 45, 0.6);
        border: 1px solid rgba(60, 80, 100, 0.3);
        border-radius: 2px;
        overflow: hidden;
      }

      .gauge-fill {
        height: 100%;
        border-radius: 1px;
        background: linear-gradient(90deg, #00d4ff, #00ffc8);
        transition: width 0.4s ease;
        box-shadow: 0 0 4px rgba(0, 255, 200, 0.3);
      }

      .gauge-label {
        font-size: 7px;
        color: var(--content-tertiary, #5a6070);
        text-transform: uppercase;
        letter-spacing: 0.04em;
      }

      /* ── Model Actions (expanded) ──────────── */

      .model-actions {
        display: flex;
        gap: 4px;
        padding: 6px 10px 6px 42px;
        flex-wrap: wrap;
        align-items: center;
      }

      .action-btn {
        font-size: 9px;
        font-weight: 600;
        padding: 4px 10px;
        border-radius: 4px;
        border: 1px solid rgba(255,255,255,0.12);
        background: rgba(255,255,255,0.04);
        color: var(--content-secondary, #8a92a5);
        cursor: pointer;
        transition: all 0.15s;
      }

      .action-btn:hover {
        border-color: var(--accent-primary, #00d4ff);
        color: var(--accent-primary, #00d4ff);
        background: rgba(0, 212, 255, 0.08);
      }

      .action-btn.primary {
        background: rgba(0, 255, 200, 0.1);
        border-color: rgba(0, 255, 200, 0.4);
        color: #00ffc8;
      }

      .action-btn.primary:hover {
        background: rgba(0, 255, 200, 0.2);
        box-shadow: 0 0 8px rgba(0, 255, 200, 0.2);
      }

      .action-info {
        font-size: 9px;
        color: var(--content-tertiary, #5a6070);
        margin-left: auto;
      }

      .action-info.improve {
        color: #00ffc8;
        font-weight: 700;
      }

      /* ── Alloy Panel ────────────────────────── */

      .alloy-panel {
        background: rgba(10, 25, 35, 0.9);
        border: 1px solid rgba(0, 255, 200, 0.15);
        border-radius: 6px;
        padding: 10px;
        box-shadow: 0 0 8px rgba(0, 255, 200, 0.05);
      }

      .alloy-row {
        display: flex;
        justify-content: space-between;
        padding: 2px 0;
        font-size: 10px;
      }

      .alloy-key {
        color: var(--content-tertiary, #5a6070);
        font-family: monospace;
        font-size: 8px;
        text-transform: uppercase;
        letter-spacing: 0.04em;
      }

      .alloy-val {
        font-weight: 700;
        font-family: monospace;
      }

      .alloy-val.trust {
        color: #ffaa00;
        text-shadow: 0 0 4px rgba(255, 170, 0, 0.3);
      }

      .alloy-val.phase {
        color: var(--content-secondary, #8a92a5);
      }

      .alloy-link {
        display: block;
        margin-top: 6px;
        font-size: 9px;
        color: rgba(0, 255, 200, 0.7);
        text-decoration: none;
        font-family: monospace;
        transition: color 0.15s;
      }

      .alloy-link:hover {
        color: rgba(0, 255, 200, 1);
        text-shadow: 0 0 6px rgba(0, 255, 200, 0.4);
      }
    `,
  ];

  protected override render(): TemplateResult {
    return html`
      <div class="stats">
        ${this.renderHero()}
        ${this.renderPanelTabs()}
        ${this._activeTab === 'published' ? html`
          ${this.renderFilters()}
          ${this.renderModelTiles()}
          ${this.renderAlloyPanel()}
        ` : html`
          ${this.renderDraftsList()}
        `}
      </div>
    `;
  }

  private renderPanelTabs(): TemplateResult {
    const draftCount = this._drafts.filter(d => d.status !== 'completed').length;
    return html`
      <div class="panel-tabs">
        <button class="panel-tab ${this._activeTab === 'published' ? 'active' : ''}"
          @click=${() => this._activeTab = 'published'}>Published</button>
        <button class="panel-tab ${this._activeTab === 'drafts' ? 'active' : ''}"
          @click=${() => this._activeTab = 'drafts'}>Drafts${draftCount > 0 ? html` <span class="draft-count">${draftCount}</span>` : nothing}</button>
      </div>
    `;
  }

  private renderDraftsList(): TemplateResult {
    if (this._drafts.length === 0) {
      return html`
        <div class="empty-drafts">
          <div class="empty-icon">&#9881;</div>
          <div>No drafts yet</div>
          <div class="empty-hint">Click "&#171; Forge This" on a published model or configure a forge in the console</div>
        </div>
      `;
    }

    return html`
      <div class="model-tiles">
        ${this._drafts.map(d => this.renderDraftTile(d))}
      </div>
    `;
  }

  private renderDraftTile(d: ForgeDraft): TemplateResult {
    const statusColors: Record<string, string> = {
      editing: '#00d4ff', queued: '#ffaa00', running: '#00ffc8', completed: '#00ffc8', failed: '#ff4444',
    };
    const ago = this.timeAgo(d.lastEditedAt);
    const baseModel = (d.alloy?.source as Record<string, unknown>)?.baseModel as string ?? '';

    return html`
      <div class="model-tile" @click=${() => this.loadDraftIntoConsole(d)}>
        <div class="model-rank-badge" style="background:${statusColors[d.status]}22;border-color:${statusColors[d.status]}66;color:${statusColors[d.status]}">
          ${d.status === 'editing' ? '&#9998;' : d.status === 'queued' ? '&#9203;' : d.status === 'running' ? '&#9654;' : d.status === 'completed' ? '&#10003;' : '&#10007;'}
        </div>
        <div class="model-tile-info">
          <div class="model-tile-name">${d.name}</div>
          <div class="model-tile-meta">
            <span class="tag domain">${d.status}</span>
            ${baseModel ? html`<span class="tag variant">${baseModel.split('/').pop()}</span>` : nothing}
          </div>
        </div>
        <div class="model-gauge">
          <div class="gauge-value" style="font-size:9px;color:var(--content-tertiary)">${ago}</div>
          <button class="draft-delete" @click=${(e: Event) => this.deleteDraft(d.id, e)} title="Delete draft">&#10005;</button>
        </div>
      </div>
    `;
  }

  private timeAgo(iso: string): string {
    if (!iso) return '';
    const ms = Date.now() - new Date(iso).getTime();
    if (ms < 60000) return 'just now';
    if (ms < 3600000) return `${Math.floor(ms / 60000)}m ago`;
    if (ms < 86400000) return `${Math.floor(ms / 3600000)}h ago`;
    return `${Math.floor(ms / 86400000)}d ago`;
  }

  private renderHero(): TemplateResult {
    return html`
      <div class="hero">
        <div class="hero-number">${this._totalDownloads.toLocaleString()}</div>
        <div class="hero-label">Total Downloads</div>
        <div class="hero-sub">${this._models.length} models published on HuggingFace</div>
      </div>
    `;
  }

  private renderFilters(): TemplateResult {
    type FilterKey = 'all' | 'forged' | 'compacted' | 'gguf';
    type SortKey = 'downloads' | 'improvement' | 'name';

    const filters: Array<{ key: FilterKey; label: string }> = [
      { key: 'all', label: 'ALL' },
      { key: 'forged', label: 'FORGED' },
      { key: 'compacted', label: 'COMPACTED' },
      { key: 'gguf', label: 'GGUF' },
    ];

    const sorts: Array<{ key: SortKey; label: string }> = [
      { key: 'downloads', label: 'DOWNLOADS' },
      { key: 'improvement', label: 'IMPROVEMENT' },
      { key: 'name', label: 'NAME' },
    ];

    return html`
      <div>
        <div class="filter-row">
          ${filters.map(f => html`
            <button class="pill ${this._filter === f.key ? 'active' : ''}"
              @click=${() => this._filter = f.key}>${f.label}</button>
          `)}
          <span style="width:4px"></span>
          ${sorts.map(s => html`
            <button class="pill ${this._sortBy === s.key ? 'active' : ''}"
              @click=${() => this._sortBy = s.key}>${s.label}</button>
          `)}
        </div>
      </div>
    `;
  }

  private renderModelTiles(): TemplateResult {
    const models = this.filteredModels;
    if (models.length === 0) return html``;

    const maxDownloads = Math.max(...models.map(m => m.downloads), 1);

    return html`
      <div>
        <div class="section-label">Published Models</div>
        <div class="model-tiles">
          ${models.map((m, i) => this.renderModelTile(m, i, maxDownloads))}
        </div>
      </div>
    `;
  }

  private selectModel(m: ModelStat): void {
    this._selectedModel = this._selectedModel === m.id ? null : m.id;
  }

  private useAsBase(m: ModelStat, e: Event): void {
    e.stopPropagation();
    const modelId = m.id.includes('/') ? m.id : `continuum-ai/${m.name}`;
    Events.emit('factory:model:select', { modelId, name: m.name, domain: m.domain, sizeGb: m.sizeGb, hasAlloy: m.hasAlloy });
  }

  private viewOnHF(m: ModelStat, e: Event): void {
    e.stopPropagation();
    const url = m.id.includes('/') ? `https://huggingface.co/${m.id}` : `https://huggingface.co/continuum-ai/${m.name}`;
    window.open(url, '_blank');
  }

  private renderModelTile(m: ModelStat, rank: number, maxDownloads: number): TemplateResult {
    const pct = (m.downloads / maxDownloads) * 100;
    const rankClass = rank === 0 ? 'gold' : rank === 1 ? 'silver' : rank === 2 ? 'bronze' : '';
    const selected = this._selectedModel === m.id;

    return html`
      <div class="model-tile ${selected ? 'selected' : ''}"
        @click=${() => this.selectModel(m)}>
        <div class="model-rank-badge ${rankClass}">${rank + 1}</div>
        <div class="model-tile-info">
          <div class="model-tile-name">${m.name}</div>
          <div class="model-tile-meta">
            <span class="tag domain">${m.domain}</span>
            <span class="tag variant">${m.variant}</span>
            ${m.hasAlloy ? html`<span class="tag alloy">ALLOY</span>` : nothing}
          </div>
        </div>
        <div class="model-gauge">
          <div class="gauge-value">${this.formatCount(m.downloads)}</div>
          <div class="gauge-bar">
            <div class="gauge-fill" style="width:${pct}%"></div>
          </div>
          <div class="gauge-label">downloads</div>
        </div>
      </div>
      ${selected ? this.renderModelActions(m) : nothing}
    `;
  }

  private renderModelActions(m: ModelStat): TemplateResult {
    return html`
      <div class="model-actions">
        <button class="action-btn primary" @click=${(e: Event) => this.useAsBase(m, e)} title="Load into forge console">&#171; Forge This</button>
        ${m.hasAlloy ? html`<button class="action-btn" @click=${(e: Event) => { e.stopPropagation(); }} title="Load alloy recipe into pipeline composer">&#171; Remix</button>` : nothing}
        <button class="action-btn" @click=${(e: Event) => this.viewOnHF(m, e)} title="Open on HuggingFace">HF &#8599;</button>
        ${m.sizeGb ? html`<span class="action-info">${m.sizeGb}GB</span>` : nothing}
        ${m.improvement ? html`<span class="action-info improve">+${m.improvement.toFixed(1)}%</span>` : nothing}
      </div>
    `;
  }

  private renderAlloyPanel(): TemplateResult {
    const alloyCount = this._models.filter(m => m.hasAlloy).length;

    return html`
      <div>
        <div class="section-label">ForgeAlloy</div>
        <div class="alloy-panel">
          <div class="alloy-row">
            <span class="alloy-key">Models</span>
            <span class="alloy-val">${alloyCount} / ${this._models.length}</span>
          </div>
          <div class="alloy-row">
            <span class="alloy-key">Trust</span>
            <span class="alloy-val trust">self-attested</span>
          </div>
          <div class="alloy-row">
            <span class="alloy-key">Phase</span>
            <span class="alloy-val phase">1 (unsigned)</span>
          </div>
          <a class="alloy-link" href="https://github.com/CambrianTech/forge-alloy" target="_blank">
            github.com/CambrianTech/forge-alloy →
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
