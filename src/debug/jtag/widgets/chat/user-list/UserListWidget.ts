/**
 * User List Widget - Migrated to ReactiveListWidget
 * Uses header/item/footer pattern with Lit templates
 */

import {
  ReactiveListWidget,
  html,
  reactive,
  unsafeCSS,
  nothing,
  type TemplateResult,
  type CSSResultGroup
} from '../../shared/ReactiveListWidget';
import { render } from 'lit';
import { unsafeHTML } from 'lit/directives/unsafe-html.js';
import type { RenderFn, RenderContext } from '../../shared/EntityScroller';
import { UserEntity } from '../../../system/data/entities/UserEntity';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import { Commands } from '../../../system/core/shared/Commands';
import { Events } from '../../../system/core/shared/Events';
import { AI_DECISION_EVENTS } from '../../../system/events/shared/AIDecisionEvents';
import { AI_LEARNING_EVENTS } from '../../../system/events/shared/AILearningEvents';
import { ContentService } from '../../../system/state/ContentService';

import { styles as externalStyles } from './user-list.styles';

// Verbose logging helper
const verbose = () => typeof window !== 'undefined' && (window as any).JTAG_VERBOSE === true;

/**
 * AI Status tracking for user list display
 */
interface AIStatusState {
  personaId: string;
  currentPhase: 'evaluating' | 'responding' | 'generating' | 'checking' | 'passed' | 'error' | null;
  timestamp: number;
  errorMessage?: string;
}

/**
 * AI Learning State tracking
 */
interface AILearningState {
  personaId: string;
  isLearning: boolean;
  domain?: string;
  timestamp: number;
}

export class UserListWidget extends ReactiveListWidget<UserEntity> {
  readonly collection = UserEntity.collection;

  // === REACTIVE STATE ===
  @reactive() private _selectedUserId: string | null = null;
  @reactive() private aiStatuses: Map<string, AIStatusState> = new Map();
  @reactive() private learningStatuses: Map<string, AILearningState> = new Map();

  // Filter chips state
  @reactive() private activeFilters: Set<string> = new Set(['all']);

  static override styles = [
    ReactiveListWidget.styles,
    unsafeCSS(externalStyles)
  ] as CSSResultGroup;

  constructor() {
    super({ widgetName: 'UserListWidget' });
  }

  // === CONFIGURATION ===
  protected override get listTitle(): string { return 'Users & Agents'; }
  protected override get containerClass(): string { return 'user-list'; }
  protected override get orderBy() {
    return [{ field: 'lastActiveAt', direction: 'desc' as const }];
  }

  // === HEADER with filter chips ===
  protected override renderHeader(): TemplateResult {
    const typeFilters = [
      { id: 'all', label: 'All', icon: '◉' },
      { id: 'human', label: 'Human', icon: '👤' },
      { id: 'persona', label: 'Persona', icon: '⭐' },
      { id: 'agent', label: 'Agent', icon: '🤖' }
    ];

    const statusFilters = [
      { id: 'online', label: 'Online', icon: '●' }
    ];

    return html`
      <div class="entity-list-header">
        <span class="header-title">${this.listTitle}</span>
        <span class="user-count">${this.entityCount}</span>
      </div>
      <div class="filter-chips">
        <div class="filter-group type-filters">
          ${typeFilters.map(f => html`
            <button
              class="filter-chip ${this.activeFilters.has(f.id) ? 'active' : ''}"
              data-filter="${f.id}"
              @click=${() => this.toggleFilter(f.id)}
            >
              <span class="chip-icon">${f.icon}</span>
              <span class="chip-label">${f.label}</span>
            </button>
          `)}
        </div>
        <span class="filter-divider"></span>
        <div class="filter-group status-filters">
          ${statusFilters.map(f => html`
            <button
              class="filter-chip ${this.activeFilters.has(f.id) ? 'active' : ''}"
              data-filter="${f.id}"
              @click=${() => this.toggleFilter(f.id)}
            >
              <span class="chip-icon">${f.icon}</span>
              <span class="chip-label">${f.label}</span>
            </button>
          `)}
        </div>
      </div>
    `;
  }

  private toggleFilter(filterId: string): void {
    const newFilters = new Set(this.activeFilters);

    if (filterId === 'all') {
      // 'All' clears other type filters, keeps status filters
      newFilters.clear();
      newFilters.add('all');
    } else if (filterId === 'online') {
      // Toggle online status independently
      if (newFilters.has('online')) {
        newFilters.delete('online');
      } else {
        newFilters.add('online');
      }
    } else {
      // Type filter - remove 'all' and toggle this type
      newFilters.delete('all');
      if (newFilters.has(filterId)) {
        newFilters.delete(filterId);
        // If no type filters left, go back to 'all'
        const typeFilters = ['human', 'persona', 'agent'];
        if (!typeFilters.some(t => newFilters.has(t))) {
          newFilters.add('all');
        }
      } else {
        newFilters.add(filterId);
      }
    }

    this.activeFilters = newFilters;
    this.requestUpdate();  // Re-render header with new active states

    // Clear and reload to apply new filters
    // (getRenderFunction checks matchesFilters for each item)
    this.scroller?.clear();
    this.scroller?.load();
  }

  // === MAIN RENDER ===
  override render(): TemplateResult {
    return html`
      <div class="entity-list-container">
        ${this.renderHeader()}
        <div class="${this.containerClass}"></div>
        ${this.renderFooter()}
      </div>
    `;
  }

  // === ITEM RENDERING ===
  renderItem(user: UserEntity): TemplateResult {
    const displayName = user.displayName ?? 'Unknown User';
    const statusClass = user.status === 'online' ? 'online' : 'offline';
    const isSelected = this._selectedUserId === user.id;

    // Avatar by type
    const avatar = user.type === 'human' ? '👤' :
                  user.type === 'agent' ? '🤖' :
                  user.type === 'persona' ? '⭐' :
                  user.type === 'system' ? '⚙️' : '❓';

    const speciality = user.speciality;
    const aiStatus = this.aiStatuses.get(user.id);
    const learningState = this.learningStatuses.get(user.id);
    const lastActive = user.lastActiveAt ? this.formatTimestamp(user.lastActiveAt) : null;

    // Model info for AI
    let modelInfo = '';
    let modelBadge = '';
    if (user.type === 'persona' || user.type === 'agent') {
      const provider = user.modelConfig?.provider || (user.personaConfig?.responseModel ? 'ollama' : '');
      const model = user.modelConfig?.model || user.personaConfig?.responseModel || '';
      if (provider && model) {
        modelInfo = `${provider}/${model}`;
        modelBadge = provider.substring(0, 8).toUpperCase();
      }
    }

    // RAG certification
    let ragCertified = user.modelConfig?.ragCertified ?? false;
    if (!ragCertified && user.uniqueId === 'persona-sentinel') {
      ragCertified = true;
    }

    // Intelligence level
    let intelligenceLevel = user.intelligenceLevel ?? 0;
    if (intelligenceLevel === 0 && (user.type === 'persona' || user.type === 'agent')) {
      const demoLevels: Record<string, number> = {
        'persona-helper-001': 82,
        'persona-teacher-001': 86,
        'persona-codereview-001': 79,
        'persona-deepseek-001': 94,
        'persona-claude-code-001': 91,
        'persona-general-ai-001': 75,
        'persona-openai': 88,
        'persona-xai': 85,
        'persona-together': 77,
        'persona-fireworks': 80,
        'persona-ollama': 70,
        'persona-sentinel': 92
      };
      intelligenceLevel = demoLevels[user.uniqueId] ?? 75;
    }

    // Response mode
    const requiresMention = user.modelConfig?.requiresExplicitMention ?? false;

    return html`
      <div
        class="user-item ${statusClass} ${isSelected ? 'selected' : ''}"
        data-user-id=${user.id}
        data-user-type=${user.type}
        data-ai-status=${aiStatus?.currentPhase ?? nothing}
        data-learning=${learningState?.isLearning ? 'true' : nothing}
        @click=${(e: Event) => this.handleUserClick(e, user)}
      >
        ${lastActive ? html`<span class="user-last-active">${lastActive}</span>` : nothing}
        <span class="user-avatar">
          ${avatar}
          <span class="status-indicator"></span>
          ${(user.type === 'persona' || user.type === 'agent') ? html`
            <span class="response-mode-dot ${requiresMention ? 'mention-required' : 'free-chat'}"
                  title=${requiresMention ? 'Requires @mention' : 'Can respond freely'}></span>
          ` : nothing}
        </span>
        <div class="user-info">
          <div class="user-name-row">
            <span class="user-name">${displayName}</span>
          </div>
          <div class="user-meta">
            <span class="user-type-badge">${user.type}</span>
            ${modelInfo ? html`<span class="user-model-info" title="AI Model">${modelInfo}</span>` : nothing}
            ${speciality ? html`<span class="user-speciality">${speciality}</span>` : nothing}
            ${modelBadge ? html`<span class="user-model-badge">${modelBadge}</span>` : nothing}
          </div>
          ${intelligenceLevel > 0 ? unsafeHTML(this.renderIntelligenceBars(intelligenceLevel, ragCertified)) : nothing}
        </div>
        <div class="user-controls">
          <button class="user-favorite-btn" title="Add to favorites" @click=${(e: Event) => this.handleFavoriteClick(e, user.id)}>⭐</button>
          <button class="user-action-btn" title="Actions" @click=${(e: Event) => this.handleActionClick(e, user.id)}>»</button>
        </div>
        ${unsafeHTML(this.renderGenomePanel(user))}
      </div>
    `;
  }

  // === FILTERING ===
  private matchesFilters(user: UserEntity): boolean {
    // Check type filters
    const hasAll = this.activeFilters.has('all');
    const typeFilters = ['human', 'persona', 'agent'];
    const activeTypeFilters = typeFilters.filter(t => this.activeFilters.has(t));

    if (!hasAll && activeTypeFilters.length > 0) {
      if (!activeTypeFilters.includes(user.type)) {
        return false;
      }
    }

    // Check online filter
    if (this.activeFilters.has('online')) {
      if (user.status !== 'online') {
        return false;
      }
    }

    return true;
  }

  // Override getRenderFunction to apply chip filtering
  protected override getRenderFunction(): RenderFn<UserEntity> {
    return (user: UserEntity, _context: RenderContext<UserEntity>) => {
      // Return hidden element for non-matching users
      if (!this.matchesFilters(user)) {
        const hiddenElement = globalThis.document.createElement('div');
        hiddenElement.style.display = 'none';
        hiddenElement.dataset.id = user.id;
        return hiddenElement;
      }

      // Normal rendering via parent
      const div = globalThis.document.createElement('div');
      div.className = 'list-item';
      div.dataset.id = user.id;
      render(this.renderItem(user), div);
      div.addEventListener('click', (e) => {
        e.stopPropagation();
        this.onItemClick(user);
      });
      return div;
    };
  }

  // === LIFECYCLE ===
  protected override onFirstRender(): void {
    super.onFirstRender();
    this.setupAIEventSubscriptions();
    this.setupLearningEventSubscriptions();
  }

  // === AI STATUS SUBSCRIPTIONS ===
  private setupAIEventSubscriptions(): void {
    verbose() && console.log('🔧 UserListWidget: Setting up AI event subscriptions...');

    this.createMountEffect(() => {
      const unsubs = [
        Events.subscribe(AI_DECISION_EVENTS.EVALUATING, (data: { personaId: string }) => {
          this.updateAIStatus(data.personaId, 'evaluating');
        }),
        Events.subscribe(AI_DECISION_EVENTS.DECIDED_RESPOND, (data: { personaId: string }) => {
          this.updateAIStatus(data.personaId, 'responding');
        }),
        Events.subscribe(AI_DECISION_EVENTS.DECIDED_SILENT, (data: { personaId: string }) => {
          this.updateAIStatus(data.personaId, 'passed');
        }),
        Events.subscribe(AI_DECISION_EVENTS.GENERATING, (data: { personaId: string }) => {
          this.updateAIStatus(data.personaId, 'generating');
        }),
        Events.subscribe(AI_DECISION_EVENTS.CHECKING_REDUNDANCY, (data: { personaId: string }) => {
          this.updateAIStatus(data.personaId, 'checking');
        }),
        Events.subscribe(AI_DECISION_EVENTS.POSTED, (data: { personaId: string }) => {
          this.updateAIStatus(data.personaId, null);
        }),
        Events.subscribe(AI_DECISION_EVENTS.ERROR, (data: { personaId: string; error: string }) => {
          this.updateAIStatus(data.personaId, 'error', data.error);
        })
      ];
      return () => unsubs.forEach(unsub => unsub());
    });
  }

  private setupLearningEventSubscriptions(): void {
    verbose() && console.log('🧬 UserListWidget: Setting up learning event subscriptions...');

    this.createMountEffect(() => {
      const unsubs = [
        Events.subscribe(AI_LEARNING_EVENTS.TRAINING_STARTED, (data: { personaId: string; domain: string }) => {
          this.updateLearningStatus(data.personaId, true, data.domain);
        }),
        Events.subscribe(AI_LEARNING_EVENTS.TRAINING_COMPLETE, (data: { personaId: string }) => {
          this.updateLearningStatus(data.personaId, false);
        }),
        Events.subscribe(AI_LEARNING_EVENTS.TRAINING_ERROR, (data: { personaId: string }) => {
          this.updateLearningStatus(data.personaId, false);
        })
      ];
      return () => unsubs.forEach(unsub => unsub());
    });
  }

  // === STATUS UPDATES ===
  // NOTE: EntityScroller caches rendered elements, so reactive state changes
  // don't automatically re-render existing items. We must update DOM directly.
  private updateAIStatus(personaId: string, phase: AIStatusState['currentPhase'], errorMessage?: string): void {
    // Update reactive state for new renders
    const newMap = new Map(this.aiStatuses);
    if (phase === null) {
      newMap.delete(personaId);
    } else {
      newMap.set(personaId, { personaId, currentPhase: phase, timestamp: Date.now(), errorMessage });
    }
    this.aiStatuses = newMap;

    // CRITICAL: Also update DOM directly for cached elements
    // EntityScroller doesn't re-render existing items when state changes
    const userElement = this.shadowRoot?.querySelector(`[data-user-id="${CSS.escape(personaId)}"]`) as HTMLElement;
    if (userElement) {
      if (phase === null) {
        userElement.removeAttribute('data-ai-status');
      } else {
        userElement.setAttribute('data-ai-status', phase);
      }
      verbose() && console.log(`🎯 UserList: Updated AI status for ${personaId} to ${phase}`);
    }
  }

  private updateLearningStatus(personaId: string, isLearning: boolean, domain?: string): void {
    // Update reactive state for new renders
    const newMap = new Map(this.learningStatuses);
    if (!isLearning) {
      newMap.delete(personaId);
    } else {
      newMap.set(personaId, { personaId, isLearning: true, domain, timestamp: Date.now() });
    }
    this.learningStatuses = newMap;

    // CRITICAL: Also update DOM directly for cached elements
    const userElement = this.shadowRoot?.querySelector(`[data-user-id="${CSS.escape(personaId)}"]`) as HTMLElement;
    if (userElement) {
      if (isLearning) {
        userElement.setAttribute('data-learning', 'true');
      } else {
        userElement.removeAttribute('data-learning');
      }
    }
  }

  // === EVENT HANDLERS ===
  private handleUserClick(e: Event, user: UserEntity): void {
    if ((e.target as HTMLElement).tagName === 'BUTTON') return;
    this._selectedUserId = user.id;
    this.openUserProfile(user);
  }

  private handleFavoriteClick(e: Event, userId: string): void {
    e.stopPropagation();
    verbose() && console.log(`⭐ UserListWidget: Toggle favorite for user ${userId}`);
  }

  private handleActionClick(e: Event, userId: string): void {
    e.stopPropagation();
    verbose() && console.log(`» UserListWidget: Show action menu for user ${userId}`);
  }

  private openUserProfile(userEntity: UserEntity): void {
    // IMPORTANT: entityId is always UUID for database lookups
    // uniqueId is human-readable string for URLs ("together" not UUID)
    const entityId = userEntity.id;  // Always UUID
    const uniqueId = userEntity.uniqueId || userEntity.id;  // Human-readable, fallback to UUID
    const title = userEntity.displayName || 'User Profile';

    verbose() && console.log(`👤 UserListWidget: Opening profile for ${title} (entityId=${entityId}, uniqueId=${uniqueId})`);

    // OPTIMISTIC: Use ContentService for instant tab creation
    // ContentService handles: tab creation, view switch, URL update, server persist
    if (this.currentUser?.id) {
      ContentService.setUserId(this.currentUser.id as UUID);
    }

    ContentService.open('profile', entityId, {
      title,
      uniqueId,  // Human-readable uniqueId for URLs
      metadata: { entity: userEntity }  // Pass full entity for instant hydration
    });
  }

  // === HELPER METHODS ===
  private formatTimestamp(timestamp: number | Date | string | undefined): string {
    if (!timestamp) return 'Never';

    let timestampMs: number;
    if (typeof timestamp === 'number') {
      timestampMs = timestamp;
    } else if (timestamp instanceof Date) {
      timestampMs = timestamp.getTime();
    } else if (typeof timestamp === 'string') {
      timestampMs = new Date(timestamp).getTime();
    } else {
      return 'Unknown';
    }

    const now = Date.now();
    const diff = now - timestampMs;

    if (diff < 60000) return 'Just now';
    if (diff < 3600000) return `${Math.floor(diff / 60000)}m ago`;
    if (diff < 86400000) return `${Math.floor(diff / 3600000)}h ago`;
    if (diff < 604800000) return `${Math.floor(diff / 86400000)}d ago`;
    return new Date(timestampMs).toLocaleDateString();
  }

  // === SVG RENDERING (returns string for unsafeHTML) ===
  private renderIntelligenceBars(level: number, ragCertified: boolean = false): string {
    const filledSegments = Math.floor(level / 10);
    const color = level >= 86 ? '#00ff88' : level >= 61 ? '#00d4ff' : level >= 31 ? '#ffaa00' : '#ff6b6b';

    return `
      <svg class="intelligence-hud" width="100%" height="32" viewBox="0 0 280 32" xmlns="http://www.w3.org/2000/svg">
        <pattern id="grid-${level}" width="10" height="10" patternUnits="userSpaceOnUse">
          <path d="M 10 0 L 0 0 0 10" fill="none" stroke="rgba(0,255,200,0.1)" stroke-width="0.5"/>
        </pattern>
        <rect width="280" height="32" fill="url(#grid-${level})" opacity="0.3"/>
        <rect x="1" y="1" width="278" height="30" fill="none" stroke="rgba(0,255,200,0.3)" stroke-width="1" rx="2"/>
        <line x1="1" y1="6" x2="6" y2="1" stroke="${color}" stroke-width="1.5" opacity="0.8"/>
        <line x1="274" y1="1" x2="279" y2="6" stroke="${color}" stroke-width="1.5" opacity="0.8"/>
        <line x1="1" y1="25" x2="6" y2="30" stroke="${color}" stroke-width="1.5" opacity="0.8"/>
        <line x1="274" y1="30" x2="279" y2="25" stroke="${color}" stroke-width="1.5" opacity="0.8"/>
        <rect x="4" y="4" width="45" height="24" fill="rgba(0,20,40,0.8)" stroke="rgba(0,255,200,0.2)" stroke-width="0.5"/>
        <text x="26.5" y="13" font-family="monospace" font-size="7" fill="rgba(0,255,200,0.7)" text-anchor="middle" font-weight="600">IQ</text>
        <text x="26.5" y="24" font-family="monospace" font-size="11" fill="${color}" text-anchor="middle" font-weight="700">${level}</text>
        <g transform="translate(54, 8)">
          ${Array.from({length: 10}, (_, i) => {
            const isFilled = i < filledSegments;
            const x = i * 18;
            return `
              <rect x="${x}" y="0" width="15" height="16"
                    fill="${isFilled ? color : 'rgba(20,30,45,0.6)'}"
                    stroke="${isFilled ? color : 'rgba(60,80,100,0.4)'}"
                    stroke-width="0.5"
                    opacity="${isFilled ? 0.9 : 0.3}"/>
              ${isFilled ? `<rect x="${x + 1}" y="1" width="13" height="2" fill="rgba(255,255,255,0.4)" opacity="0.6"/>` : ''}
            `;
          }).join('')}
        </g>
        <g transform="translate(238, 8)">
          ${ragCertified ? `
            <circle cx="8" cy="8" r="6" fill="none" stroke="#00ff88" stroke-width="1.5"/>
            <path d="M 5 8 L 7 10 L 11 6" fill="none" stroke="#00ff88" stroke-width="1.5" stroke-linecap="round"/>
            <text x="20" y="11" font-family="monospace" font-size="6" fill="#00ff88" font-weight="600">RAG</text>
          ` : `
            <circle cx="8" cy="8" r="6" fill="none" stroke="rgba(100,100,120,0.3)" stroke-width="1"/>
            <line x1="5" y1="5" x2="11" y2="11" stroke="rgba(100,100,120,0.3)" stroke-width="1"/>
            <line x1="11" y1="5" x2="5" y2="11" stroke="rgba(100,100,120,0.3)" stroke-width="1"/>
          `}
        </g>
        <line x1="54" y1="8" x2="54" y2="24" stroke="${color}" stroke-width="0.5" opacity="0.6">
          <animate attributeName="x1" from="54" to="234" dur="2s" repeatCount="indefinite"/>
          <animate attributeName="x2" from="54" to="234" dur="2s" repeatCount="indefinite"/>
        </line>
      </svg>
    `.trim();
  }

  private renderGenomePanel(user: UserEntity): string {
    if (user.type !== 'persona' && user.type !== 'agent') {
      return '';
    }

    const totalLayers = 4;
    const activeLayers = 2 + Math.floor(Math.random() * 3);

    const layerBars = Array.from({ length: totalLayers }, (_, i) => {
      const isActive = i < activeLayers;
      const activeClass = isActive ? 'active' : 'inactive';
      return `<div class="genome-layer ${activeClass}"></div>`;
    }).join('');

    const hasLearning = user.personaConfig?.trainingMode === 'learning';
    const isCloud = user.modelConfig?.provider !== 'ollama';
    const hasRAG = user.modelConfig?.ragCertified === true;
    const hasGenome = user.genomeId !== undefined;

    const diamond = `
      <div class="genome-diamond">
        <div class="diamond-cell top ${hasLearning ? 'active' : ''}"></div>
        <div class="diamond-cell right ${isCloud ? 'active' : ''}"></div>
        <div class="diamond-cell bottom ${hasRAG ? 'active' : ''}"></div>
        <div class="diamond-cell left ${hasGenome ? 'active' : ''}"></div>
      </div>
    `.trim();

    return `
      <div class="genome-panel">
        <div class="genome-label">GENOME</div>
        <div class="genome-bars">${layerBars}</div>
        ${diamond}
      </div>
    `.trim();
  }

  // === SELECTION HOOK (override base) ===
  protected override onItemClick(_item: UserEntity): void {
    // Handled by @click in renderItem template
  }
}
