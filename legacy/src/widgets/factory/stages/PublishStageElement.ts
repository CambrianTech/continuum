/**
 * PublishStageElement — UI for the alloy 'deliver' stage
 *
 * Prepares forge output for review. The actual publish to HuggingFace
 * happens manually via model/publish command after reviewing results.
 * Controls: org, repo name, tags, privacy, card generation
 * Maps 1:1 to ForgeRecipe DeliverStage schema.
 */

import { html, css, reactive, type TemplateResult, type CSSResultGroup } from '../../shared/ReactiveWidget';
import { StageElement, STAGE_BASE_STYLES } from './StageElement';

export class PublishStageElement extends StageElement {

  @reactive() private _org = 'continuum-ai';
  @reactive() private _repoNameTemplate = '{base}-{domain}-forged';
  @reactive() private _includeAlloy = true;
  @reactive() private _cardFromBenchmarks = true;
  @reactive() private _private = false;
  @reactive() private _tags: string[] = ['continuum', 'forged', 'experiential-plasticity', 'forge-alloy'];

  get stageType(): string { return 'deliver'; }

  get stageConfig(): Record<string, unknown> {
    return {
      type: 'deliver',
      org: this._org,
      repoNameTemplate: this._repoNameTemplate,
      includeAlloy: this._includeAlloy,
      cardFromBenchmarks: this._cardFromBenchmarks,
      tags: this._tags,
      private: this._private,
    };
  }

  override validate(): string[] {
    const errors: string[] = [];
    if (!this._org) errors.push('Organization is required');
    if (!this._repoNameTemplate) errors.push('Repo name template is required');
    return errors;
  }

  static override styles: CSSResultGroup = [
    STAGE_BASE_STYLES,
    css`
      :host { border-left: 3px solid #64c8ff; }

      .toggle-row {
        display: flex;
        gap: 8px;
        flex-wrap: wrap;
      }

      .pub-toggle {
        font-size: 10px;
        padding: 3px 10px;
        border-radius: 3px;
        border: 1px solid var(--border-color, rgba(255,255,255,0.12));
        background: transparent;
        color: var(--content-secondary, #8a92a5);
        cursor: pointer;
        transition: all 0.15s;
      }

      .pub-toggle.active {
        background: rgba(100, 200, 255, 0.15);
        border-color: #64c8ff;
        color: #64c8ff;
      }

      .pub-toggle.warn.active {
        background: rgba(255, 170, 0, 0.15);
        border-color: #ffaa00;
        color: #ffaa00;
      }
    `,
  ];

  protected override render(): TemplateResult {
    return html`
      ${this.renderHeader()}
      <div class="stage-controls">
        <div class="field">
          <span class="field-label">Organization</span>
          <input class="field-input" type="text"
            .value=${this._org}
            @change=${(e: Event) => { this._org = (e.target as HTMLInputElement).value; this.emitChange(); }}>
        </div>
        <div class="field">
          <span class="field-label">Repo Name Template</span>
          <input class="field-input" type="text"
            .value=${this._repoNameTemplate}
            @change=${(e: Event) => { this._repoNameTemplate = (e.target as HTMLInputElement).value; this.emitChange(); }}>
          <span class="field-hint">{base} = model name, {domain} = training domain</span>
        </div>
      </div>
      <div class="stage-controls single-col">
        <div class="field">
          <span class="field-label">Options</span>
          <div class="toggle-row">
            <button class="pub-toggle ${this._includeAlloy ? 'active' : ''}"
              @click=${() => { this._includeAlloy = !this._includeAlloy; this.emitChange(); }}>Include Alloy</button>
            <button class="pub-toggle ${this._cardFromBenchmarks ? 'active' : ''}"
              @click=${() => { this._cardFromBenchmarks = !this._cardFromBenchmarks; this.emitChange(); }}>Auto Model Card</button>
            <button class="pub-toggle warn ${this._private ? 'active' : ''}"
              @click=${() => { this._private = !this._private; this.emitChange(); }}>
              ${this._private ? 'Private' : 'Public'}
            </button>
          </div>
        </div>
      </div>
      ${this.renderGate()}
    `;
  }
}

if (!customElements.get('publish-stage-element')) {
  customElements.define('publish-stage-element', PublishStageElement);
}
