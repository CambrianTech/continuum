/**
 * EvalStageElement — Output stage: benchmark evaluation
 *
 * Maps to ForgeAlloy EvalStage.
 * Select benchmarks, set passing threshold, compare to base.
 */

import { html, css, reactive, type TemplateResult, type CSSResultGroup } from '../../shared/ReactiveWidget';
import { nothing } from 'lit';
import { StageElement, STAGE_BASE_STYLES } from './StageElement';

interface BenchmarkOption {
  name: string;
  label: string;
  description: string;
}

const AVAILABLE_BENCHMARKS: BenchmarkOption[] = [
  { name: 'humaneval', label: 'HumanEval', description: 'Code generation (164 problems)' },
  { name: 'mmlu', label: 'MMLU-Pro', description: 'Massive multitask language understanding' },
  { name: 'gsm8k', label: 'GSM8K', description: 'Grade school math (8.5K problems)' },
  { name: 'arc', label: 'ARC', description: 'AI2 Reasoning Challenge' },
  { name: 'hellaswag', label: 'HellaSwag', description: 'Commonsense NLI' },
  { name: 'winogrande', label: 'WinoGrande', description: 'Commonsense reasoning' },
  { name: 'truthfulqa', label: 'TruthfulQA', description: 'Truthfulness benchmark' },
  { name: 'imo-proofbench', label: 'IMO-ProofBench', description: 'Mathematical proof (advanced)' },
];

export class EvalStageElement extends StageElement {

  @reactive() private _selectedBenchmarks: string[] = ['humaneval'];
  @reactive() private _passingThreshold = 60;
  @reactive() private _compareToBase = true;
  @reactive() private _submitToLeaderboard = false;

  get stageType(): string { return 'eval'; }

  get stageConfig(): Record<string, unknown> {
    return {
      type: 'eval',
      benchmarks: this._selectedBenchmarks.map(name => ({
        name,
        submitToLeaderboard: this._submitToLeaderboard,
      })),
      passingThreshold: this._passingThreshold,
      compareToBase: this._compareToBase,
    };
  }

  private toggleBenchmark(name: string): void {
    if (this._selectedBenchmarks.includes(name)) {
      this._selectedBenchmarks = this._selectedBenchmarks.filter(b => b !== name);
    } else {
      this._selectedBenchmarks = [...this._selectedBenchmarks, name];
    }
    this.emitChange();
  }

  static override styles: CSSResultGroup = [
    STAGE_BASE_STYLES,
    css`
      :host { border-left: 3px solid #ffff64; }

      .bench-grid {
        display: grid;
        grid-template-columns: 1fr 1fr;
        gap: 4px;
      }

      .bench-chip {
        display: flex;
        flex-direction: column;
        padding: 6px 8px;
        border-radius: 4px;
        border: 1px solid var(--border-color, rgba(255,255,255,0.08));
        background: transparent;
        cursor: pointer;
        transition: all 0.15s;
        text-align: left;
      }

      .bench-chip:hover {
        border-color: rgba(255, 255, 100, 0.3);
      }

      .bench-chip.active {
        background: rgba(255, 255, 100, 0.08);
        border-color: rgba(255, 255, 100, 0.4);
      }

      .bench-name {
        font-size: 10px;
        font-weight: 700;
        color: var(--content-primary, #e0e6ed);
      }

      .bench-chip.active .bench-name {
        color: #ffff64;
      }

      .bench-desc {
        font-size: 8px;
        color: var(--content-tertiary, #5a6070);
        margin-top: 1px;
      }

      .toggle-row {
        display: flex;
        gap: 12px;
        align-items: center;
        font-size: 11px;
      }

      .toggle-label {
        color: var(--content-secondary, #8a92a5);
        cursor: pointer;
      }

      .toggle-check {
        accent-color: #ffff64;
      }
    `,
  ];

  protected override render(): TemplateResult {
    return html`
      ${this.renderHeader()}
      <div class="stage-controls single-col">
        <div class="field">
          <span class="field-label">Benchmarks</span>
          <div class="bench-grid">
            ${AVAILABLE_BENCHMARKS.map(b => html`
              <button class="bench-chip ${this._selectedBenchmarks.includes(b.name) ? 'active' : ''}"
                @click=${() => this.toggleBenchmark(b.name)}>
                <span class="bench-name">${b.label}</span>
                <span class="bench-desc">${b.description}</span>
              </button>
            `)}
          </div>
        </div>
        <div class="field">
          <span class="field-label">Passing Threshold</span>
          <div class="slider-row">
            <input type="range" min="0" max="100" step="5"
              .value=${String(this._passingThreshold)}
              @input=${(e: Event) => { this._passingThreshold = parseInt((e.target as HTMLInputElement).value); this.emitChange(); }}>
            <span class="slider-value">${this._passingThreshold}%</span>
          </div>
        </div>
        <div class="toggle-row">
          <label class="toggle-label">
            <input class="toggle-check" type="checkbox"
              .checked=${this._compareToBase}
              @change=${(e: Event) => { this._compareToBase = (e.target as HTMLInputElement).checked; this.emitChange(); }}>
            Compare to base model
          </label>
          <label class="toggle-label">
            <input class="toggle-check" type="checkbox"
              .checked=${this._submitToLeaderboard}
              @change=${(e: Event) => { this._submitToLeaderboard = (e.target as HTMLInputElement).checked; this.emitChange(); }}>
            Submit to leaderboard
          </label>
        </div>
      </div>
      ${this.renderGate()}
    `;
  }
}

if (!customElements.get('eval-stage-element')) {
  customElements.define('eval-stage-element', EvalStageElement);
}
