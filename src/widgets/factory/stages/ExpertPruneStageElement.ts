/**
 * ExpertPruneStageElement — UI for the alloy 'expert-prune' stage
 *
 * MoE expert selection: keep the best N experts, remove the rest.
 * Controls: keep count, selection strategy, profiling config
 * Maps 1:1 to ForgeAlloy ExpertPruneStage schema.
 */

import { html, css, reactive, type TemplateResult, type CSSResultGroup } from '../../shared/ReactiveWidget';
import { StageElement, STAGE_BASE_STYLES } from './StageElement';

export class ExpertPruneStageElement extends StageElement {

  @reactive() private _keepExperts = 8;
  @reactive() private _selectionStrategy: 'activation' | 'gradient' | 'random' = 'activation';
  @reactive() private _profileDataset = '';
  @reactive() private _profileSteps = 100;

  get stageType(): string { return 'expert-prune'; }

  get stageConfig(): Record<string, unknown> {
    return {
      type: 'expert-prune',
      keepExperts: this._keepExperts,
      selectionStrategy: this._selectionStrategy,
      ...(this._profileDataset ? { profileDataset: this._profileDataset } : {}),
      profileSteps: this._profileSteps,
    };
  }

  override validate(): string[] {
    const errors: string[] = [];
    if (this._keepExperts < 1) errors.push('Must keep at least 1 expert');
    if (this._profileSteps < 1) errors.push('Profile steps must be at least 1');
    return errors;
  }

  static override styles: CSSResultGroup = [
    STAGE_BASE_STYLES,
    css`
      :host { border-left: 3px solid #ff9664; }

      .expert-info {
        font-size: 10px;
        color: var(--content-secondary, #8a92a5);
        background: rgba(255, 150, 100, 0.08);
        border: 1px solid rgba(255, 150, 100, 0.15);
        border-radius: 4px;
        padding: 6px 10px;
        margin-bottom: 8px;
      }
    `,
  ];

  protected override render(): TemplateResult {
    return html`
      ${this.renderHeader()}
      <div class="expert-info">
        MoE models (e.g. Qwen3.5-35B-A3B) have many experts per layer.
        Select the top-performing experts for your domain and discard the rest.
      </div>
      <div class="stage-controls">
        <div class="field">
          <span class="field-label">Keep Experts</span>
          <div class="slider-row">
            <input type="range" min="1" max="64" step="1"
              .value=${String(this._keepExperts)}
              @input=${(e: Event) => { this._keepExperts = parseInt((e.target as HTMLInputElement).value); this.emitChange(); }}>
            <span class="slider-value">${this._keepExperts}</span>
          </div>
        </div>
        <div class="field">
          <span class="field-label">Selection Strategy</span>
          <select class="field-select"
            .value=${this._selectionStrategy}
            @change=${(e: Event) => { this._selectionStrategy = (e.target as HTMLSelectElement).value as typeof this._selectionStrategy; this.emitChange(); }}>
            <option value="activation">Activation (recommended)</option>
            <option value="gradient">Gradient</option>
            <option value="random">Random</option>
          </select>
        </div>
        <div class="field">
          <span class="field-label">Profile Steps</span>
          <div class="slider-row">
            <input type="range" min="10" max="500" step="10"
              .value=${String(this._profileSteps)}
              @input=${(e: Event) => { this._profileSteps = parseInt((e.target as HTMLInputElement).value); this.emitChange(); }}>
            <span class="slider-value">${this._profileSteps}</span>
          </div>
          <span class="field-hint">Steps to profile expert utilization before selecting</span>
        </div>
        <div class="field">
          <span class="field-label">Profile Dataset</span>
          <input class="field-input" type="text" placeholder="Auto (domain-matched)"
            .value=${this._profileDataset}
            @change=${(e: Event) => { this._profileDataset = (e.target as HTMLInputElement).value; this.emitChange(); }}>
        </div>
      </div>
      ${this.renderGate()}
    `;
  }
}

if (!customElements.get('expert-prune-stage-element')) {
  customElements.define('expert-prune-stage-element', ExpertPruneStageElement);
}
