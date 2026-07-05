/**
 * PublishedModelsElement — Leaderboard-style list of published models
 *
 * Receives model data via properties from parent.
 * Handles its own expand/collapse state.
 * Pure display component — parent loads data.
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

export interface PublishedModelData {
  id: string;
  name: string;
  downloads: number;
  likes: number;
  domain: string;
  variant: string;
  baseModel: string;
  sizeGb: number;
  tags: string[];
}

export class PublishedModelsElement extends ReactiveWidget {

  @reactive() models: PublishedModelData[] = [];
  @reactive() totalDownloads = 0;
  @reactive() loading = false;

  @reactive() private _expandedIndex = -1;

  static override styles: CSSResultGroup = [
    ReactiveWidget.styles,
    css`
    :host { display: block; }

    .model-list {
      display: flex;
      flex-direction: column;
      gap: 2px;
    }

    .model-card {
      display: flex;
      flex-direction: column;
      background: var(--surface-elevated, rgba(255,255,255,0.04));
      border: 1px solid var(--border-color, rgba(255,255,255,0.08));
      border-radius: 6px;
      padding: 10px 14px;
      transition: border-color 0.2s;
      cursor: pointer;
    }

    .model-card:hover {
      border-color: var(--accent-primary, #00d4ff);
    }

    .model-header {
      display: flex;
      align-items: center;
      gap: 12px;
      width: 100%;
    }

    .model-rank {
      font-size: 12px;
      font-weight: 700;
      color: var(--content-secondary, #8a92a5);
      min-width: 28px;
      text-align: center;
    }

    .model-info { flex: 1; min-width: 0; }

    .model-name {
      font-size: 13px;
      font-weight: 600;
      white-space: nowrap;
      overflow: hidden;
      text-overflow: ellipsis;
    }

    .model-meta {
      display: flex;
      gap: 6px;
      margin-top: 3px;
    }

    .badge {
      font-size: 10px;
      padding: 1px 6px;
      border-radius: 3px;
      font-weight: 500;
    }

    .badge.domain {
      background: rgba(0, 212, 255, 0.1);
      color: var(--accent-primary, #00d4ff);
    }

    .badge.variant {
      background: rgba(0, 255, 200, 0.1);
      color: #00ffc8;
    }

    .badge.alloy {
      font-size: 9px;
      padding: 1px 5px;
      background: rgba(255, 170, 0, 0.15);
      color: #ffaa00;
      border: 1px solid rgba(255, 170, 0, 0.3);
    }

    .model-stats {
      display: flex;
      gap: 16px;
      flex-shrink: 0;
    }

    .stat {
      display: flex;
      flex-direction: column;
      align-items: flex-end;
    }

    .stat-value {
      font-size: 14px;
      font-weight: 700;
      color: var(--content-primary, #e0e6ed);
    }

    .stat-label {
      font-size: 10px;
      color: var(--content-secondary, #8a92a5);
    }

    .model-body {
      width: 100%;
      margin-top: 10px;
      padding-top: 10px;
      border-top: 1px solid var(--border-color, rgba(255,255,255,0.08));
    }

    .model-detail {
      display: flex;
      gap: 16px;
      font-size: 12px;
      color: var(--content-secondary, #8a92a5);
      margin-bottom: 8px;
    }

    .model-detail b {
      color: var(--content-primary, #e0e6ed);
    }

    .model-actions {
      display: flex;
      gap: 8px;
      flex-wrap: wrap;
    }

    .action-link {
      padding: 4px 10px;
      font-size: 11px;
      font-weight: 600;
      border: 1px solid var(--border-color, rgba(255,255,255,0.15));
      border-radius: 4px;
      background: rgba(255,255,255,0.05);
      color: var(--content-primary, #e0e6ed);
      text-decoration: none;
      cursor: pointer;
      transition: all 0.15s;
    }

    .action-link:hover {
      background: rgba(0, 212, 255, 0.15);
      border-color: var(--accent-primary, #00d4ff);
      color: var(--accent-primary, #00d4ff);
    }

    .empty-state {
      text-align: center;
      padding: 40px 20px;
      color: var(--content-secondary, #8a92a5);
    }
  `];

  protected override render(): TemplateResult {
    if (this.models.length === 0 && !this.loading) {
      return html`
        <div class="empty-state">
          <div>No models published yet</div>
          <div style="font-size:12px;opacity:0.7;margin-top:4px">Forge a model to publish it to continuum-ai on HuggingFace</div>
        </div>
      `;
    }

    return html`
      <div class="model-list">
        ${this.models.map((m, i) => this.renderModelCard(m, i))}
      </div>
    `;
  }

  private renderModelCard(m: PublishedModelData, rank: number): TemplateResult {
    const expanded = this._expandedIndex === rank;
    const hfUrl = `https://huggingface.co/${m.id}`;

    return html`
      <div class="model-card" @click=${() => this._expandedIndex = expanded ? -1 : rank}>
        <div class="model-header">
          <div class="model-rank">#${rank + 1}</div>
          <div class="model-info">
            <div class="model-name">${m.name}</div>
            <div class="model-meta">
              <span class="badge domain">${m.domain}</span>
              <span class="badge variant">${m.variant ?? 'forged'}</span>
              ${m.tags?.includes('forge-alloy') ? html`<span class="badge alloy">alloy</span>` : nothing}
            </div>
          </div>
          <div class="model-stats">
            <div class="stat">
              <span class="stat-value">${this.formatCount(m.downloads)}</span>
              <span class="stat-label">downloads</span>
            </div>
            <div class="stat">
              <span class="stat-value">${m.likes || '--'}</span>
              <span class="stat-label">likes</span>
            </div>
          </div>
        </div>
        ${expanded ? html`
          <div class="model-body">
            <div class="model-detail">
              ${m.baseModel ? html`<span>Base: <b>${m.baseModel}</b></span>` : nothing}
              ${m.sizeGb ? html`<span>Size: <b>${m.sizeGb}GB</b></span>` : nothing}
            </div>
            <div class="model-actions">
              <a class="action-link" href="${hfUrl}" target="_blank"
                @click=${(e: Event) => e.stopPropagation()}>View on HF</a>
              ${m.tags?.includes('forge-alloy') ? html`
                <a class="action-link" href="${hfUrl}/resolve/main/${m.name}.alloy.json" target="_blank"
                  @click=${(e: Event) => e.stopPropagation()}>Download Alloy</a>
              ` : nothing}
            </div>
          </div>
        ` : nothing}
      </div>
    `;
  }

  private formatCount(n: number): string {
    if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
    if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
    return n.toString();
  }
}

if (!customElements.get('published-models-element')) {
  customElements.define('published-models-element', PublishedModelsElement);
}
