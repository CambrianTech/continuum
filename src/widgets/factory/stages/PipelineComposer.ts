/**
 * PipelineComposer — Visual alloy pipeline editor
 *
 * Renders a sequence of StageElements from an alloy recipe.
 * Users can add, remove, and reorder stages.
 * Emits the complete pipeline config matching the alloy stages array.
 *
 * The composer doesn't know what stage types exist — it discovers them
 * from the STAGE_REGISTRY. Add a new stage type → register it → done.
 */

import {
  ReactiveWidget,
  html,
  css,
  reactive,
  type TemplateResult,
  type CSSResultGroup,
} from '../../shared/ReactiveWidget';
import { nothing } from 'lit';
import { STAGE_COLORS, STAGE_TEXT_COLORS } from './StageElement';

// Import stage elements (self-registering)
import './SourceConfigStageElement';
import './ContextExtendStageElement';
import './ModalityStageElement';
import './PruneStageElement';
import './TrainStageElement';
import './LoraStageElement';
import './CompactStageElement';
import './ExpertPruneStageElement';
import './QuantStageElement';
import './EvalStageElement';
import './PublishStageElement';
import './DeployStageElement';

/** Source model metadata — determines which stages are available */
interface SourceModelInfo {
  isMoE: boolean;
  hasVision: boolean;
  hasAudio: boolean;
  architecture: string;
}

/** Registry of available stage types → custom element tags.
 *  Organized by pipeline position: input → transform → output.
 *  Add a new alloy stage type → register it here → the composer discovers it.
 *
 *  `requires`: optional function that gates visibility based on source model.
 *  If omitted, stage is always available. */
const STAGE_REGISTRY: Record<string, {
  tag: string;
  label: string;
  description: string;
  position: 'input' | 'transform' | 'output';
  requires?: (model: SourceModelInfo) => boolean;
}> = {
  // Input stages (front bookend)
  'source-config':  { tag: 'source-config-stage-element',  label: 'Source Config',  description: 'Context window, modalities, target devices',      position: 'input' },
  'context-extend': { tag: 'context-extend-stage-element', label: 'Context Extend', description: 'RoPE rescaling (YaRN, NTK) — extend to 128K+',   position: 'input' },
  'modality':       { tag: 'modality-stage-element',       label: 'Modality',       description: 'Bolt vision, audio, or video onto a text model',  position: 'input',
                      requires: (m) => !m.hasVision },  // Only show for text-only models
  // Transform stages
  'prune':          { tag: 'prune-stage-element',          label: 'Prune',          description: 'Head pruning (entropy, magnitude, gradient)',      position: 'transform' },
  'train':          { tag: 'train-stage-element',          label: 'Train',          description: 'Recovery/fine-tuning with full config',            position: 'transform' },
  'lora':           { tag: 'lora-stage-element',           label: 'LoRA',           description: 'Low-rank adaptation with QLoRA support',           position: 'transform' },
  'compact':        { tag: 'compact-stage-element',        label: 'Compact',        description: 'Utilization-aware mixed-precision compaction',     position: 'transform' },
  'expert-prune':   { tag: 'expert-prune-stage-element',   label: 'Expert Prune',   description: 'MoE expert selection by activation',               position: 'transform',
                      requires: (m) => m.isMoE },  // Only show for MoE models
  // Output stages (end bookend)
  'quant':          { tag: 'quant-stage-element',          label: 'Quantize',       description: 'GGUF, MLX, ONNX, safetensors',                    position: 'output' },
  'eval':           { tag: 'eval-stage-element',           label: 'Evaluate',       description: 'HumanEval, MMLU, GSM8K, IMO-ProofBench',          position: 'output' },
  'deliver':        { tag: 'publish-stage-element',        label: 'Deliver',        description: 'Prepare for review — publish manually after',      position: 'output' },
  'deploy':         { tag: 'deploy-stage-element',         label: 'Deploy',         description: 'Push to grid node for serving',                    position: 'output' },
};

interface PipelineStage {
  type: string;
  config: Record<string, unknown>;
}

export class PipelineComposer extends ReactiveWidget {

  @reactive() stages: PipelineStage[] = [
    { type: 'source-config', config: {} },
    { type: 'context-extend', config: {} },
    { type: 'prune', config: {} },
    { type: 'train', config: {} },
    { type: 'eval', config: {} },
    { type: 'deliver', config: {} },
  ];

  @reactive() private _showAddMenu = false;

  /** Source model info — gates which stages are available in the add menu */
  @reactive() sourceModel: SourceModelInfo = {
    isMoE: false,
    hasVision: false,
    hasAudio: false,
    architecture: '',
  };

  /** Check if a stage type is available for the current source model */
  private isStageAvailable(type: string): boolean {
    const reg = STAGE_REGISTRY[type];
    if (!reg?.requires) return true;
    return reg.requires(this.sourceModel);
  }

  /** Current pipeline as alloy stages array */
  get pipelineConfig(): Record<string, unknown>[] {
    return this.stages.map(s => ({ type: s.type, ...s.config }));
  }

  private addStage(type: string): void {
    this.stages = [...this.stages, { type, config: {} }];
    this._showAddMenu = false;
    this.emitPipelineChange();
  }

  private removeStage(index: number): void {
    this.stages = this.stages.filter((_, i) => i !== index);
    this.emitPipelineChange();
  }

  private moveStage(index: number, direction: -1 | 1): void {
    const newIndex = index + direction;
    if (newIndex < 0 || newIndex >= this.stages.length) return;
    const updated = [...this.stages];
    [updated[index], updated[newIndex]] = [updated[newIndex], updated[index]];
    this.stages = updated;
    this.emitPipelineChange();
  }

  private onStageChange(e: CustomEvent): void {
    const { order, config } = e.detail;
    if (order >= 0 && order < this.stages.length) {
      const updated = [...this.stages];
      updated[order] = { ...updated[order], config };
      this.stages = updated;
      this.emitPipelineChange();
    }
  }

  private emitPipelineChange(): void {
    this.dispatchEvent(new CustomEvent('pipeline-change', {
      detail: this.pipelineConfig,
      bubbles: true,
      composed: true,
    }));
  }

  static override styles: CSSResultGroup = [
    ReactiveWidget.styles,
    css`
      :host { display: block; }

      .pipeline {
        display: flex;
        flex-direction: column;
        gap: 4px;
      }

      .stage-wrapper {
        position: relative;
      }

      .stage-actions {
        position: absolute;
        top: 8px;
        right: 8px;
        display: flex;
        gap: 4px;
        opacity: 0;
        transition: opacity 0.15s;
      }

      .stage-wrapper:hover .stage-actions {
        opacity: 1;
      }

      .stage-action-btn {
        width: 22px;
        height: 22px;
        border: 1px solid var(--border-color, rgba(255,255,255,0.12));
        border-radius: 3px;
        background: rgba(0,0,0,0.5);
        color: var(--content-secondary, #8a92a5);
        font-size: 11px;
        cursor: pointer;
        display: flex;
        align-items: center;
        justify-content: center;
        transition: all 0.15s;
      }

      .stage-action-btn:hover {
        border-color: var(--accent-primary, #00d4ff);
        color: var(--accent-primary, #00d4ff);
      }

      .stage-action-btn.remove:hover {
        border-color: #ff4444;
        color: #ff4444;
      }

      .connector {
        display: flex;
        justify-content: center;
        padding: 2px 0;
      }

      .connector-line {
        width: 2px;
        height: 12px;
        background: var(--border-color, rgba(255,255,255,0.15));
        border-radius: 1px;
      }

      .add-stage {
        display: flex;
        justify-content: center;
        margin-top: 8px;
      }

      .add-btn {
        padding: 6px 16px;
        font-size: 11px;
        font-weight: 600;
        border: 1px dashed var(--border-color, rgba(255,255,255,0.2));
        border-radius: 6px;
        background: transparent;
        color: var(--content-secondary, #8a92a5);
        cursor: pointer;
        transition: all 0.15s;
      }

      .add-btn:hover {
        border-color: var(--accent-primary, #00d4ff);
        color: var(--accent-primary, #00d4ff);
        border-style: solid;
      }

      .add-menu {
        display: flex;
        flex-wrap: wrap;
        gap: 4px;
        justify-content: center;
        margin-top: 8px;
        padding: 8px;
        background: var(--surface-elevated, rgba(255,255,255,0.04));
        border: 1px solid var(--border-color, rgba(255,255,255,0.08));
        border-radius: 6px;
      }

      .add-menu-item {
        padding: 4px 10px;
        font-size: 10px;
        font-weight: 600;
        border: 1px solid var(--border-color, rgba(255,255,255,0.12));
        border-radius: 4px;
        background: transparent;
        cursor: pointer;
        transition: all 0.15s;
      }

      .add-menu-item:hover {
        border-color: transparent;
      }

      .add-menu-group {
        width: 100%;
      }

      .add-menu-group-label {
        display: block;
        font-size: 8px;
        font-weight: 700;
        letter-spacing: 0.08em;
        color: var(--content-tertiary, #5a6070);
        margin-bottom: 4px;
        padding-left: 2px;
      }

      .add-menu-items {
        display: flex;
        gap: 4px;
        flex-wrap: wrap;
      }
    `,
  ];

  protected override render(): TemplateResult {
    return html`
      <div class="pipeline" @stage-change=${this.onStageChange}>
        ${this.stages.map((stage, i) => html`
          ${i > 0 ? html`<div class="connector"><div class="connector-line"></div></div>` : nothing}
          <div class="stage-wrapper">
            ${this.renderStage(stage, i)}
            <div class="stage-actions">
              ${i > 0 ? html`<button class="stage-action-btn" @click=${() => this.moveStage(i, -1)} title="Move up">&#9650;</button>` : nothing}
              ${i < this.stages.length - 1 ? html`<button class="stage-action-btn" @click=${() => this.moveStage(i, 1)} title="Move down">&#9660;</button>` : nothing}
              <button class="stage-action-btn remove" @click=${() => this.removeStage(i)} title="Remove">&#10005;</button>
            </div>
          </div>
        `)}
        <div class="add-stage">
          <button class="add-btn" @click=${() => this._showAddMenu = !this._showAddMenu}>
            + Add Stage
          </button>
        </div>
        ${this._showAddMenu ? this.renderAddMenu() : nothing}
      </div>
    `;
  }

  private renderStage(stage: PipelineStage, index: number): TemplateResult {
    const reg = STAGE_REGISTRY[stage.type];
    if (!reg) {
      return html`<div style="padding:8px;color:#ff4444;font-size:12px">Unknown stage: ${stage.type}</div>`;
    }
    // Create the element dynamically using its tag
    const tag = reg.tag;
    return html`${this.createStageElement(tag, index)}`;
  }

  private createStageElement(tag: string, order: number): TemplateResult {
    // Use static rendering — Lit handles element creation from tag strings via unsafeStatic
    // For now, switch on known tags (the registry is finite and known at compile time)
    switch (tag) {
      case 'source-config-stage-element':
        return html`<source-config-stage-element .order=${order}></source-config-stage-element>`;
      case 'context-extend-stage-element':
        return html`<context-extend-stage-element .order=${order}></context-extend-stage-element>`;
      case 'modality-stage-element':
        return html`<modality-stage-element .order=${order}></modality-stage-element>`;
      case 'prune-stage-element':
        return html`<prune-stage-element .order=${order}></prune-stage-element>`;
      case 'train-stage-element':
        return html`<train-stage-element .order=${order}></train-stage-element>`;
      case 'lora-stage-element':
        return html`<lora-stage-element .order=${order}></lora-stage-element>`;
      case 'compact-stage-element':
        return html`<compact-stage-element .order=${order}></compact-stage-element>`;
      case 'expert-prune-stage-element':
        return html`<expert-prune-stage-element .order=${order}></expert-prune-stage-element>`;
      case 'quant-stage-element':
        return html`<quant-stage-element .order=${order}></quant-stage-element>`;
      case 'eval-stage-element':
        return html`<eval-stage-element .order=${order}></eval-stage-element>`;
      case 'publish-stage-element':  // Used by both 'deliver' and legacy 'publish'
        return html`<publish-stage-element .order=${order}></publish-stage-element>`;
      case 'deploy-stage-element':
        return html`<deploy-stage-element .order=${order}></deploy-stage-element>`;
      default:
        return html`<div>Stage: ${tag} (UI not built yet)</div>`;
    }
  }

  private renderAddMenu(): TemplateResult {
    const groups = ['input', 'transform', 'output'] as const;
    const groupLabels = { input: 'INPUT', transform: 'TRANSFORM', output: 'OUTPUT' };

    return html`
      <div class="add-menu">
        ${groups.map(group => {
          const stages = Object.entries(STAGE_REGISTRY)
            .filter(([type, r]) => r.position === group && this.isStageAvailable(type));
          if (stages.length === 0) return nothing;
          return html`
            <div class="add-menu-group">
              <span class="add-menu-group-label">${groupLabels[group]}</span>
              <div class="add-menu-items">
                ${stages.map(([type, reg]) => {
                  const bg = STAGE_COLORS[type] ?? 'rgba(255,255,255,0.1)';
                  const color = STAGE_TEXT_COLORS[type] ?? '#e0e6ed';
                  return html`
                    <button class="add-menu-item"
                      style="background:${bg};color:${color};border-color:${color}33"
                      title=${reg.description}
                      @click=${() => this.addStage(type)}>${reg.label}</button>
                  `;
                })}
              </div>
            </div>
          `;
        })}
      </div>
    `;
  }
}

if (!customElements.get('pipeline-composer')) {
  customElements.define('pipeline-composer', PipelineComposer);
}
