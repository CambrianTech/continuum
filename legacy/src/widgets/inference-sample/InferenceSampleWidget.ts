/**
 * InferenceSampleWidget — Quality monitoring during and after training
 *
 * Features:
 *   - Adapter selector dropdown (from genome/adapter-list)
 *   - Prompt template editor with presets
 *   - "Generate Sample" button → inference/generate with selected adapters
 *   - Auto-sample mode: generate every N steps during training
 *   - Side-by-side: base model output vs adapter-enhanced output
 *   - Sample history with timestamps and adapter versions
 */

import {
  ReactiveWidget,
  html,
  reactive,
  css,
  type TemplateResult,
  type CSSResultGroup,
} from '../shared/ReactiveWidget';
import { nothing } from 'lit';
import { Events } from '../../system/core/shared/Events';
import {
  AI_LEARNING_EVENTS,
  type AITrainingStepEventData,
  type AITrainingCompleteEventData,
} from '../../system/events/shared/AILearningEvents';

// ── Types ───────────────────────────────────────────────────────────────────

interface AdapterOption {
  name: string;
  domain: string;
  personaName: string;
  isActive: boolean;
  loss?: number;
}

interface SampleResult {
  id: string;
  timestamp: number;
  prompt: string;
  baseOutput: string;
  adapterOutput: string;
  adaptersUsed: string[];
  model: string;
  provider: string;
  baseResponseMs: number;
  adapterResponseMs: number;
  step?: number;
}

const PROMPT_PRESETS: { label: string; prompt: string; system?: string }[] = [
  { label: 'Greeting', prompt: 'Hello! How are you doing today?', system: 'You are a helpful assistant.' },
  { label: 'Code', prompt: 'Write a function to calculate fibonacci numbers in Python.', system: 'You are an expert programmer.' },
  { label: 'Creative', prompt: 'Tell me a short story about a robot learning to paint.', system: 'You are a creative storyteller.' },
  { label: 'Explain', prompt: 'Explain how neural networks learn, in simple terms.', system: 'You are a patient teacher.' },
  { label: 'Conversation', prompt: 'What do you think about the future of local AI models?', system: 'You are a thoughtful conversationalist who has strong opinions.' },
];

const MAX_HISTORY = 20;

// ── Component ───────────────────────────────────────────────────────────────

export class InferenceSampleWidget extends ReactiveWidget {
  static override styles = [
    ReactiveWidget.styles,
    css`
      :host {
        display: block;
        width: 100%;
        height: 100%;
        overflow-y: auto;
        color: var(--content-primary, #e0e6ed);
        font-family: var(--font-primary, sans-serif);
      }

      .sample-dashboard {
        padding: 16px 20px;
        max-width: 1000px;
        margin: 0 auto;
      }

      .dashboard-header {
        margin-bottom: 20px;
      }

      .dashboard-title {
        font-size: 18px;
        font-weight: 700;
      }

      .dashboard-subtitle {
        font-size: 11px;
        color: var(--content-secondary, #8a92a5);
        margin-top: 2px;
      }

      .section-label {
        font-size: 10px;
        font-weight: 700;
        color: var(--content-secondary, #8a92a5);
        text-transform: uppercase;
        letter-spacing: 0.8px;
        margin-bottom: 8px;
        margin-top: 20px;
      }

      /* Controls */
      .controls {
        background: rgba(15, 20, 25, 0.6);
        border: 1px solid var(--border-subtle, rgba(255, 255, 255, 0.1));
        border-radius: 6px;
        padding: 14px;
        margin-bottom: 20px;
      }

      .control-row {
        display: flex;
        gap: 12px;
        align-items: flex-end;
        margin-bottom: 10px;
        flex-wrap: wrap;
      }

      .control-group {
        display: flex;
        flex-direction: column;
        gap: 4px;
      }

      .control-group.flex {
        flex: 1;
        min-width: 200px;
      }

      .control-label {
        font-size: 10px;
        font-weight: 600;
        color: var(--content-secondary, #8a92a5);
        text-transform: uppercase;
        letter-spacing: 0.3px;
      }

      select, textarea {
        background: var(--input-background, rgba(40, 45, 55, 0.8));
        border: 1px solid var(--input-border, rgba(255, 255, 255, 0.15));
        border-radius: 4px;
        color: var(--input-text, #fff);
        font-family: var(--font-mono, monospace);
        font-size: 11px;
        padding: 6px 8px;
        outline: none;
      }

      select:focus, textarea:focus {
        border-color: var(--input-border-focus, rgba(0, 212, 255, 0.5));
        box-shadow: 0 0 0 2px var(--input-focus-shadow, rgba(0, 212, 255, 0.2));
      }

      textarea {
        min-height: 60px;
        resize: vertical;
        width: 100%;
        box-sizing: border-box;
      }

      .preset-row {
        display: flex;
        gap: 4px;
        flex-wrap: wrap;
        margin-bottom: 8px;
      }

      .preset-btn {
        padding: 3px 8px;
        font-size: 10px;
        border-radius: 3px;
        border: 1px solid rgba(255, 255, 255, 0.1);
        background: rgba(255, 255, 255, 0.04);
        color: var(--content-secondary, #8a92a5);
        cursor: pointer;
        transition: all 0.15s ease;
      }

      .preset-btn:hover {
        background: rgba(0, 212, 255, 0.1);
        border-color: rgba(0, 212, 255, 0.3);
        color: rgba(0, 212, 255, 0.9);
      }

      .preset-btn.active {
        background: rgba(0, 212, 255, 0.12);
        border-color: rgba(0, 212, 255, 0.4);
        color: rgba(0, 212, 255, 0.9);
      }

      .action-row {
        display: flex;
        gap: 8px;
        align-items: center;
      }

      .generate-btn {
        padding: 6px 16px;
        font-size: 11px;
        font-weight: 700;
        border-radius: 4px;
        border: none;
        background: linear-gradient(135deg, rgba(0, 212, 255, 0.8), rgba(0, 180, 230, 0.8));
        color: #000;
        cursor: pointer;
        transition: all 0.15s ease;
      }

      .generate-btn:hover {
        background: linear-gradient(135deg, rgba(0, 212, 255, 0.9), rgba(0, 180, 230, 0.9));
      }

      .generate-btn:disabled {
        opacity: 0.5;
        cursor: not-allowed;
      }

      .auto-sample-toggle {
        display: flex;
        align-items: center;
        gap: 6px;
        font-size: 10px;
        color: var(--content-secondary, #8a92a5);
      }

      .auto-sample-toggle input[type="checkbox"] {
        accent-color: rgba(0, 212, 255, 0.8);
      }

      /* Side-by-side comparison */
      .comparison {
        display: grid;
        grid-template-columns: 1fr 1fr;
        gap: 12px;
        margin-bottom: 16px;
      }

      @media (max-width: 600px) {
        .comparison {
          grid-template-columns: 1fr;
        }
      }

      .output-panel {
        background: rgba(15, 20, 25, 0.6);
        border: 1px solid var(--border-subtle, rgba(255, 255, 255, 0.1));
        border-radius: 6px;
        padding: 12px;
      }

      .output-label {
        font-size: 10px;
        font-weight: 700;
        text-transform: uppercase;
        letter-spacing: 0.5px;
        margin-bottom: 6px;
      }

      .output-label.base {
        color: var(--content-secondary, #8a92a5);
      }

      .output-label.adapter {
        color: rgba(0, 255, 200, 0.9);
      }

      .output-meta {
        font-size: 9px;
        color: var(--content-secondary, #8a92a5);
        font-family: var(--font-mono, monospace);
        margin-bottom: 6px;
      }

      .output-text {
        font-size: 12px;
        line-height: 1.5;
        white-space: pre-wrap;
        word-break: break-word;
        color: var(--content-primary, #e0e6ed);
        max-height: 200px;
        overflow-y: auto;
      }

      .generating-indicator {
        display: flex;
        align-items: center;
        gap: 8px;
        padding: 12px;
        color: rgba(0, 212, 255, 0.8);
        font-size: 12px;
      }

      /* History */
      .history-list {
        display: flex;
        flex-direction: column;
        gap: 8px;
      }

      .history-item {
        background: rgba(15, 20, 25, 0.4);
        border: 1px solid rgba(255, 255, 255, 0.06);
        border-radius: 4px;
        padding: 8px 10px;
        cursor: pointer;
        transition: border-color 0.15s ease;
      }

      .history-item:hover {
        border-color: rgba(0, 212, 255, 0.3);
      }

      .history-item.selected {
        border-color: rgba(0, 212, 255, 0.5);
      }

      .history-meta {
        display: flex;
        justify-content: space-between;
        align-items: center;
        font-size: 10px;
        color: var(--content-secondary, #8a92a5);
        margin-bottom: 3px;
      }

      .history-prompt {
        font-size: 11px;
        color: var(--content-primary, #e0e6ed);
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
      }

      .history-adapters {
        font-size: 9px;
        color: rgba(0, 255, 200, 0.7);
        font-family: var(--font-mono, monospace);
      }
    `,
  ] as CSSResultGroup;

  // ── State ───────────────────────────────────────────────────────────────

  @reactive() private _adapters: AdapterOption[] = [];
  @reactive() private _selectedAdapters: Set<string> = new Set();
  @reactive() private _prompt: string = PROMPT_PRESETS[0].prompt;
  @reactive() private _systemPrompt: string = PROMPT_PRESETS[0].system ?? '';
  @reactive() private _generating: boolean = false;
  @reactive() private _currentSample: SampleResult | null = null;
  @reactive() private _history: SampleResult[] = [];
  @reactive() private _autoSample: boolean = false;
  @reactive() private _autoSampleInterval: number = 50;  // Every N steps
  @reactive() private _selectedPreset: number = 0;

  private _cleanups: (() => void)[] = [];
  private _lastAutoSampleStep = 0;
  private _sampleCounter = 0;

  constructor() {
    super({ widgetName: 'InferenceSampleWidget' });
  }

  // ── Lifecycle ───────────────────────────────────────────────────────────

  protected override onFirstRender(): void {
    super.onFirstRender();

    // Auto-sample during training
    this._cleanups.push(
      Events.subscribe(AI_LEARNING_EVENTS.TRAINING_STEP, (data: AITrainingStepEventData) => {
        if (this._autoSample && !this._generating) {
          if (data.step - this._lastAutoSampleStep >= this._autoSampleInterval) {
            this._lastAutoSampleStep = data.step;
            this._generateSample(data.step);
          }
        }
      }),

      Events.subscribe(AI_LEARNING_EVENTS.TRAINING_COMPLETE, (_data: AITrainingCompleteEventData) => {
        // Final sample on training completion
        if (this._autoSample && !this._generating) {
          this._generateSample();
        }
      }),
    );

    this._loadAdapters();
  }

  override disconnectedCallback(): void {
    super.disconnectedCallback();
    this._cleanups.forEach(fn => fn());
    this._cleanups = [];
  }

  // ── Data loading ────────────────────────────────────────────────────────

  private async _loadAdapters(): Promise<void> {
    try {
      const result = await this.executeCommand<any, any>('genome/layers', {
        includeMetrics: true,
      });
      if (result.success && result.adapters) {
        this._adapters = result.adapters.map((a: any) => ({
          name: a.name,
          domain: a.domain,
          personaName: a.personaName,
          isActive: a.isActive,
          loss: a.loss,
        }));
      }
    } catch (err) {
      console.warn('[InferenceSample] Failed to load adapters:', err);
    }
  }

  // ── Generate sample ─────────────────────────────────────────────────────

  private async _generateSample(step?: number): Promise<void> {
    if (this._generating) return;
    this._generating = true;

    const adaptersToUse = [...this._selectedAdapters];
    const prompt = this._prompt;
    const systemPrompt = this._systemPrompt;

    try {
      // 1. Generate with base model (no adapters)
      const baseResult = await this.executeCommand<any, any>('inference/generate', {
        prompt,
        systemPrompt,
        maxTokens: 256,
        temperature: 0.7,
        adapters: [],
      });

      // 2. Generate with selected adapters
      const adapterResult = adaptersToUse.length > 0
        ? await this.executeCommand<any, any>('inference/generate', {
            prompt,
            systemPrompt,
            maxTokens: 256,
            temperature: 0.7,
            adapters: adaptersToUse,
          })
        : baseResult; // If no adapters selected, show same output

      const sample: SampleResult = {
        id: `sample-${++this._sampleCounter}`,
        timestamp: Date.now(),
        prompt,
        baseOutput: baseResult.success ? baseResult.text : `Error: ${baseResult.error}`,
        adapterOutput: adapterResult.success ? adapterResult.text : `Error: ${adapterResult.error}`,
        adaptersUsed: adapterResult.success ? (adapterResult.adaptersApplied ?? adaptersToUse) : adaptersToUse,
        model: baseResult.model ?? 'unknown',
        provider: baseResult.provider ?? 'unknown',
        baseResponseMs: baseResult.responseTimeMs ?? 0,
        adapterResponseMs: adapterResult.responseTimeMs ?? 0,
        step,
      };

      this._currentSample = sample;
      this._history = [sample, ...this._history].slice(0, MAX_HISTORY);
    } catch (err) {
      console.error('[InferenceSample] Generation failed:', err);
    } finally {
      this._generating = false;
    }
  }

  // ── Render ──────────────────────────────────────────────────────────────

  protected override renderContent(): TemplateResult {
    return html`
      <div class="sample-dashboard">
        <div class="dashboard-header">
          <div class="dashboard-title">Inference Samples</div>
          <div class="dashboard-subtitle">
            Compare base model vs adapter-enhanced output
          </div>
        </div>

        ${this._renderControls()}
        ${this._generating ? this._renderGenerating() : nothing}
        ${this._currentSample ? this._renderComparison(this._currentSample) : nothing}
        ${this._history.length > 1 ? this._renderHistory() : nothing}
      </div>
    `;
  }

  // ── Controls ────────────────────────────────────────────────────────────

  private _renderControls(): TemplateResult {
    return html`
      <div class="controls">
        <div class="control-row">
          <div class="control-group">
            <span class="control-label">Adapters</span>
            <select multiple
              @change=${(e: Event) => this._onAdapterSelect(e)}
              style="min-height: 60px; min-width: 180px;">
              ${this._adapters.map(a => html`
                <option value="${a.name}" ?selected=${this._selectedAdapters.has(a.name)}>
                  ${a.name} (${a.domain})${a.loss != null ? ` — loss ${a.loss.toFixed(3)}` : ''}
                </option>
              `)}
            </select>
          </div>

          <div class="control-group flex">
            <span class="control-label">Presets</span>
            <div class="preset-row">
              ${PROMPT_PRESETS.map((p, i) => html`
                <button class="preset-btn ${this._selectedPreset === i ? 'active' : ''}"
                  @click=${() => this._selectPreset(i)}>
                  ${p.label}
                </button>
              `)}
            </div>
            <textarea
              .value=${this._prompt}
              @input=${(e: Event) => { this._prompt = (e.target as HTMLTextAreaElement).value; }}
              placeholder="Enter prompt..."
            ></textarea>
          </div>
        </div>

        <div class="action-row">
          <button class="generate-btn"
            ?disabled=${this._generating}
            @click=${() => this._generateSample()}>
            ${this._generating ? 'Generating...' : 'Generate Sample'}
          </button>

          <button class="generate-btn" style="background: rgba(255, 255, 255, 0.1); color: var(--content-primary);"
            @click=${() => this._loadAdapters()}>
            Refresh Adapters
          </button>

          <label class="auto-sample-toggle">
            <input type="checkbox"
              .checked=${this._autoSample}
              @change=${(e: Event) => { this._autoSample = (e.target as HTMLInputElement).checked; }}>
            Auto-sample every
            <select style="width: 60px;"
              @change=${(e: Event) => { this._autoSampleInterval = parseInt((e.target as HTMLSelectElement).value); }}>
              <option value="25" ?selected=${this._autoSampleInterval === 25}>25</option>
              <option value="50" ?selected=${this._autoSampleInterval === 50}>50</option>
              <option value="100" ?selected=${this._autoSampleInterval === 100}>100</option>
            </select>
            steps
          </label>
        </div>
      </div>
    `;
  }

  // ── Generating indicator ────────────────────────────────────────────────

  private _renderGenerating(): TemplateResult {
    return html`
      <div class="generating-indicator">
        <div class="spinner" style="width: 16px; height: 16px;"></div>
        Generating samples (base + adapter)...
      </div>
    `;
  }

  // ── Comparison ──────────────────────────────────────────────────────────

  private _renderComparison(sample: SampleResult): TemplateResult {
    return html`
      <div class="section-label">
        Latest Sample${sample.step != null ? ` (Step ${sample.step})` : ''}
      </div>
      <div class="comparison">
        <div class="output-panel">
          <div class="output-label base">Base Model</div>
          <div class="output-meta">${sample.model} / ${sample.provider} / ${sample.baseResponseMs}ms</div>
          <div class="output-text">${sample.baseOutput}</div>
        </div>
        <div class="output-panel" style="border-color: rgba(0, 255, 200, 0.15);">
          <div class="output-label adapter">
            With Adapters: ${sample.adaptersUsed.length > 0 ? sample.adaptersUsed.join(', ') : 'none'}
          </div>
          <div class="output-meta">${sample.model} / ${sample.provider} / ${sample.adapterResponseMs}ms</div>
          <div class="output-text">${sample.adapterOutput}</div>
        </div>
      </div>
    `;
  }

  // ── History ─────────────────────────────────────────────────────────────

  private _renderHistory(): TemplateResult {
    return html`
      <div class="section-label">Sample History</div>
      <div class="history-list">
        ${this._history.map(s => html`
          <div class="history-item ${this._currentSample?.id === s.id ? 'selected' : ''}"
            @click=${() => { this._currentSample = s; }}>
            <div class="history-meta">
              <span>${new Date(s.timestamp).toLocaleTimeString()}</span>
              ${s.step != null ? html`<span>Step ${s.step}</span>` : nothing}
              <span>${s.baseResponseMs}ms / ${s.adapterResponseMs}ms</span>
            </div>
            <div class="history-prompt">${s.prompt}</div>
            ${s.adaptersUsed.length > 0 ? html`
              <div class="history-adapters">${s.adaptersUsed.join(', ')}</div>
            ` : nothing}
          </div>
        `)}
      </div>
    `;
  }

  // ── Event handlers ──────────────────────────────────────────────────────

  private _onAdapterSelect(e: Event): void {
    const select = e.target as HTMLSelectElement;
    const selected = new Set<string>();
    for (let i = 0; i < select.selectedOptions.length; i++) {
      selected.add(select.selectedOptions[i].value);
    }
    this._selectedAdapters = selected;
  }

  private _selectPreset(index: number): void {
    this._selectedPreset = index;
    this._prompt = PROMPT_PRESETS[index].prompt;
    this._systemPrompt = PROMPT_PRESETS[index].system ?? '';
  }
}

// ── Register ────────────────────────────────────────────────────────────────

if (typeof customElements !== 'undefined' && !customElements.get('inference-sample-widget')) {
  customElements.define('inference-sample-widget', InferenceSampleWidget);
}
