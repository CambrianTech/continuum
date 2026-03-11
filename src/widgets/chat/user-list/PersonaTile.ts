/**
 * PersonaTile — Self-contained Lit component for persona/agent tiles.
 *
 * Every visual element is backed by real data:
 *   Ring       → AI_DECISION_EVENTS (cognitive phase)
 *   Diamonds   → THINKING / SPEAKING / LEARNING / TOOLS activity
 *   Meters     → INT (intelligence) + NRG (energy) + QUE (inbox queue depth)
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

/**
 * Diamond persist duration — each stage of cognition holds its diamond
 * lit for this long after the last event, giving a visible sequential glow.
 */
const DIAMOND_PERSIST_MS = 2500;

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
  @reactive() intelligenceLevel: number = 0;

  // === REACTIVE STATE (event-driven) ===
  @reactive() private _cognitivePhase: CognitivePhase = null;

  // Diamond states — map to cognition pipeline stages (clockwise: eval → generate → respond → learn)
  @reactive() private _evaluateActive: boolean = false;   // TOP: evaluating message
  @reactive() private _generateActive: boolean = false;   // RIGHT: LLM inference
  @reactive() private _respondActive: boolean = false;    // BOTTOM: posted response
  @reactive() private _learnActive: boolean = false;      // LEFT: captured training signal

  @reactive() private _energy: number = 1.0;
  @reactive() private _inboxLoad: number = 0;
  @reactive() private _mood: string = 'idle';
  @reactive() private _genomeLayers: GenomeLayerInfo[] = [];

  // Decay timers (one per diamond)
  private _evaluateTimer: ReturnType<typeof setTimeout> | null = null;
  private _generateTimer: ReturnType<typeof setTimeout> | null = null;
  private _respondTimer: ReturnType<typeof setTimeout> | null = null;
  private _learnTimer: ReturnType<typeof setTimeout> | null = null;

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
   * First render complete — all properties set by parent are available.
   * This is the reliable point to start event subscriptions and data fetching.
   */
  protected override firstUpdated(): void {
    if (this._isAI && this.userId) {
      if (this._unsubs.length === 0) this._subscribeToEvents();
      this._fetchGenomeLayers();
    }
  }

  /**
   * Re-subscribe when userId changes (EntityScroller may reuse elements).
   * Also handles the case where userType arrives after userId — re-check
   * on every property change if we haven't subscribed yet.
   */
  override updated(changedProperties: Map<string, unknown>): void {
    super.updated(changedProperties);
    if (changedProperties.has('userId') && this.userId) {
      this._cleanup();
      if (this._isAI) {
        this._subscribeToEvents();
        this._fetchGenomeLayers();
      }
    } else if (this._isAI && this.userId && this._unsubs.length === 0) {
      // Properties arrived in different order — subscribe now
      this._subscribeToEvents();
      this._fetchGenomeLayers();
    }
  }

  // === EVENT SUBSCRIPTIONS ===
  //
  // Diamonds map to the 4 stages of the cognition pipeline (clockwise):
  //   TOP    = Evaluate  (EVALUATING, DECIDED_RESPOND, CHECKING_REDUNDANCY)
  //   RIGHT  = Generate  (GENERATING, tool execution during generation)
  //   BOTTOM = Respond   (POSTED — persona has spoken)
  //   LEFT   = Learn     (INTERACTION_CAPTURED, training events)
  //
  // Every chat response fires all 4 in sequence, giving a visible clockwise sweep.

  private _subscribeToEvents(): void {
    const uid = this.userId;

    // ── TOP: Evaluate ──────────────────────────────────────────
    // Fires when persona begins evaluating whether to respond
    this._unsubs.push(
      Events.subscribe(AI_DECISION_EVENTS.EVALUATING, (data: { personaId: string }) => {
        if (data.personaId === uid) {
          this._activateDiamond('evaluate');
          this._cognitivePhase = 'evaluating';
        }
      }),
      Events.subscribe(AI_DECISION_EVENTS.DECIDED_RESPOND, (data: { personaId: string }) => {
        if (data.personaId === uid) {
          this._activateDiamond('evaluate');
          this._cognitivePhase = 'responding';
        }
      }),
      Events.subscribe(AI_DECISION_EVENTS.CHECKING_REDUNDANCY, (data: { personaId: string }) => {
        if (data.personaId === uid) {
          this._activateDiamond('evaluate');
          this._cognitivePhase = 'checking';
        }
      }),
      Events.subscribe(AI_DECISION_EVENTS.DECIDED_SILENT, (data: { personaId: string }) => {
        if (data.personaId === uid) this._cognitivePhase = 'passed';
      }),
      Events.subscribe(AI_DECISION_EVENTS.ERROR, (data: { personaId: string }) => {
        if (data.personaId === uid) this._cognitivePhase = 'error';
      })
    );

    // ── RIGHT: Generate ────────────────────────────────────────
    // Fires when LLM inference starts and during tool execution
    this._unsubs.push(
      Events.subscribe(AI_DECISION_EVENTS.GENERATING, (data: { personaId: string }) => {
        if (data.personaId === uid) {
          this._activateDiamond('generate');
          this._cognitivePhase = 'generating';
        }
      }),
      Events.subscribe(TOOL_EVENTS.STARTED, (data: { userId?: string }) => {
        if (data.userId === uid) this._activateDiamond('generate');
      }),
      Events.subscribe(TOOL_EVENTS.RESULT, (data: { userId?: string }) => {
        if (data.userId === uid) this._activateDiamond('generate');
      })
    );

    // ── BOTTOM: Respond ────────────────────────────────────────
    // Fires when persona posts a response (the "speaking" moment)
    this._unsubs.push(
      Events.subscribe(AI_DECISION_EVENTS.POSTED, (data: { personaId: string }) => {
        if (data.personaId === uid) {
          this._activateDiamond('respond');
          this._cognitivePhase = null;
        }
      }),
      // Also fires on voice synthesis (same semantic: persona is speaking)
      Events.subscribe('voice:ai:speech:start', (data: { personaId?: string; speakerId?: string }) => {
        if ((data.personaId ?? data.speakerId) === uid) this._activateDiamond('respond');
      })
    );

    // ── LEFT: Learn ────────────────────────────────────────────
    // Fires when training signal captured or active training
    this._unsubs.push(
      Events.subscribe(AI_LEARNING_EVENTS.INTERACTION_CAPTURED, (data: { personaId: string }) => {
        if (data.personaId === uid) this._activateDiamond('learn');
      }),
      Events.subscribe(AI_LEARNING_EVENTS.TRAINING_STARTED, (data: { personaId: string }) => {
        if (data.personaId === uid) this._activateDiamond('learn');
      }),
      Events.subscribe(AI_LEARNING_EVENTS.TRAINING_PROGRESS, (data: { personaId: string }) => {
        if (data.personaId === uid) this._activateDiamond('learn');
      }),
      Events.subscribe(AI_LEARNING_EVENTS.TRAINING_COMPLETE, (data: { personaId: string }) => {
        if (data.personaId === uid) {
          this._activateDiamond('learn');
          this._fetchGenomeLayers();
        }
      }),
      Events.subscribe(AI_LEARNING_EVENTS.TRAINING_ERROR, (data: { personaId: string }) => {
        if (data.personaId === uid) this._activateDiamond('learn');
      })
    );

    // ── State meters (energy, inbox, mood) ─────────────────────
    this._unsubs.push(
      Events.subscribe('persona:state:snapshot', (data: {
        personaId: string; energy: number;
        inboxLoad: number; mood: string;
      }) => {
        if (data.personaId === uid) {
          this._energy = data.energy;
          this._inboxLoad = data.inboxLoad;
          this._mood = data.mood;
        }
      })
    );
  }

  // === DIAMOND ACTIVATION WITH PERSIST ===

  private _activateDiamond(which: 'evaluate' | 'generate' | 'respond' | 'learn'): void {
    const timerKey = `_${which}Timer` as '_evaluateTimer' | '_generateTimer' | '_respondTimer' | '_learnTimer';
    const stateKey = `_${which}Active` as '_evaluateActive' | '_generateActive' | '_respondActive' | '_learnActive';

    // Activate
    (this as Record<string, unknown>)[stateKey] = true;

    // Clear existing timer (re-firing extends the persist window)
    if (this[timerKey]) clearTimeout(this[timerKey]!);

    // Persist then fade
    this[timerKey] = setTimeout(() => {
      (this as Record<string, unknown>)[stateKey] = false;
      this[timerKey] = null;
    }, DIAMOND_PERSIST_MS);
  }

  // === DATA FETCHING ===

  private async _fetchGenomeLayers(): Promise<void> {
    if (!this.userId) return;

    try {
      const result = await GenomeLayers.execute({
        personaId: this.userId,
        personaName: this.displayName,
      });
      if (result.success) {
        this._genomeLayers = result.layers;
      } else {
        console.error(`[PersonaTile] genome/layers failed for ${this.displayName}:`, (result as any).error ?? 'unknown');
      }
    } catch (err) {
      console.error(`[PersonaTile] genome/layers threw for ${this.displayName}:`, err);
    }
  }

  // === CLEANUP ===

  private _cleanup(): void {
    this._unsubs.forEach(unsub => unsub());
    this._unsubs = [];

    if (this._evaluateTimer) clearTimeout(this._evaluateTimer);
    if (this._generateTimer) clearTimeout(this._generateTimer);
    if (this._respondTimer) clearTimeout(this._respondTimer);
    if (this._learnTimer) clearTimeout(this._learnTimer);
    this._evaluateTimer = null;
    this._generateTimer = null;
    this._respondTimer = null;
    this._learnTimer = null;
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

  // === METERS — Real-time persona telemetry ===
  //   INT: Intelligence level (model capability, static per persona)
  //   NRG: Energy (depletes with work, recovers with rest)
  //   QUE: Inbox queue depth (how busy this persona is)

  /** Derive intelligence from provider/model when not explicitly set */
  private get _effectiveIntelligence(): number {
    if (this.intelligenceLevel > 0) return this.intelligenceLevel;

    // Infer from provider badge when not seeded
    const badge = this.modelBadge.toLowerCase();
    if (badge.includes('anthropic') || badge.includes('claude')) return 92;
    if (badge.includes('openai') || badge.includes('gpt')) return 88;
    if (badge.includes('google') || badge.includes('gemini')) return 85;
    if (badge.includes('deepseek') || badge.includes('deepsee')) return 82;
    if (badge.includes('firework') || badge.includes('firewor')) return 78;
    if (badge.includes('groq')) return 75;
    if (badge.includes('alibaba') || badge.includes('qwen')) return 72;
    if (badge.includes('candle')) return 45;  // Local small models
    if (badge.includes('ollama')) return 50;
    return 60;  // Unknown provider
  }

  private _renderMeters(): TemplateResult {
    const intel = this._effectiveIntelligence;
    const intelNorm = intel / 100;
    const intelColor = intel >= 80 ? '#a78bfa' :  // Purple for frontier
                       intel >= 60 ? '#00d4ff' :  // Cyan for capable
                       intel >= 40 ? '#ffaa00' :  // Orange for basic
                                     '#ff6b6b';   // Red for minimal

    const energyColor = this._energy >= 0.7 ? '#00ff88' :
                        this._energy >= 0.4 ? '#ffaa00' : '#ff6b6b';

    // Queue: normalize to 0-1 (saturate at 20+ items)
    const queueNorm = Math.min(1.0, this._inboxLoad / 20);
    // Queue color inverts: empty = dim, full = hot
    const queueColor = queueNorm >= 0.7 ? '#ff6b6b' :
                       queueNorm >= 0.3 ? '#ffaa00' : 'rgba(0, 255, 200, 0.3)';

    // Mood indicator
    const moodEmoji = this._mood === 'active' ? 'active' :
                      this._mood === 'tired' ? 'tired' :
                      this._mood === 'overwhelmed' ? 'overwhelmed' : 'idle';

    return html`
      <div class="meters" title="Mood: ${moodEmoji}">
        <div class="meter" title="Intelligence: ${intel}/100 — model capability level">
          <span class="meter-label">INT</span>
          <div class="meter-track">
            <div class="meter-fill" style="width: ${intelNorm * 100}%; background: ${intelColor};"></div>
          </div>
        </div>
        <div class="meter" title="Energy: ${Math.round(this._energy * 100)}% — depletes with work, recovers at rest">
          <span class="meter-label">NRG</span>
          <div class="meter-track">
            <div class="meter-fill" style="width: ${this._energy * 100}%; background: ${energyColor};"></div>
          </div>
        </div>
        <div class="meter" title="Queue: ${this._inboxLoad} items — messages waiting to process">
          <span class="meter-label">QUE</span>
          <div class="meter-track">
            <div class="meter-fill" style="width: ${queueNorm * 100}%; background: ${queueColor};"></div>
          </div>
        </div>
      </div>
    `;
  }

  // === GENOME PANEL (always shown for AI types — matches old layout) ===
  // Contains: label + bars + diamond grid (same structure as before)

  /**
   * Compute bar color from maturity score.
   * Gray (0–0.3) → amber (0.3–0.6) → cyan (0.6–0.8) → green (0.8–1.0)
   */
  /**
   * Continuous heatmap: deep blue (0.0) → teal → green → yellow → white-hot (1.0)
   * Interpolates through 5 stops so every maturity value gets a distinct color.
   */
  private _maturityColor(maturity: number): string {
    const stops: [number, number, number][] = [
      [30,  60, 120],   // 0.0 — deep slate blue
      [0,  160, 160],   // 0.25 — teal
      [0,  210,  80],   // 0.5 — green
      [255, 180,  0],   // 0.75 — amber/gold
      [0,  255, 136],   // 1.0 — bright mint
    ];
    const t = Math.max(0, Math.min(1, maturity));
    const segment = t * (stops.length - 1);
    const i = Math.min(Math.floor(segment), stops.length - 2);
    const frac = segment - i;
    const r = Math.round(stops[i][0] + (stops[i + 1][0] - stops[i][0]) * frac);
    const g = Math.round(stops[i][1] + (stops[i + 1][1] - stops[i][1]) * frac);
    const b = Math.round(stops[i][2] + (stops[i + 1][2] - stops[i][2]) * frac);
    return `rgb(${r}, ${g}, ${b})`;
  }

  private _renderGenomePanel(): TemplateResult {
    // Fixed 4 skill slots — like equipment slots in an RPG.
    // Top adapters by maturity fill slots; empty slots shown as inactive.
    const SLOT_COUNT = 4;

    // Sort by maturity descending so best adapters fill first
    const sortedLayers = [...this._genomeLayers]
      .sort((a, b) => (b.maturity ?? 0) - (a.maturity ?? 0))
      .slice(0, SLOT_COUNT);

    const barCount = SLOT_COUNT;

    const totalAdapters = this._genomeLayers.length;
    const genomeTitle = totalAdapters > SLOT_COUNT
      ? `GENOME (${totalAdapters} adapters, showing top ${SLOT_COUNT})`
      : `GENOME (${totalAdapters} adapter${totalAdapters === 1 ? '' : 's'})`;

    return html`
      <div class="genome-panel" title="${genomeTitle}">
        <div class="genome-label">GENOME</div>
        <div class="genome-bars">
          ${Array.from({ length: barCount }, (_, i) => {
            const layer = sortedLayers[i];
            const maturity = layer?.maturity ?? 0;
            // Continuous height: 25% floor → 100% ceiling, linear with maturity
            const heightPct = layer ? Math.round(25 + maturity * 75) : 15;
            const color = layer ? this._maturityColor(maturity) : '';

            // Rich tooltip with training details
            let title = 'Empty slot';
            if (layer) {
              const parts = [`${layer.name} (${layer.domain})`];
              parts.push(`Maturity: ${Math.round(maturity * 100)}%`);
              if (layer.trainingMetrics) {
                const m = layer.trainingMetrics;
                parts.push(`Loss: ${m.finalLoss.toFixed(3)}`);
                parts.push(`Examples: ${m.examplesProcessed}`);
                parts.push(`Epochs: ${m.epochs}`);
                if (m.phenotypeScore != null) parts.push(`Phenotype: ${m.phenotypeScore.toFixed(1)}`);
                if (m.phenotypeImprovement != null) parts.push(`Improvement: +${m.phenotypeImprovement.toFixed(1)}`);
              }
              if (!layer.hasWeights) parts.push('(no weights)');
              title = parts.join('\n');
            }

            const barStyle = layer
              ? `height: ${heightPct}%; --layer-maturity-color: ${color};`
              : '';

            const classes = layer
              ? (maturity > 0 ? 'has-data' : 'inactive')
              : 'inactive';
            const training = this._learnActive ? ' training' : '';

            return html`
              <div class="genome-layer ${classes}${training}"
                   style=${barStyle || nothing}
                   title="${title}"></div>
            `;
          })}
        </div>
        <div class="genome-diamond">
          <div class="diamond-cell ${this._evaluateActive ? 'active' : ''}" title="Evaluate"></div>
          <div class="diamond-cell ${this._generateActive ? 'active' : ''}" title="Generate"></div>
          <div class="diamond-cell ${this._respondActive ? 'active' : ''}" title="Respond"></div>
          <div class="diamond-cell ${this._learnActive ? 'active' : ''}" title="Learn"></div>
        </div>
      </div>
    `;
  }
}

// Register custom element
if (typeof customElements !== 'undefined' && !customElements.get('persona-tile')) {
  customElements.define('persona-tile', PersonaTile);
}
