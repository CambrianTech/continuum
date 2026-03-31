/**
 * FactoryWidget — Model forge production floor (composition root)
 *
 * Thin orchestrator that:
 * - Loads data (published models, forge status)
 * - Subscribes to forge events
 * - Composes child components via Lit composition
 *
 * Child components (each owns its own styles and display logic):
 * - forge-controls-element: Forge parameter form + start button
 * - active-forge-element: Live forge status with metrics and sparkline
 * - published-models-element: Leaderboard-style model list
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

// Import child components (self-registering)
import './ForgeControlsElement';
import './ForgeDeltaElement';
import './ActiveForgeElement';
import './PublishedModelsElement';

import type { ForgeStatusData } from './ActiveForgeElement';
import type { PublishedModelData } from './PublishedModelsElement';

// ── Component ───────────────────────────────────────────────────────────

export class FactoryWidget extends ReactiveWidget {

  // ── State ──────────────────────────────────────────────────────────
  @reactive() private _forgeStatus: ForgeStatusData | null = null;
  @reactive() private _models: PublishedModelData[] = [];
  @reactive() private _lossHistory: number[] = [];
  @reactive() private _isLoading = true;
  @reactive() private _totalDownloads = 0;
  @reactive() private _forgeStarting = false;

  private _statusPollInterval: ReturnType<typeof setInterval> | null = null;

  // ── Forge progress (derived) ───────────────────────────────────────

  private get _progressPct(): number {
    const s = this._forgeStatus;
    if (!s) return 0;
    const totalSteps = (s.totalSteps ?? 1000) * (s.totalCycles ?? 1);
    const currentStep = ((s.cycle ?? 1) - 1) * (s.totalSteps ?? 1000) + (s.step ?? 0);
    return Math.min(100, Math.round((currentStep / totalSteps) * 100));
  }

  private get _progressLabel(): string {
    const s = this._forgeStatus;
    if (!s) return 'FORGING...';
    const pct = this._progressPct;
    const loss = s.loss && s.loss > 0 ? ` · ${s.loss.toFixed(3)}` : '';
    const eta = s.etaSeconds ? ` · ${this.formatETA(s.etaSeconds)}` : '';
    if (s.phase === 'loading' || s.phase === 'loading_data') return 'Loading...';
    if (s.phase === 'baseline_eval') return 'Baseline...';
    if (s.phase === 'complete') return 'Done';
    return `${pct}%${loss}${eta}`;
  }

  private get _isForging(): boolean {
    const phase = this._forgeStatus?.phase;
    return phase === 'training' || phase === 'loading' || phase === 'loading_data'
      || phase === 'baseline_eval' || phase === 'pruning' || phase === 'running'
      || phase === 'post_train_eval' || phase === 'post_prune_eval' || phase === 'defrag';
  }

  // ── Lifecycle ──────────────────────────────────────────────────────

  override connectedCallback(): void {
    super.connectedCallback();
    this.subscribeToForgeEvents();
    this.loadPublishedModels();
    this.startStatusPolling();
    this.configureRightPanel();
  }

  /** Tell the right panel what widget to show for the factory */
  private configureRightPanel(): void {
    // Small delay to ensure right panel widget is mounted and listening
    setTimeout(() => this.emitRightPanelConfig(), 500);
  }

  private emitRightPanelConfig(): void {
    Events.emit('layout:rightpanel:configure', {
      widget: 'factory-stats-widget',
      contentType: 'factory',
      sections: [{
        id: 'factory-stats',
        title: 'Models',
        icon: '🏭',
        widgetTag: 'factory-stats-widget',
        flexWeight: 1,
      }],
    });
  }

  override disconnectedCallback(): void {
    super.disconnectedCallback();
    if (this._statusPollInterval) {
      clearInterval(this._statusPollInterval);
      this._statusPollInterval = null;
    }
  }

  // ── Event Subscriptions ────────────────────────────────────────────

  private subscribeToForgeEvents(): void {
    Events.subscribe('model:forge:step', (data: any) => {
      this._forgeStatus = {
        phase: 'training',
        detail: data.detail ?? '',
        vramGb: data.vramGb ?? 0,
        timestamp: data.timestamp ?? new Date().toISOString(),
        step: data.step,
        totalSteps: data.totalSteps,
        loss: data.loss,
        itPerSec: data.itPerSec,
        etaSeconds: data.etaSeconds,
        cycle: data.cycle,
        totalCycles: data.totalCycles,
      };
      if (data.loss !== undefined) {
        this._lossHistory = [...this._lossHistory.slice(-50), data.loss];
      }
    });

    Events.subscribe('model:forge:phase', (data: any) => {
      if (this._forgeStatus) {
        this._forgeStatus = { ...this._forgeStatus, phase: data.phase, detail: data.detail ?? '' };
      }
    });

    Events.subscribe('model:forge:complete', (data: any) => {
      this._forgeStatus = {
        phase: 'complete',
        detail: data.detail ?? 'Forge complete',
        vramGb: 0,
        timestamp: data.timestamp ?? new Date().toISOString(),
        improvementPct: data.improvementPct,
        perplexity: data.perplexity,
      };
      this.loadPublishedModels();
    });
  }

  // ── Status Polling ─────────────────────────────────────────────────

  private startStatusPolling(): void {
    this.pollForgeStatus();
    this._statusPollInterval = setInterval(() => this.pollForgeStatus(), 15_000);
  }

  private async pollForgeStatus(): Promise<void> {
    try {
      const result = await this.executeCommand<any, any>('model/forge-status', {});
      if (!result?.forges?.length) {
        // No active forges — clear stale status
        if (this._forgeStatus && this._forgeStatus.phase !== 'complete') {
          this._forgeStatus = null;
        }
        return;
      }
      if (result?.forges?.length > 0) {
        const f = result.forges[0];
        this._forgeStatus = {
          phase: f.phase ?? 'unknown',
          detail: f.detail ?? '',
          vramGb: f.vramGb ?? 0,
          timestamp: f.timestamp ?? new Date().toISOString(),
          step: f.step,
          totalSteps: f.totalSteps,
          loss: f.loss,
          itPerSec: f.itPerSec,
          etaSeconds: f.etaSeconds,
          cycle: f.cycle,
          totalCycles: f.totalCycles,
        };
        if (f.loss && f.loss > 0) {
          this._lossHistory = [...this._lossHistory.slice(-50), f.loss];
        }
        if (f.phase === 'complete' || f.phase === 'error') {
          if (this._statusPollInterval) {
            clearInterval(this._statusPollInterval);
            this._statusPollInterval = null;
          }
          this.loadPublishedModels();
        }
      }
    } catch {
      // Node unreachable
    }
  }

  // ── Data Loading ───────────────────────────────────────────────────

  private async loadPublishedModels(): Promise<void> {
    this._isLoading = true;
    try {
      const result = await this.executeCommand<any, any>('model/list-published', { includeGguf: true });
      if (result?.models) {
        this._models = result.models.sort((a: any, b: any) => b.downloads - a.downloads);
        this._totalDownloads = result.totalDownloads ?? 0;
      }
    } catch {
      this._models = [];
    }
    this._isLoading = false;
  }

  // ── Event Handlers (from child components) ─────────────────────────

  private async onForgeStart(e: CustomEvent): Promise<void> {
    if (this._isForging || this._forgeStarting) return;
    this._forgeStarting = true;
    try {
      await this.executeCommand<any, any>('model/forge', e.detail);
    } catch (err) {
      console.error('Forge start failed:', err);
    } finally {
      this._forgeStarting = false;
    }
  }

  private onForgeExport(e: CustomEvent): void {
    const alloy = e.detail;
    const blob = new Blob([JSON.stringify(alloy, null, 2)], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `${alloy.name}.alloy.json`;
    a.click();
    URL.revokeObjectURL(url);
  }

  // ── Rendering ──────────────────────────────────────────────────────

  static override styles: CSSResultGroup = [
    ReactiveWidget.styles,
    css`
      :host {
        display: block;
        width: 100%;
        height: 100%;
        overflow-y: auto;
        color: var(--content-primary, #e0e6ed);
      }

      .factory {
        padding: 20px 24px;
        max-width: 1200px;
      }

      .header {
        display: flex;
        justify-content: space-between;
        align-items: baseline;
        margin-bottom: 24px;
      }

      .title {
        font-size: 20px;
        font-weight: 700;
      }

      .subtitle {
        font-size: 12px;
        color: var(--content-secondary, #8a92a5);
      }

      .section {
        margin-bottom: 28px;
      }

      .section-title {
        font-size: 14px;
        font-weight: 600;
        text-transform: uppercase;
        letter-spacing: 0.05em;
        color: var(--content-secondary, #8a92a5);
        margin-bottom: 12px;
        padding-bottom: 6px;
        border-bottom: 1px solid var(--border-color, rgba(255,255,255,0.08));
      }

      .section-stats {
        font-size: 12px;
        font-weight: 400;
        color: var(--content-secondary, #8a92a5);
        margin-left: 12px;
      }
    `,
  ];

  protected override render(): TemplateResult {
    return html`
      <div class="factory">
        <div class="header">
          <span class="title">Model Factory</span>
          <span class="subtitle">continuum-ai</span>
        </div>

        <div class="section">
          <div class="section-title">Forge</div>
          <forge-controls-element
            .forging=${this._isForging}
            .starting=${this._forgeStarting}
            .progressPct=${this._progressPct}
            .progressLabel=${this._progressLabel}
            @forge-start=${this.onForgeStart}
            @forge-export=${this.onForgeExport}
          ></forge-controls-element>
        </div>

        <div class="section">
          <div class="section-title">Active Forge</div>
          <active-forge-element
            .status=${this._forgeStatus}
            .lossHistory=${this._lossHistory}
          ></active-forge-element>
        </div>

      </div>
    `;
  }

  // ── Helpers ────────────────────────────────────────────────────────

  private formatETA(seconds: number): string {
    if (seconds < 60) return `${Math.round(seconds)}s`;
    if (seconds < 3600) return `${Math.round(seconds / 60)}m`;
    const h = Math.floor(seconds / 3600);
    const m = Math.round((seconds % 3600) / 60);
    return `${h}h${m}m`;
  }
}

// Self-register
if (!customElements.get('factory-widget')) {
  customElements.define('factory-widget', FactoryWidget);
}
