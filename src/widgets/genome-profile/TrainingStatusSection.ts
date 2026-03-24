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
import { ContentService } from '../../system/state/ContentService';
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

interface AcademySessionInfo {
  id: string;
  skill: string;
  status: string;
  personaName: string;
  baseModel: string;
  mode: string;
  createdAt: string;
  nodeName?: string;
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

  .dashboard-link {
    display: block;
    text-align: center;
    padding: 6px 0;
    margin-top: 8px;
    font-size: 10px;
    font-weight: 600;
    color: rgba(0, 212, 255, 0.8);
    cursor: pointer;
    border-top: 1px solid rgba(60, 80, 100, 0.2);
    transition: color 0.15s ease;
  }

  .dashboard-link:hover {
    color: rgba(0, 212, 255, 1);
  }
`;

export class TrainingStatusSection extends ReactiveWidget {
  static override styles = [unsafeCSS(STYLES)] as CSSResultGroup;

  @reactive() private _active: Map<string, ActiveTraining> = new Map();
  @reactive() private _recent: RecentCompletion[] = [];
  @reactive() private _academySessions: AcademySessionInfo[] = [];

  private _cleanups: (() => void)[] = [];
  private _pollTimer: ReturnType<typeof setInterval> | null = null;

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

    // Load Academy sessions from grid nodes
    this._loadAcademySessions();
    this._pollTimer = setInterval(() => {
      if (this._academySessions.some(s => !['completed', 'failed', 'cancelled'].includes(s.status))) {
        this._loadAcademySessions();
      }
    }, 30_000);
  }

  override disconnectedCallback(): void {
    super.disconnectedCallback();
    this._cleanups.forEach(fn => fn());
    this._cleanups = [];
    if (this._pollTimer) {
      clearInterval(this._pollTimer);
      this._pollTimer = null;
    }
  }

  private async _loadAcademySessions(): Promise<void> {
    // Use the aggregation command — does all grid calls server-side in one roundtrip
    try {
      const result = await this.executeCommand<any, any>('genome/training-overview', {});
      if (result?.sessions) {
        this._academySessions = result.sessions.map((s: Record<string, unknown>) => ({
          id: s.id,
          skill: s.skill,
          status: s.status,
          personaName: s.personaName,
          baseModel: s.baseModel,
          mode: s.mode,
          createdAt: s.createdAt as string,
          nodeName: s.nodeName,
        }));
      }
    } catch (err) {
      console.warn('[TrainingStatusSection] Failed to load academy sessions:', err);
    }
  }

  protected override renderContent(): TemplateResult {
    const activeList = [...this._active.values()];
    const activeSessions = this._academySessions.filter(s => !['completed', 'failed', 'cancelled'].includes(s.status));
    const hasAnything = activeList.length > 0 || this._recent.length > 0 || activeSessions.length > 0;

    if (!hasAnything) {
      return html`
        <div class="idle-state">No active training. Start a session from the Academy.</div>
        <div class="dashboard-link" @click=${this._openDashboard}>View Training Dashboard →</div>
      `;
    }

    return html`
      ${activeSessions.map(s => this._renderAcademySession(s))}
      ${activeList.map(t => this._renderActive(t))}
      ${this._recent.length > 0 ? html`
        <div class="recent-label">Recent</div>
        ${this._recent.map(c => this._renderCompletion(c))}
      ` : nothing}
      <div class="dashboard-link" @click=${this._openDashboard}>View Training Dashboard →</div>
    `;
  }

  private _renderAcademySession(s: AcademySessionInfo): TemplateResult {
    const elapsed = Math.round((Date.now() - new Date(s.createdAt).getTime()) / 1000);
    const elapsedStr = elapsed > 3600 ? `${(elapsed / 3600).toFixed(1)}h` : elapsed > 60 ? `${Math.round(elapsed / 60)}m` : `${elapsed}s`;

    return html`
      <div class="active-training" style="border-color: rgba(0, 212, 255, 0.15);">
        <div class="training-header">
          <span class="training-domain">${s.skill}</span>
          <span class="training-persona">${s.personaName}${s.nodeName ? ` @ ${s.nodeName}` : ''}</span>
        </div>
        <div class="training-stats">
          <span class="stat-highlight">${s.status}</span>
          <span>${s.mode}</span>
          <span>${elapsedStr}</span>
        </div>
      </div>
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

  private _openDashboard(): void {
    ContentService.open('training-dashboard', undefined, { title: 'Training' });
  }
}

if (typeof customElements !== 'undefined' && !customElements.get('training-status-section')) {
  customElements.define('training-status-section', TrainingStatusSection);
}
