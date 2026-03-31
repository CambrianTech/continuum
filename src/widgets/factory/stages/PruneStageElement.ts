/**
 * PruneStageElement — UI for the alloy 'prune' stage
 *
 * Controls: strategy, level (0-90%), min heads, min KV heads, analysis steps
 * Maps 1:1 to ForgeAlloy PruneStage schema.
 */

import { html, css, reactive, type TemplateResult, type CSSResultGroup } from '../../shared/ReactiveWidget';
import { StageElement, STAGE_BASE_STYLES } from './StageElement';

export class PruneStageElement extends StageElement {

  @reactive() private _strategy: 'entropy' | 'magnitude' | 'gradient' | 'random' = 'entropy';
  @reactive() private _level = 30; // percentage (0-90)
  @reactive() private _minHeads = 4;
  @reactive() private _minKvHeads = 2;
  @reactive() private _analysisSteps = 200;

  get stageType(): string { return 'prune'; }

  get stageConfig(): Record<string, unknown> {
    return {
      type: 'prune',
      strategy: this._strategy,
      level: this._level / 100,
      minHeadsPerLayer: this._minHeads,
      minKvHeadsPerLayer: this._minKvHeads,
      analysisSteps: this._analysisSteps,
    };
  }

  override validate(): string[] {
    const errors: string[] = [];
    if (this._level < 0 || this._level > 90) errors.push('Prune level must be 0-90%');
    if (this._minHeads < 1) errors.push('Min heads must be at least 1');
    return errors;
  }

  static override styles: CSSResultGroup = [
    STAGE_BASE_STYLES,
    css`
      :host {
        border-left: 3px solid #ff6464;
      }
    `,
  ];

  protected override render(): TemplateResult {
    return html`
      ${this.renderHeader()}
      <div class="stage-controls">
        <div class="field">
          <span class="field-label">Strategy</span>
          <select class="field-select"
            .value=${this._strategy}
            @change=${(e: Event) => { this._strategy = (e.target as HTMLSelectElement).value as typeof this._strategy; this.emitChange(); }}>
            <option value="entropy">Entropy (recommended)</option>
            <option value="magnitude">Magnitude</option>
            <option value="gradient">Gradient</option>
            <option value="random">Random</option>
          </select>
        </div>
        <div class="field">
          <span class="field-label">Prune Level</span>
          <div class="slider-row">
            <input type="range" min="0" max="90" step="5"
              .value=${String(this._level)}
              @input=${(e: Event) => { this._level = parseInt((e.target as HTMLInputElement).value); this.emitChange(); }}>
            <span class="slider-value">${this._level}%</span>
          </div>
        </div>
        <div class="field">
          <span class="field-label">Min Heads/Layer</span>
          <input class="field-input" type="number" min="1" max="32"
            .value=${String(this._minHeads)}
            @change=${(e: Event) => { this._minHeads = parseInt((e.target as HTMLInputElement).value); this.emitChange(); }}>
        </div>
        <div class="field">
          <span class="field-label">Analysis Steps</span>
          <input class="field-input" type="number" min="10" max="1000" step="10"
            .value=${String(this._analysisSteps)}
            @change=${(e: Event) => { this._analysisSteps = parseInt((e.target as HTMLInputElement).value); this.emitChange(); }}>
        </div>
      </div>
    `;
  }
}

if (!customElements.get('prune-stage-element')) {
  customElements.define('prune-stage-element', PruneStageElement);
}
