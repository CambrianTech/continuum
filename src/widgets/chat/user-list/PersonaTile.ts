/**
 * PersonaTile — Self-contained Lit component for persona/agent tiles.
 *
 * Every visual element is backed by real data:
 *   Ring       → AI_DECISION_EVENTS (cognitive phase)
 *   Diamonds   → THINKING / SPEAKING / LEARNING / TOOLS activity
 *   Meters     → Energy (persona state) + Fitness (adapter count)
 *   Genome bars→ Real LoRA adapters from AdapterStore via genome/layers
 *
 * EntityScroller caches the outer DOM node, but this component manages
 * its own @reactive() state internally — no manual DOM patching needed.
 */

import { LitElement, html, nothing, unsafeCSS, type TemplateResult, type CSSResultGroup } from 'lit';
import { reactive } from '../../shared/ReactiveWidget';
import { Events } from '../../../system/core/shared/Events';
import { AI_DECISION_EVENTS } from '../../../system/events/shared/AIDecisionEvents';
import { AI_LEARNING_EVENTS } from '../../../system/events/shared/AILearningEvents';
import { TOOL_EVENTS } from '../../../system/core/shared/ToolResult';
import { GenomeLayers, type GenomeLayerInfo } from '../../../commands/genome/layers/shared/GenomeLayersTypes';
import { styles as tileStyles } from './persona-tile.styles';

/** Diamond decay timeout in ms */
const DIAMOND_DECAY_MS = 2000;

type CognitivePhase = 'evaluating' | 'responding' | 'generating' | 'checking' | 'passed' | 'error' | null;

export class PersonaTile extends LitElement {
  // === PROPERTIES (set by parent) ===
  @reactive() userId: string = '';
  @reactive() displayName: string = '';
  @reactive() userType: string = '';
  @reactive() uniqueId: string = '';
  @reactive() status: string = 'offline';
  @reactive() speciality: string = '';
  @reactive() modelInfo: string = '';
  @reactive() modelBadge: string = '';
  @reactive() requiresMention: boolean = false;
  @reactive() ragCertified: boolean = false;
  @reactive() lastActive: string = '';

  // === REACTIVE STATE (event-driven) ===
  @reactive() private _cognitivePhase: CognitivePhase = null;
  @reactive() private _thinkingActive: boolean = false;
  @reactive() private _speakingActive: boolean = false;
  @reactive() private _learningActive: boolean = false;
  @reactive() private _toolsActive: boolean = false;
  @reactive() private _energy: number = 1.0;
  @reactive() private _fitness: number = 0;
  @reactive() private _genomeLayers: GenomeLayerInfo[] = [];


  // Decay timers
  private _thinkingTimer: ReturnType<typeof setTimeout> | null = null;
  private _speakingTimer: ReturnType<typeof setTimeout> | null = null;
  private _learningTimer: ReturnType<typeof setTimeout> | null = null;
  private _toolsTimer: ReturnType<typeof setTimeout> | null = null;

  // Event unsubscribers
  private _unsubs: Array<() => void> = [];

  static override styles = [unsafeCSS(tileStyles)] as CSSResultGroup;

  private get _isAI(): boolean {
    return this.userType === 'persona' || this.userType === 'agent';
  }

  // === LIFECYCLE ===

  override connectedCallback(): void {
    super.connectedCallback();
    if (this._isAI && this.userId) {
      this._subscribeToEvents();
      this._fetchGenomeLayers();
    }
  }

  override disconnectedCallback(): void {
    super.disconnectedCallback();
    this._cleanup();
  }

  /**
   * Re-subscribe when userId changes (EntityScroller may reuse elements)
   */
  override updated(changedProperties: Map<string, unknown>): void {
    super.updated(changedProperties);
    if (changedProperties.has('userId') && this.userId) {
      this._cleanup();
      if (this._isAI) {
        this._subscribeToEvents();
        this._fetchGenomeLayers();
      }
    }
  }

  // === EVENT SUBSCRIPTIONS ===

  private _subscribeToEvents(): void {
    const uid = this.userId;

    // THINKING diamond — any AI decision activity for this persona
    this._unsubs.push(
      Events.subscribe(AI_DECISION_EVENTS.EVALUATING, (data: { personaId: string }) => {
        if (data.personaId === uid) this._activateDiamond('thinking');
        if (data.personaId === uid) this._cognitivePhase = 'evaluating';
      }),
      Events.subscribe(AI_DECISION_EVENTS.DECIDED_RESPOND, (data: { personaId: string }) => {
        if (data.personaId === uid) this._activateDiamond('thinking');
        if (data.personaId === uid) this._cognitivePhase = 'responding';
      }),
      Events.subscribe(AI_DECISION_EVENTS.GENERATING, (data: { personaId: string }) => {
        if (data.personaId === uid) this._activateDiamond('thinking');
        if (data.personaId === uid) this._cognitivePhase = 'generating';
      }),
      Events.subscribe(AI_DECISION_EVENTS.CHECKING_REDUNDANCY, (data: { personaId: string }) => {
        if (data.personaId === uid) this._activateDiamond('thinking');
        if (data.personaId === uid) this._cognitivePhase = 'checking';
      }),
      Events.subscribe(AI_DECISION_EVENTS.DECIDED_SILENT, (data: { personaId: string }) => {
        if (data.personaId === uid) this._cognitivePhase = 'passed';
      }),
      Events.subscribe(AI_DECISION_EVENTS.POSTED, (data: { personaId: string }) => {
        if (data.personaId === uid) this._cognitivePhase = null;
      }),
      Events.subscribe(AI_DECISION_EVENTS.ERROR, (data: { personaId: string }) => {
        if (data.personaId === uid) this._cognitivePhase = 'error';
      })
    );

    // SPEAKING diamond — voice synthesis events
    this._unsubs.push(
      Events.subscribe('voice:ai:speech:start', (data: { personaId?: string; speakerId?: string }) => {
        if ((data.personaId ?? data.speakerId) === uid) this._activateDiamond('speaking');
      }),
      Events.subscribe('voice:ai:speech:end', (data: { personaId?: string; speakerId?: string }) => {
        // Let decay timer handle it, but reset the active duration
        if ((data.personaId ?? data.speakerId) === uid) this._activateDiamond('speaking');
      })
    );

    // LEARNING diamond — training events
    this._unsubs.push(
      Events.subscribe(AI_LEARNING_EVENTS.TRAINING_STARTED, (data: { personaId: string }) => {
        if (data.personaId === uid) this._activateDiamond('learning');
      }),
      Events.subscribe(AI_LEARNING_EVENTS.TRAINING_PROGRESS, (data: { personaId: string }) => {
        if (data.personaId === uid) this._activateDiamond('learning');
      }),
      Events.subscribe(AI_LEARNING_EVENTS.TRAINING_COMPLETE, (data: { personaId: string }) => {
        if (data.personaId === uid) {
          this._activateDiamond('learning');
          // Refresh genome bars — new adapter may have been trained
          this._fetchGenomeLayers();
        }
      }),
      Events.subscribe(AI_LEARNING_EVENTS.TRAINING_ERROR, (data: { personaId: string }) => {
        if (data.personaId === uid) this._activateDiamond('learning');
      }),
      Events.subscribe(AI_LEARNING_EVENTS.INTERACTION_CAPTURED, (data: { personaId: string }) => {
        if (data.personaId === uid) this._activateDiamond('learning');
      })
    );

    // TOOLS diamond — tool execution events
    this._unsubs.push(
      Events.subscribe(TOOL_EVENTS.STARTED, (data: { userId?: string }) => {
        if (data.userId === uid) this._activateDiamond('tools');
      }),
      Events.subscribe(TOOL_EVENTS.RESULT, (data: { userId?: string }) => {
        if (data.userId === uid) this._activateDiamond('tools');
      })
    );

    // Energy meter — persona state snapshots
    this._unsubs.push(
      Events.subscribe('persona:state:snapshot', (data: { personaId: string; energy: number }) => {
        if (data.personaId === uid) {
          this._energy = data.energy;
        }
      })
    );
  }

  // === DIAMOND ACTIVATION WITH DECAY ===

  private _activateDiamond(which: 'thinking' | 'speaking' | 'learning' | 'tools'): void {
    const timerKey = `_${which}Timer` as '_thinkingTimer' | '_speakingTimer' | '_learningTimer' | '_toolsTimer';
    const stateKey = `_${which}Active` as '_thinkingActive' | '_speakingActive' | '_learningActive' | '_toolsActive';

    // Activate
    (this as Record<string, unknown>)[stateKey] = true;

    // Clear existing timer
    if (this[timerKey]) clearTimeout(this[timerKey]!);

    // Set decay timer
    this[timerKey] = setTimeout(() => {
      (this as Record<string, unknown>)[stateKey] = false;
      this[timerKey] = null;
    }, DIAMOND_DECAY_MS);
  }

  // === DATA FETCHING ===

  private async _fetchGenomeLayers(): Promise<void> {
    if (!this.userId) return;

    try {
      const result = await GenomeLayers.execute({ personaId: this.userId });
      if (result.success) {
        this._genomeLayers = result.layers;
        this._fitness = result.fitness;
      }
    } catch {
      // Graceful absence — no genome section if command fails
      this._genomeLayers = [];
      this._fitness = 0;
    }
  }

  // === CLEANUP ===

  private _cleanup(): void {
    this._unsubs.forEach(unsub => unsub());
    this._unsubs = [];

    if (this._thinkingTimer) clearTimeout(this._thinkingTimer);
    if (this._speakingTimer) clearTimeout(this._speakingTimer);
    if (this._learningTimer) clearTimeout(this._learningTimer);
    if (this._toolsTimer) clearTimeout(this._toolsTimer);
    this._thinkingTimer = null;
    this._speakingTimer = null;
    this._learningTimer = null;
    this._toolsTimer = null;
  }

  // === RENDER ===
  // Layout mirrors the original UserListWidget structure exactly:
  //   .tile-content > [last-active] + .tile-avatar + .tile-info + .genome-panel

  protected override render(): TemplateResult {
    const statusClass = this.status === 'online' ? 'online' : 'offline';
    const emoji = this.userType === 'human' ? '👤' :
                  this.userType === 'agent' ? '🤖' :
                  this.userType === 'persona' ? '⭐' :
                  this.userType === 'system' ? '⚙️' : '❓';

    // AI personas: use avatar image as background if available, emoji fallback
    const avatarUrl = this._isAI && this.uniqueId ? `/avatars/${this.uniqueId}.png` : '';
    const avatarStyle = avatarUrl
      ? `background-image: url('${avatarUrl}'); background-size: cover; background-position: center top;`
      : '';

    return html`
      <div class="tile-content ${statusClass}" data-ai-status=${this._cognitivePhase ?? nothing}>
        ${this.lastActive ? html`<span class="tile-last-active">${this.lastActive}</span>` : nothing}
        <span class="tile-avatar" style=${avatarStyle || nothing}>
          ${!avatarUrl ? emoji : nothing}
          <span class="status-indicator"></span>
          ${this._isAI ? html`
            <span class="response-mode-dot ${this.requiresMention ? 'mention-required' : 'free-chat'}"
                  title=${this.requiresMention ? 'Requires @mention' : 'Can respond freely'}></span>
          ` : nothing}
        </span>
        <div class="tile-info">
          <div class="tile-name-row">
            <span class="tile-name">${this.displayName}</span>
          </div>
          <div class="tile-meta">
            <span class="tile-type-badge">${this.userType}</span>
            ${this.modelInfo ? html`<span class="tile-model-info" title="AI Model">${this.modelInfo}</span>` : nothing}
            ${this.speciality ? html`<span class="tile-speciality">${this.speciality}</span>` : nothing}
            ${this.modelBadge ? html`<span class="tile-model-badge">${this.modelBadge}</span>` : nothing}
          </div>
          ${this._isAI ? this._renderMeters() : nothing}
        </div>
        ${this._isAI ? this._renderGenomePanel() : nothing}
      </div>
    `;
  }

  // === METERS (replaces IQ bars — same visual position, real data) ===

  private _renderMeters(): TemplateResult {
    const energyColor = this._energy >= 0.7 ? '#00ff88' :
                        this._energy >= 0.4 ? '#ffaa00' : '#ff6b6b';
    const fitnessColor = '#00d4ff';

    return html`
      <div class="meters">
        <div class="meter" title="Energy: ${Math.round(this._energy * 100)}%">
          <span class="meter-label">NRG</span>
          <div class="meter-track">
            <div class="meter-fill" style="width: ${this._energy * 100}%; background: ${energyColor};"></div>
          </div>
        </div>
        <div class="meter" title="Fitness: ${Math.round(this._fitness * 100)}%">
          <span class="meter-label">FIT</span>
          <div class="meter-track">
            <div class="meter-fill" style="width: ${this._fitness * 100}%; background: ${fitnessColor};"></div>
          </div>
        </div>
      </div>
    `;
  }

  // === GENOME PANEL (always shown for AI types — matches old layout) ===
  // Contains: label + bars + diamond grid (same structure as before)

  private _renderGenomePanel(): TemplateResult {
    // If we have real adapters, show those. Otherwise show 4 inactive placeholder bars.
    const MIN_BARS = 4;
    const barCount = Math.max(this._genomeLayers.length, MIN_BARS);

    return html`
      <div class="genome-panel">
        <div class="genome-label">GENOME</div>
        <div class="genome-bars">
          ${Array.from({ length: barCount }, (_, i) => {
            const layer = this._genomeLayers[i];
            const isActive = layer?.hasWeights ?? false;
            const title = layer
              ? `${layer.name} (${layer.domain})${layer.hasWeights ? '' : ' — no weights'}`
              : 'Empty slot';
            return html`
              <div class="genome-layer ${isActive ? 'active' : 'inactive'}" title="${title}"></div>
            `;
          })}
        </div>
        <div class="genome-diamond">
          <div class="diamond-cell top ${this._thinkingActive ? 'active' : ''}" title="Thinking"></div>
          <div class="diamond-cell right ${this._speakingActive ? 'active' : ''}" title="Speaking"></div>
          <div class="diamond-cell bottom ${this._learningActive ? 'active' : ''}" title="Learning"></div>
          <div class="diamond-cell left ${this._toolsActive ? 'active' : ''}" title="Tools"></div>
        </div>
      </div>
    `;
  }
}

// Register custom element
if (typeof customElements !== 'undefined' && !customElements.get('persona-tile')) {
  customElements.define('persona-tile', PersonaTile);
}
