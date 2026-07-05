/**
 * ActiveForgeElement — Shows current forge status with metrics and sparkline
 *
 * Receives forge status via properties from parent.
 * Pure display component — no commands, no event subscriptions.
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

export interface ForgeStatusData {
  phase: string;
  detail: string;
  vramGb: number;
  timestamp: string;
  step?: number;
  totalSteps?: number;
  loss?: number;
  itPerSec?: number;
  etaSeconds?: number;
  cycle?: number;
  totalCycles?: number;
  perplexity?: number;
  improvementPct?: number;
}

export class ActiveForgeElement extends ReactiveWidget {

  @reactive() status: ForgeStatusData | null = null;
  @reactive() lossHistory: number[] = [];

  static override styles: CSSResultGroup = [
    ReactiveWidget.styles,
    css`
    :host { display: block; }

    .forge-card {
      background: var(--surface-elevated, rgba(255,255,255,0.04));
      border: 1px solid var(--border-color, rgba(255,255,255,0.08));
      border-radius: 8px;
      padding: 16px 20px;
    }

    .forge-header {
      display: flex;
      justify-content: space-between;
      align-items: center;
      margin-bottom: 12px;
    }

    .forge-phase {
      font-size: 13px;
      font-weight: 600;
      color: var(--accent-primary, #00d4ff);
    }

    .forge-detail {
      font-size: 12px;
      color: var(--content-secondary, #8a92a5);
    }

    .forge-metrics {
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(120px, 1fr));
      gap: 12px;
      margin-top: 12px;
    }

    .metric { text-align: center; }

    .metric-value {
      font-size: 24px;
      font-weight: 700;
      font-variant-numeric: tabular-nums;
    }

    .metric-label {
      font-size: 11px;
      color: var(--content-secondary, #8a92a5);
      text-transform: uppercase;
      letter-spacing: 0.04em;
    }

    .metric-value.good { color: #00ffc8; }
    .metric-value.warn { color: #ffaa00; }
    .metric-value.neutral { color: var(--content-primary, #e0e6ed); }

    .progress-bar {
      height: 4px;
      background: rgba(255,255,255,0.06);
      border-radius: 2px;
      overflow: hidden;
      margin-top: 12px;
    }

    .progress-fill {
      height: 100%;
      background: linear-gradient(90deg, #00d4ff, #00ffc8);
      border-radius: 2px;
      transition: width 0.3s ease;
    }

    .sparkline { margin-top: 8px; }
    .sparkline svg { width: 100%; height: 40px; }
    .sparkline path { fill: none; stroke: #00ffc8; stroke-width: 1.5; }

    .empty-state {
      text-align: center;
      padding: 40px 20px;
      color: var(--content-secondary, #8a92a5);
    }

    .empty-icon { font-size: 36px; margin-bottom: 8px; }
    .empty-message { font-size: 14px; margin-bottom: 4px; }
    .empty-hint { font-size: 12px; opacity: 0.7; }
  `];

  protected override render(): TemplateResult {
    return this.status ? this.renderForgeCard() : this.renderEmpty();
  }

  private renderForgeCard(): TemplateResult {
    const s = this.status!;
    const progress = s.step && s.totalSteps ? (s.step / s.totalSteps) * 100 : 0;
    const eta = s.etaSeconds ? this.formatETA(s.etaSeconds) : '--';
    const lossClass = s.loss !== undefined ? (s.loss < 2.5 ? 'good' : s.loss < 3.0 ? 'warn' : 'neutral') : 'neutral';

    return html`
      <div class="forge-card">
        <div class="forge-header">
          <span class="forge-phase">${s.phase.toUpperCase()}</span>
          <span class="forge-detail">${s.detail}</span>
        </div>
        <div class="forge-metrics">
          <div class="metric">
            <div class="metric-value ${lossClass}">${s.loss?.toFixed(2) ?? '--'}</div>
            <div class="metric-label">Loss</div>
          </div>
          <div class="metric">
            <div class="metric-value neutral">${s.step ?? '--'}/${s.totalSteps ?? '--'}</div>
            <div class="metric-label">Step</div>
          </div>
          <div class="metric">
            <div class="metric-value neutral">${eta}</div>
            <div class="metric-label">ETA</div>
          </div>
          <div class="metric">
            <div class="metric-value ${s.vramGb > 28 ? 'warn' : 'neutral'}">${s.vramGb?.toFixed(1) ?? '--'}GB</div>
            <div class="metric-label">VRAM</div>
          </div>
          <div class="metric">
            <div class="metric-value neutral">${s.itPerSec?.toFixed(1) ?? '--'}</div>
            <div class="metric-label">it/s</div>
          </div>
          ${s.improvementPct !== undefined ? html`
          <div class="metric">
            <div class="metric-value good">${s.improvementPct > 0 ? '+' : ''}${s.improvementPct.toFixed(1)}%</div>
            <div class="metric-label">Improvement</div>
          </div>
          ` : nothing}
        </div>
        ${progress > 0 ? html`
          <div class="progress-bar">
            <div class="progress-fill" style=${`width:${progress}%`}></div>
          </div>
        ` : nothing}
        ${this.lossHistory.length > 2 ? this.renderSparkline() : nothing}
      </div>
    `;
  }

  private renderEmpty(): TemplateResult {
    return html`
      <div class="empty-state">
        <div class="empty-icon">&#9881;</div>
        <div class="empty-message">No active forges</div>
        <div class="empty-hint">Configure a forge above and hit START FORGE</div>
      </div>
    `;
  }

  private renderSparkline() {
    const data = this.lossHistory;
    if (data.length < 2) return nothing;

    const min = Math.min(...data);
    const max = Math.max(...data);
    const range = max - min || 1;
    const w = 100;
    const h = 40;

    const points = data.map((v, i) => {
      const x = (i / (data.length - 1)) * w;
      const y = h - ((v - min) / range) * (h - 4) - 2;
      return `${x},${y}`;
    });

    return html`
      <div class="sparkline">
        <svg viewBox="0 0 ${w} ${h}" preserveAspectRatio="none">
          <path d="M ${points.join(' L ')}" />
        </svg>
      </div>
    `;
  }

  private formatETA(seconds: number): string {
    if (seconds < 60) return `${Math.round(seconds)}s`;
    if (seconds < 3600) return `${Math.round(seconds / 60)}m`;
    const h = Math.floor(seconds / 3600);
    const m = Math.round((seconds % 3600) / 60);
    return `${h}h${m}m`;
  }
}

if (!customElements.get('active-forge-element')) {
  customElements.define('active-forge-element', ActiveForgeElement);
}
