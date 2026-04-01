/**
 * DeployStageElement — Output stage: deploy to grid or endpoint
 *
 * Maps to ForgeAlloy DeployStage.
 * Target node, health check, warmup, auto-scale.
 */

import { html, css, reactive, type TemplateResult, type CSSResultGroup } from '../../shared/ReactiveWidget';
import { StageElement, STAGE_BASE_STYLES } from './StageElement';

export class DeployStageElement extends StageElement {

  @reactive() private _target = 'bigmama';
  @reactive() private _healthCheck = true;
  @reactive() private _warmup = true;
  @reactive() private _maxConcurrency = 4;
  @reactive() private _autoScale = false;

  get stageType(): string { return 'deploy'; }

  get stageConfig(): Record<string, unknown> {
    return {
      type: 'deploy',
      target: this._target,
      healthCheck: this._healthCheck,
      warmup: this._warmup,
      maxConcurrency: this._maxConcurrency,
      autoScale: this._autoScale,
    };
  }

  static override styles: CSSResultGroup = [
    STAGE_BASE_STYLES,
    css`
      :host { border-left: 3px solid #64ffc8; }

      .toggle-row {
        display: flex;
        gap: 12px;
        align-items: center;
        font-size: 11px;
        margin-top: 4px;
      }

      .toggle-label {
        color: var(--content-secondary, #8a92a5);
        cursor: pointer;
      }

      .toggle-check {
        accent-color: #64ffc8;
      }
    `,
  ];

  protected override render(): TemplateResult {
    return html`
      ${this.renderHeader()}
      <div class="stage-controls">
        <div class="field">
          <span class="field-label">Target</span>
          <select class="field-select"
            .value=${this._target}
            @change=${(e: Event) => { this._target = (e.target as HTMLSelectElement).value; this.emitChange(); }}>
            <option value="bigmama">BigMama (RTX 5090)</option>
            <option value="local">Local (this machine)</option>
            <option value="grid">Grid (auto-select best node)</option>
          </select>
        </div>
        <div class="field">
          <span class="field-label">Max Concurrency</span>
          <div class="slider-row">
            <input type="range" min="1" max="16" step="1"
              .value=${String(this._maxConcurrency)}
              @input=${(e: Event) => { this._maxConcurrency = parseInt((e.target as HTMLInputElement).value); this.emitChange(); }}>
            <span class="slider-value">${this._maxConcurrency}</span>
          </div>
        </div>
      </div>
      <div class="toggle-row">
        <label class="toggle-label">
          <input class="toggle-check" type="checkbox"
            .checked=${this._healthCheck}
            @change=${(e: Event) => { this._healthCheck = (e.target as HTMLInputElement).checked; this.emitChange(); }}>
          Health check
        </label>
        <label class="toggle-label">
          <input class="toggle-check" type="checkbox"
            .checked=${this._warmup}
            @change=${(e: Event) => { this._warmup = (e.target as HTMLInputElement).checked; this.emitChange(); }}>
          Warmup inference
        </label>
        <label class="toggle-label">
          <input class="toggle-check" type="checkbox"
            .checked=${this._autoScale}
            @change=${(e: Event) => { this._autoScale = (e.target as HTMLInputElement).checked; this.emitChange(); }}>
          Auto-scale
        </label>
      </div>
      ${this.renderGate()}
    `;
  }
}

if (!customElements.get('deploy-stage-element')) {
  customElements.define('deploy-stage-element', DeployStageElement);
}
