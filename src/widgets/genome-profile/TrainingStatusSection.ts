/**
 * TrainingStatusSection — Sidebar widget showing real-time training activity
 *
 * Subscribes to AI_LEARNING_EVENTS to show:
 *   - Active training sessions (domain, progress, loss)
 *   - Recent completions with results
 *   - Idle state when nothing is training
 *
 * Self-contained: loads its own data via events, no parent coordination needed.
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
  AI_LEARNING_EVENTS,
  type AITrainingStartedEventData,
  type AITrainingProgressEventData,
  type AITrainingCompleteEventData,
  type AITrainingErrorEventData,
} from '../../system/events/shared/AILearningEvents';

interface ActiveTraining {
  personaId: string;
  personaName: string;
  domain: string;
  provider: string;
  progress: number;
  currentLoss?: number;
  currentEpoch?: number;
  totalEpochs?: number;
  startedAt: number;
  exampleCount: number;
}

interface RecentCompletion {
  personaName: string;
  domain: string;
  finalLoss: number;
  trainingTime: number;
  examplesProcessed: number;
  completedAt: number;
  error?: string;
}

const MAX_RECENT = 5;

const STYLES = `
  :host {
    display: block;
    padding: 8px 10px;
    font-size: 11px;
    color: var(--content-primary, #e0e0e0);
    overflow-y: auto;
  }

  .idle-state {
    text-align: center;
    color: var(--content-secondary, #777);
    font-style: italic;
    padding: 12px 0;
  }

  .active-training {
    background: rgba(0, 255, 200, 0.05);
    border: 1px solid rgba(0, 255, 200, 0.15);
    border-radius: 4px;
    padding: 8px;
    margin-bottom: 6px;
  }

  .training-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 4px;
  }

  .training-domain {
    font-weight: 700;
    color: rgba(0, 255, 200, 0.9);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .training-persona {
    color: var(--content-secondary, #999);
    font-size: 10px;
  }

  .progress-bar-wrapper {
    height: 4px;
    background: rgba(60, 80, 100, 0.4);
    border-radius: 2px;
    overflow: hidden;
    margin: 4px 0;
  }

  .progress-bar {
    height: 100%;
    border-radius: 2px;
    background: linear-gradient(90deg, rgba(0, 255, 200, 0.6), rgba(0, 212, 255, 0.8));
    transition: width 0.3s ease;
  }

  .training-stats {
    display: flex;
    gap: 8px;
    font-size: 10px;
    color: var(--content-secondary, #999);
  }

  .stat-highlight {
    color: rgba(0, 212, 255, 0.8);
  }

  .recent-label {
    font-size: 10px;
    font-weight: 700;
    color: var(--content-secondary, #888);
    text-transform: uppercase;
    letter-spacing: 0.5px;
    margin: 10px 0 4px 0;
  }

  .completion-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 3px 0;
    border-bottom: 1px solid rgba(60, 80, 100, 0.15);
  }

  .completion-domain {
    font-weight: 600;
    color: rgba(0, 212, 255, 0.8);
  }

  .completion-loss {
    color: rgba(0, 255, 200, 0.8);
    font-family: monospace;
    font-size: 10px;
  }

  .completion-error {
    color: rgba(255, 80, 80, 0.9);
    font-size: 10px;
  }

  .completion-time {
    font-size: 10px;
    color: var(--content-secondary, #777);
  }
`;

export class TrainingStatusSection extends ReactiveWidget {
  static override styles = [unsafeCSS(STYLES)] as CSSResultGroup;

  @reactive() private _active: Map<string, ActiveTraining> = new Map();
  @reactive() private _recent: RecentCompletion[] = [];

  private _cleanups: (() => void)[] = [];

  constructor() {
    super({ widgetName: 'TrainingStatusSection' });
  }

  protected override async onFirstRender(): Promise<void> {
    super.onFirstRender();

    this._cleanups.push(
      Events.subscribe(AI_LEARNING_EVENTS.TRAINING_STARTED, (data: AITrainingStartedEventData) => {
        const updated = new Map(this._active);
        updated.set(data.personaId, {
          personaId: data.personaId,
          personaName: data.personaName,
          domain: data.domain,
          provider: data.provider,
          progress: 0,
          startedAt: data.timestamp,
          exampleCount: data.exampleCount,
        });
        this._active = updated;
      }),

      Events.subscribe(AI_LEARNING_EVENTS.TRAINING_PROGRESS, (data: AITrainingProgressEventData) => {
        const existing = this._active.get(data.personaId);
        if (existing) {
          const updated = new Map(this._active);
          updated.set(data.personaId, {
            ...existing,
            progress: data.progress,
            currentLoss: data.currentLoss,
            currentEpoch: data.currentEpoch,
            totalEpochs: data.totalEpochs,
          });
          this._active = updated;
        }
      }),

      Events.subscribe(AI_LEARNING_EVENTS.TRAINING_COMPLETE, (data: AITrainingCompleteEventData) => {
        // Remove from active
        const updated = new Map(this._active);
        updated.delete(data.personaId);
        this._active = updated;

        // Add to recent
        this._recent = [{
          personaName: data.personaName,
          domain: data.domain,
          finalLoss: data.finalLoss,
          trainingTime: data.trainingTime,
          examplesProcessed: data.examplesProcessed,
          completedAt: data.timestamp,
        }, ...this._recent].slice(0, MAX_RECENT);
      }),

      Events.subscribe(AI_LEARNING_EVENTS.TRAINING_ERROR, (data: AITrainingErrorEventData) => {
        const updated = new Map(this._active);
        updated.delete(data.personaId);
        this._active = updated;

        this._recent = [{
          personaName: data.personaName,
          domain: data.domain,
          finalLoss: 0,
          trainingTime: 0,
          examplesProcessed: 0,
          completedAt: data.timestamp,
          error: data.error,
        }, ...this._recent].slice(0, MAX_RECENT);
      }),
    );
  }

  override disconnectedCallback(): void {
    super.disconnectedCallback();
    this._cleanups.forEach(fn => fn());
    this._cleanups = [];
  }

  protected override renderContent(): TemplateResult {
    const activeList = [...this._active.values()];

    if (activeList.length === 0 && this._recent.length === 0) {
      return html`<div class="idle-state">No active training. Start a session from the Academy.</div>`;
    }

    return html`
      ${activeList.map(t => this._renderActive(t))}
      ${this._recent.length > 0 ? html`
        <div class="recent-label">Recent</div>
        ${this._recent.map(c => this._renderCompletion(c))}
      ` : nothing}
    `;
  }

  private _renderActive(t: ActiveTraining): TemplateResult {
    const elapsed = Math.round((Date.now() - t.startedAt) / 1000);
    return html`
      <div class="active-training">
        <div class="training-header">
          <span class="training-domain">${t.domain}</span>
          <span class="training-persona">${t.personaName}</span>
        </div>
        <div class="progress-bar-wrapper">
          <div class="progress-bar" style="width: ${t.progress}%"></div>
        </div>
        <div class="training-stats">
          <span class="stat-highlight">${Math.round(t.progress)}%</span>
          ${t.currentLoss != null ? html`<span>Loss: ${t.currentLoss.toFixed(3)}</span>` : nothing}
          ${t.currentEpoch != null ? html`<span>Epoch ${t.currentEpoch}/${t.totalEpochs ?? '?'}</span>` : nothing}
          <span>${elapsed}s</span>
          <span>${t.exampleCount} examples</span>
        </div>
      </div>
    `;
  }

  private _renderCompletion(c: RecentCompletion): TemplateResult {
    const timeStr = `${(c.trainingTime / 1000).toFixed(0)}s`;
    return html`
      <div class="completion-row">
        <span class="completion-domain">${c.domain}</span>
        ${c.error
          ? html`<span class="completion-error">${c.error}</span>`
          : html`
            <span class="completion-loss">${c.finalLoss.toFixed(3)}</span>
            <span class="completion-time">${timeStr} / ${c.examplesProcessed}ex</span>
          `
        }
      </div>
    `;
  }
}

if (typeof customElements !== 'undefined' && !customElements.get('training-status-section')) {
  customElements.define('training-status-section', TrainingStatusSection);
}
