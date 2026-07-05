/**
 * GenomeProfileWidget — Comprehensive genome view for a persona
 *
 * Sections:
 *   1. Genome Overview — total adapters, average maturity, domains, overall fitness
 *   2. Adapter Table — name, domain, maturity bar, loss, examples, created, base model
 *   3. Academy History — session cards with skill, mode, status, score, restart button
 *
 * Data sources:
 *   - genome/layers (adapter inventory with maturity)
 *   - genome/academy-session-list (session history)
 *
 * Opened via: UserProfileWidget pathway card → ContentService.open('genome-profile', userId)
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
import { GenomeLayers, type GenomeLayerInfo } from '../../commands/genome/layers/shared/GenomeLayersTypes';
import { GenomeAcademySessionList, type AcademySessionSummary } from '../../commands/genome/academy-session-list/shared/GenomeAcademySessionListTypes';
import { GenomeAcademySessionRestart } from '../../commands/genome/academy-session-restart/shared/GenomeAcademySessionRestartTypes';
import { GenomeAdapterPrune } from '../../commands/genome/adapter-prune/shared/GenomeAdapterPruneTypes';
import { Events } from '../../system/core/shared/Events';
import { contentState } from '../../system/state/ContentStateService';
import { DataRead } from '../../commands/data/read/shared/DataReadTypes';
import { ALL_PANEL_STYLES } from '../shared/styles';

// Import sidebar section widgets — triggers customElements.define() registration
// so RightPanelWidget can create them via document.createElement()
import './TrainingStatusSection';
import './AdapterActionsSection';

// === STYLES (must be before class for static styles reference) ===

const GENOME_PROFILE_STYLES = `
  .genome-profile {
    padding: 16px;
    color: var(--content-primary, #e0e0e0);
    font-family: var(--font-mono, monospace);
    max-width: 900px;
    margin: 0 auto;
  }

  .genome-profile.loading,
  .genome-profile.error {
    padding: 32px;
    text-align: center;
    color: var(--content-secondary, #999);
  }

  .section {
    margin-bottom: 24px;
  }

  .section-title {
    font-size: 14px;
    font-weight: 700;
    color: rgba(0, 255, 200, 0.9);
    text-transform: uppercase;
    letter-spacing: 1px;
    margin: 0 0 12px 0;
    padding-bottom: 6px;
    border-bottom: 1px solid rgba(0, 255, 200, 0.2);
  }

  .empty-state, .section-loading {
    padding: 16px;
    text-align: center;
    color: var(--content-secondary, #777);
    font-style: italic;
    font-size: 12px;
  }

  .genome-profile.empty {
    padding: 32px;
    text-align: center;
    color: var(--content-secondary, #999);
  }

  /* === OVERVIEW === */

  .overview-grid {
    display: flex;
    gap: 12px;
    flex-wrap: wrap;
  }

  .stat-card {
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: 12px 16px;
    background: rgba(10, 25, 35, 0.8);
    border: 1px solid rgba(0, 255, 200, 0.2);
    border-radius: 6px;
    min-width: 80px;
  }

  .stat-value {
    font-size: 22px;
    font-weight: 700;
    color: rgba(0, 255, 200, 0.9);
  }

  .stat-label {
    font-size: 10px;
    color: var(--content-secondary, #999);
    text-transform: uppercase;
    letter-spacing: 0.5px;
    margin-top: 4px;
  }

  /* === ADAPTER TABLE === */

  .adapter-table {
    border: 1px solid rgba(0, 255, 200, 0.15);
    border-radius: 6px;
    overflow: hidden;
  }

  .adapter-header {
    display: grid;
    grid-template-columns: 2fr 1fr 120px 60px 60px 1fr;
    gap: 8px;
    padding: 8px 12px;
    background: rgba(0, 255, 200, 0.05);
    border-bottom: 1px solid rgba(0, 255, 200, 0.15);
    font-size: 10px;
    font-weight: 700;
    color: rgba(0, 255, 200, 0.7);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .adapter-row {
    display: grid;
    grid-template-columns: 2fr 1fr 120px 60px 60px 1fr;
    gap: 8px;
    padding: 8px 12px;
    border-bottom: 1px solid rgba(60, 80, 100, 0.2);
    font-size: 12px;
    cursor: pointer;
    transition: background 0.2s;
  }

  .adapter-row:hover {
    background: rgba(0, 255, 200, 0.03);
  }

  .adapter-row.expanded {
    background: rgba(0, 255, 200, 0.05);
  }

  .col-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .col-domain {
    color: var(--content-secondary, #999);
  }

  .col-maturity {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .maturity-bar-wrapper {
    flex: 1;
    height: 8px;
    background: rgba(20, 30, 45, 0.6);
    border: 1px solid rgba(60, 80, 100, 0.4);
    border-radius: 3px;
    overflow: hidden;
  }

  .maturity-bar {
    height: 100%;
    border-radius: 2px;
    transition: width 0.4s ease;
  }

  .maturity-pct {
    font-size: 10px;
    min-width: 30px;
    text-align: right;
    color: var(--content-secondary, #999);
  }

  /* === ADAPTER DETAIL (expanded) === */

  .adapter-detail {
    padding: 12px 16px;
    background: rgba(10, 25, 35, 0.5);
    border-bottom: 1px solid rgba(60, 80, 100, 0.3);
  }

  .detail-grid {
    display: flex;
    gap: 16px;
    flex-wrap: wrap;
    margin-bottom: 8px;
  }

  .detail-item {
    display: flex;
    flex-direction: column;
  }

  .detail-label {
    font-size: 9px;
    color: var(--content-secondary, #777);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .detail-value {
    font-size: 13px;
    color: var(--content-primary, #e0e0e0);
  }

  .detail-value.improvement {
    color: #00ff88;
    font-weight: 700;
  }

  .sparkline-container {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 8px;
  }

  .sparkline-label {
    font-size: 9px;
    color: var(--content-secondary, #777);
    text-transform: uppercase;
    white-space: nowrap;
  }

  .sparkline {
    width: 200px;
    height: 40px;
  }

  .sparkline-range {
    display: flex;
    flex-direction: column;
    font-size: 9px;
    color: var(--content-secondary, #777);
  }

  /* === ACADEMY HISTORY === */

  .session-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .session-card {
    padding: 12px;
    background: rgba(10, 25, 35, 0.8);
    border: 1px solid rgba(60, 80, 100, 0.3);
    border-radius: 6px;
  }

  .session-card.active-session {
    border-color: rgba(255, 170, 0, 0.4);
    background: rgba(255, 170, 0, 0.05);
    animation: session-pulse 2s ease-in-out infinite;
  }

  @keyframes session-pulse {
    0%, 100% { border-color: rgba(255, 170, 0, 0.4); }
    50% { border-color: rgba(255, 170, 0, 0.7); }
  }

  .active-badge {
    font-size: 10px;
    font-weight: 700;
    color: #ffaa00;
    background: rgba(255, 170, 0, 0.12);
    padding: 1px 6px;
    border-radius: 3px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .session-header {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 6px;
  }

  .session-skill {
    font-size: 14px;
    font-weight: 600;
    color: var(--content-primary, #e0e0e0);
  }

  .session-mode-badge {
    font-size: 9px;
    font-weight: 700;
    color: rgba(0, 212, 255, 0.9);
    background: rgba(0, 212, 255, 0.1);
    padding: 2px 6px;
    border-radius: 3px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .session-status {
    margin-left: auto;
    font-size: 11px;
    font-weight: 600;
  }

  .session-status.complete { color: #00ff88; }
  .session-status.failed { color: #ff6b6b; }
  .session-status.in-progress { color: #ffaa00; }

  .session-meta {
    display: flex;
    gap: 16px;
    font-size: 11px;
    color: var(--content-secondary, #999);
    margin-bottom: 6px;
  }

  .session-metrics {
    display: flex;
    gap: 16px;
    font-size: 11px;
    color: var(--content-primary, #ccc);
    margin-bottom: 8px;
  }

  .session-actions {
    display: flex;
    gap: 8px;
  }

  .restart-btn {
    font-size: 10px;
    font-weight: 600;
    color: rgba(0, 212, 255, 0.9);
    background: rgba(0, 212, 255, 0.08);
    border: 1px solid rgba(0, 212, 255, 0.3);
    border-radius: 4px;
    padding: 4px 10px;
    cursor: pointer;
    transition: background 0.2s, border-color 0.2s;
    font-family: inherit;
  }

  .restart-btn:hover {
    background: rgba(0, 212, 255, 0.15);
    border-color: rgba(0, 212, 255, 0.6);
  }

  /* === Section title with actions === */

  .section-title-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 12px;
    padding-bottom: 6px;
    border-bottom: 1px solid rgba(0, 255, 200, 0.2);
  }

  .section-title-row .section-title {
    margin: 0;
    padding: 0;
    border: none;
  }

  /* === Action buttons === */

  .action-btn {
    font-size: 10px;
    font-weight: 600;
    border-radius: 4px;
    padding: 4px 10px;
    cursor: pointer;
    font-family: inherit;
    transition: background 0.2s, border-color 0.2s;
  }

  .action-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .prune-btn {
    color: rgba(255, 170, 0, 0.9);
    background: rgba(255, 170, 0, 0.08);
    border: 1px solid rgba(255, 170, 0, 0.3);
  }

  .prune-btn:hover:not(:disabled) {
    background: rgba(255, 170, 0, 0.15);
    border-color: rgba(255, 170, 0, 0.6);
  }

  .delete-btn {
    color: rgba(255, 107, 107, 0.9);
    background: rgba(255, 107, 107, 0.08);
    border: 1px solid rgba(255, 107, 107, 0.3);
  }

  .delete-btn:hover:not(:disabled) {
    background: rgba(255, 107, 107, 0.15);
    border-color: rgba(255, 107, 107, 0.6);
  }

  .adapter-actions-row {
    display: flex;
    gap: 8px;
    margin-top: 10px;
    padding-top: 8px;
    border-top: 1px solid rgba(60, 80, 100, 0.2);
  }
`;

export class GenomeProfileWidget extends ReactiveWidget {
  @reactive() private _userId: string = '';
  @reactive() private _displayName: string = '';

  // Per-section data and loading states — each section loads independently
  @reactive() private _layers: GenomeLayerInfo[] = [];
  @reactive() private _fitness: number = 0;
  @reactive() private _layersLoading: boolean = false;

  @reactive() private _sessions: AcademySessionSummary[] = [];
  @reactive() private _sessionsLoading: boolean = false;

  @reactive() private _expandedAdapter: number = -1;
  @reactive() private _pruning: Set<string> = new Set();

  private _eventCleanups: (() => void)[] = [];

  static override styles = [unsafeCSS(ALL_PANEL_STYLES), unsafeCSS(GENOME_PROFILE_STYLES)] as CSSResultGroup;

  override disconnectedCallback(): void {
    super.disconnectedCallback();
    this._eventCleanups.forEach(fn => fn());
    this._eventCleanups = [];
  }

  /**
   * Called by MainWidget when this content view becomes active.
   * Receives the entityId (persona UUID) and optional metadata.
   */
  onActivate(entityId?: string, metadata?: Record<string, unknown>): void {
    if (!entityId) return;

    const entity = metadata?.entity as { displayName?: string; uniqueId?: string } | undefined;
    const displayName = entity?.displayName ?? '';

    if (entityId !== this._userId) {
      this._userId = entityId;
      this._displayName = displayName;
      this._expandedAdapter = -1;

      // Resolve persona name if not provided in metadata
      if (!displayName) {
        this._resolvePersonaName(entityId);
      } else {
        this._updateTabTitle(displayName);
      }

      // Subscribe to data events for live updates
      this._subscribeToDataEvents();

      // Fire both fetches independently — each section manages its own loading
      this._fetchLayers();
      this._fetchSessions();
    }
  }

  /**
   * Look up persona display name from user entity and update tab title.
   */
  private async _resolvePersonaName(userId: string): Promise<void> {
    try {
      const result = await DataRead.execute({ collection: 'users', id: userId });
      const user = result.data as unknown as { displayName?: string } | undefined;
      if (user?.displayName) {
        this._displayName = user.displayName;
        this._updateTabTitle(user.displayName);
      }
    } catch {
      // Non-critical — tab just keeps generic title
    }
  }

  private _updateTabTitle(name: string): void {
    const shortName = name.length > 20 ? name.slice(0, 18) + '...' : name;
    contentState.updateItemTitle('genome-profile', this._userId, `${shortName} Genome`);
  }

  private _subscribeToDataEvents(): void {
    // Clean up previous subscriptions
    this._eventCleanups.forEach(fn => fn());
    this._eventCleanups = [];

    // When academy sessions change, refetch
    this._eventCleanups.push(
      Events.subscribe('data:academy_sessions:created', () => this._fetchSessions()),
      Events.subscribe('data:academy_sessions:updated', () => this._fetchSessions()),
    );
  }

  private async _fetchLayers(): Promise<void> {
    this._layersLoading = true;
    try {
      const result = await GenomeLayers.execute({ personaId: this._userId, personaName: this._displayName });
      if (result.success) {
        this._layers = result.layers;
        this._fitness = result.fitness;
      }
    } catch (err) {
      console.error('Failed to load layers:', err);
    } finally {
      this._layersLoading = false;
    }
  }

  private async _fetchSessions(): Promise<void> {
    this._sessionsLoading = true;
    try {
      const result = await GenomeAcademySessionList.execute({ personaId: this._userId, limit: 20 });
      if (result.success) {
        this._sessions = result.sessions;
      }
    } catch (err) {
      console.error('Failed to load sessions:', err);
    } finally {
      this._sessionsLoading = false;
    }
  }

  private async _deleteAdapter(adapterName: string): Promise<void> {
    const newSet = new Set(this._pruning);
    newSet.add(adapterName);
    this._pruning = newSet;

    try {
      const result = await GenomeAdapterPrune.execute({
        personaId: this._userId,
        domain: adapterName,  // TODO: need a proper adapter-delete command, prune by domain for now
        keepLatest: 0,
      });
      if (result.success) {
        // Refresh adapter list
        this._fetchLayers();
      }
    } catch (err) {
      console.error('Failed to delete adapter:', err);
    } finally {
      const cleaned = new Set(this._pruning);
      cleaned.delete(adapterName);
      this._pruning = cleaned;
    }
  }

  private async _pruneAll(): Promise<void> {
    try {
      const result = await GenomeAdapterPrune.execute({
        personaId: this._userId,
        keepLatest: 1,  // Keep latest per domain, prune old duplicates
      });
      if (result.success) {
        this._fetchLayers();
      }
    } catch (err) {
      console.error('Failed to prune adapters:', err);
    }
  }

  private async _restartSession(sessionId: string): Promise<void> {
    try {
      const result = await GenomeAcademySessionRestart.execute({ sessionId });
      if (result.success) {
        this._fetchSessions();
      }
    } catch (err) {
      console.error('Failed to restart session:', err);
    }
  }

  protected override render(): TemplateResult {
    if (!this._userId) {
      return html`<div class="genome-profile empty">Select a persona to view their genome.</div>`;
    }

    return html`
      <div class="genome-profile">
        ${this._renderOverview()}
        ${this._renderAdapterTable()}
        ${this._renderAcademyHistory()}
      </div>
    `;
  }

  // === OVERVIEW SECTION ===

  private _renderOverview(): TemplateResult {
    if (this._layersLoading && this._layers.length === 0) {
      return html`<section class="section overview"><div class="section-loading">Loading adapters...</div></section>`;
    }
    const trainedCount = this._layers.filter(l => l.hasWeights).length;
    const avgMaturity = this._layers.length > 0
      ? this._layers.reduce((sum, l) => sum + l.maturity, 0) / this._layers.length
      : 0;
    const domains = new Set(this._layers.map(l => l.domain));
    const totalSizeMB = this._layers.reduce((sum, l) => sum + (l.sizeMB ?? 0), 0);
    const sizeDisplay = totalSizeMB >= 1024
      ? `${(totalSizeMB / 1024).toFixed(1)} GB`
      : `${Math.round(totalSizeMB)} MB`;

    return html`
      <section class="section overview">
        <div class="section-title-row">
          <h2 class="section-title">Genome Overview</h2>
          ${this._layers.length > 1 ? html`
            <button class="action-btn prune-btn" @click=${() => this._pruneAll()} title="Keep latest adapter per domain, delete old duplicates">
              Prune Duplicates
            </button>
          ` : nothing}
        </div>
        <div class="overview-grid">
          <div class="stat-card">
            <span class="stat-value">${this._layers.length}</span>
            <span class="stat-label">Adapters</span>
          </div>
          <div class="stat-card">
            <span class="stat-value">${trainedCount}</span>
            <span class="stat-label">Trained</span>
          </div>
          <div class="stat-card">
            <span class="stat-value">${Math.round(avgMaturity * 100)}%</span>
            <span class="stat-label">Maturity</span>
          </div>
          <div class="stat-card">
            <span class="stat-value">${domains.size}</span>
            <span class="stat-label">Domains</span>
          </div>
          <div class="stat-card">
            <span class="stat-value">${sizeDisplay}</span>
            <span class="stat-label">Disk</span>
          </div>
          <div class="stat-card">
            <span class="stat-value">${Math.round(this._fitness * 100)}%</span>
            <span class="stat-label">Fitness</span>
          </div>
        </div>
      </section>
    `;
  }

  // === ADAPTER TABLE ===

  private _renderAdapterTable(): TemplateResult {
    if (this._layersLoading && this._layers.length === 0) {
      return html`<section class="section"><h2 class="section-title">Adapters</h2><div class="section-loading">Loading...</div></section>`;
    }
    if (this._layers.length === 0) {
      return html`
        <section class="section">
          <h2 class="section-title">Adapters</h2>
          <div class="empty-state">No adapters found. Train skills via the Academy.</div>
        </section>
      `;
    }

    return html`
      <section class="section">
        <h2 class="section-title">Adapters (${this._layers.length})</h2>
        <div class="adapter-table">
          <div class="adapter-header">
            <span class="col-name">Name</span>
            <span class="col-domain">Domain</span>
            <span class="col-maturity">Maturity</span>
            <span class="col-loss">Loss</span>
            <span class="col-examples">Examples</span>
            <span class="col-model">Base Model</span>
          </div>
          ${this._layers.map((layer, i) => this._renderAdapterRow(layer, i))}
        </div>
      </section>
    `;
  }

  private _renderAdapterRow(layer: GenomeLayerInfo, index: number): TemplateResult {
    const m = layer.trainingMetrics;
    const matPct = Math.round(layer.maturity * 100);
    const matColor = this._maturityColor(layer.maturity);
    const isExpanded = this._expandedAdapter === index;

    return html`
      <div class="adapter-row ${isExpanded ? 'expanded' : ''}" @click=${() => { this._expandedAdapter = isExpanded ? -1 : index; }}>
        <span class="col-name" title=${layer.name}>${layer.name}</span>
        <span class="col-domain">${layer.domain}</span>
        <span class="col-maturity">
          <div class="maturity-bar-wrapper">
            <div class="maturity-bar" style="width: ${matPct}%; background: ${matColor};"></div>
          </div>
          <span class="maturity-pct">${matPct}%</span>
        </span>
        <span class="col-loss">${m ? m.finalLoss.toFixed(3) : '-'}</span>
        <span class="col-examples">${m ? m.examplesProcessed : '-'}</span>
        <span class="col-model">${this._shortModel(layer.baseModel)}</span>
      </div>
      ${isExpanded ? this._renderAdapterDetail(layer) : nothing}
    `;
  }

  private _renderAdapterDetail(layer: GenomeLayerInfo): TemplateResult {
    const m = layer.trainingMetrics;
    const isPruning = this._pruning.has(layer.name);
    return html`
      <div class="adapter-detail">
        <div class="detail-grid">
          <div class="detail-item">
            <span class="detail-label">Created</span>
            <span class="detail-value">${layer.createdAt ? new Date(layer.createdAt).toLocaleDateString() : '-'}</span>
          </div>
          <div class="detail-item">
            <span class="detail-label">Size</span>
            <span class="detail-value">${layer.sizeMB ? `${layer.sizeMB.toFixed(1)} MB` : '-'}</span>
          </div>
          <div class="detail-item">
            <span class="detail-label">Epochs</span>
            <span class="detail-value">${m?.epochs ?? '-'}</span>
          </div>
          <div class="detail-item">
            <span class="detail-label">Duration</span>
            <span class="detail-value">${m?.trainingDurationMs ? `${(m.trainingDurationMs / 1000).toFixed(1)}s` : '-'}</span>
          </div>
          ${m?.phenotypeScore != null ? html`
            <div class="detail-item">
              <span class="detail-label">Phenotype Score</span>
              <span class="detail-value">${m.phenotypeScore.toFixed(1)}</span>
            </div>
          ` : nothing}
          ${m?.phenotypeImprovement != null ? html`
            <div class="detail-item">
              <span class="detail-label">Improvement</span>
              <span class="detail-value improvement">+${m.phenotypeImprovement.toFixed(1)}</span>
            </div>
          ` : nothing}
        </div>
        ${m?.lossHistory?.length ? this._renderLossSparkline(m.lossHistory) : nothing}
        <div class="adapter-actions-row">
          <button class="action-btn delete-btn" ?disabled=${isPruning}
                  @click=${(e: Event) => { e.stopPropagation(); this._deleteAdapter(layer.name); }}>
            ${isPruning ? 'Deleting...' : 'Delete'}
          </button>
        </div>
      </div>
    `;
  }

  private _renderLossSparkline(history: number[]): TemplateResult {
    if (history.length < 2) return html``;

    const maxLoss = Math.max(...history);
    const minLoss = Math.min(...history);
    const range = maxLoss - minLoss || 1;
    const height = 40;
    const width = 200;
    const step = width / (history.length - 1);

    const points = history.map((loss, i) => {
      const x = i * step;
      const y = height - ((loss - minLoss) / range) * height;
      return `${x},${y}`;
    }).join(' ');

    return html`
      <div class="sparkline-container">
        <span class="sparkline-label">Loss Curve</span>
        <svg class="sparkline" viewBox="0 0 ${width} ${height}" preserveAspectRatio="none">
          <polyline points=${points} fill="none" stroke="var(--genome-cyan, #00d4ff)" stroke-width="1.5" />
        </svg>
        <div class="sparkline-range">
          <span>${maxLoss.toFixed(2)}</span>
          <span>${minLoss.toFixed(2)}</span>
        </div>
      </div>
    `;
  }

  // === ACADEMY HISTORY ===

  private _renderAcademyHistory(): TemplateResult {
    const loadingMsg = this._sessionsLoading && this._sessions.length === 0
      ? html`<div class="section-loading">Loading sessions...</div>` : nothing;

    // Sort: in-progress first, then by date descending
    const sorted = [...this._sessions].sort((a, b) => {
      const aActive = a.status !== 'complete' && a.status !== 'failed' ? 1 : 0;
      const bActive = b.status !== 'complete' && b.status !== 'failed' ? 1 : 0;
      if (aActive !== bActive) return bActive - aActive;
      return new Date(b.createdAt ?? 0).getTime() - new Date(a.createdAt ?? 0).getTime();
    });

    const activeCount = sorted.filter(s => s.status !== 'complete' && s.status !== 'failed').length;

    return html`
      <section class="section">
        <h2 class="section-title">
          Academy${activeCount > 0 ? html` <span class="active-badge">${activeCount} active</span>` : nothing}${this._sessions.length > 0 ? html` (${this._sessions.length})` : nothing}
        </h2>
        ${loadingMsg}
        ${!this._sessionsLoading && this._sessions.length === 0
          ? html`<div class="empty-state">No academy sessions found.</div>`
          : html`<div class="session-list">${sorted.map(s => this._renderSessionCard(s))}</div>`
        }
      </section>
    `;
  }

  private _renderSessionCard(session: AcademySessionSummary): TemplateResult {
    const statusClass = session.status === 'complete' ? 'complete' :
                        session.status === 'failed' ? 'failed' : 'in-progress';
    const statusIcon = session.status === 'complete' ? '\u2713' :
                       session.status === 'failed' ? '\u2717' : '\u21BB';
    const isActive = session.status !== 'complete' && session.status !== 'failed';

    return html`
      <div class="session-card ${isActive ? 'active-session' : ''}"
        <div class="session-header">
          <span class="session-skill">${session.skill}</span>
          <span class="session-mode-badge">${session.mode}</span>
          <span class="session-status ${statusClass}">${statusIcon} ${session.status}</span>
        </div>
        <div class="session-meta">
          <span>Base: ${this._shortModel(session.baseModel)}</span>
          <span>${session.createdAt ? new Date(session.createdAt).toLocaleDateString() : ''}</span>
        </div>
        ${session.metrics ? html`
          <div class="session-metrics">
            <span>Topics: ${session.metrics.topicsPassed}/${session.metrics.topicsPassed + session.metrics.topicsFailed}</span>
            <span>Score: ${Math.round(session.metrics.averageExamScore)}</span>
            <span>Adapters: ${session.metrics.layerIds?.length ?? 0}</span>
          </div>
        ` : nothing}
        <div class="session-actions">
          <button class="restart-btn" @click=${(e: Event) => { e.stopPropagation(); this._restartSession(String(session.id)); }}>
            Restart
          </button>
        </div>
      </div>
    `;
  }

  // === HELPERS ===

  private _maturityColor(maturity: number): string {
    if (maturity >= 0.8) return '#00ff88';
    if (maturity >= 0.6) return '#00d4ff';
    if (maturity >= 0.3) return '#ffaa00';
    return 'rgba(60, 80, 100, 0.5)';
  }

  private _shortModel(model: string): string {
    if (!model) return '-';
    // "unsloth/Llama-3.2-3B-Instruct" → "Llama-3.2-3B"
    const parts = model.split('/');
    const name = parts[parts.length - 1];
    return name.replace('-Instruct', '').replace('-Chat', '');
  }
}

// Register custom element
if (typeof customElements !== 'undefined' && !customElements.get('genome-profile-widget')) {
  customElements.define('genome-profile-widget', GenomeProfileWidget);
}
