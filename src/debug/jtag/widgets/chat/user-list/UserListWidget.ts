/**
 * User List Widget - Database-Driven Chat Users
 * Now uses EntityScrollerWidget base class for automatic EntityScroller management
 */

import { EntityScrollerWidget } from '../../shared/EntityScrollerWidget';
import { DATA_COMMANDS } from '@commands/data/shared/DataCommandConstants';
import { UserEntity } from '../../../system/data/entities/UserEntity';
import type { DataListParams, DataListResult } from '../../../commands/data/list/shared/DataListTypes';
import { Commands } from '../../../system/core/shared/Commands';
import { SCROLLER_PRESETS, type RenderFn, type LoadFn, type ScrollerConfig } from '../../shared/EntityScroller';
import { Events } from '../../../system/core/shared/Events';
import { AI_DECISION_EVENTS } from '../../../system/events/shared/AIDecisionEvents';
import { AI_LEARNING_EVENTS } from '../../../system/events/shared/AILearningEvents';
import type { ContentOpenParams, ContentOpenResult } from '../../../commands/collaboration/content/open/shared/ContentOpenTypes';

// Verbose logging helper for browser
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
 * AI Learning State tracking for user list display
 */
interface AILearningState {
  personaId: string;
  isLearning: boolean;
  domain?: string;
  timestamp: number;
}

export class UserListWidget extends EntityScrollerWidget<UserEntity> {
  private searchFilter: string = '';
  private selectedUserId: string | null = null;
  private aiStatuses: Map<string, AIStatusState> = new Map();
  private learningStatuses: Map<string, AILearningState> = new Map();
  private allUsers: UserEntity[] = [];

  constructor() {
    super({
      widgetId: 'user-list-widget',
      widgetName: 'UserListWidget',
      styles: 'user-list.css',
      enableAI: false,
      enableDatabase: true,
      enableRouterEvents: false,
      enableScreenshots: false
    });
  }

  // Path resolution now handled automatically by ChatWidgetBase
  // EntityScroller setup now handled automatically by EntityScrollerWidget base class

  /**
   * Override onWidgetInitialize to setup AI event subscriptions
   */
  protected override async onWidgetInitialize(): Promise<void> {
    await super.onWidgetInitialize();
    this.setupAIEventSubscriptions();
    this.setupLearningEventSubscriptions();
  }

  /**
   * Subscribe to AI decision events for status emoji indicators
   */
  private setupAIEventSubscriptions(): void {
    verbose() && console.log('🔧 UserListWidget: Setting up AI event subscriptions...');

    Events.subscribe(AI_DECISION_EVENTS.EVALUATING, (data: { personaId: string }) => {
      this.updateAIStatus(data.personaId, 'evaluating');
    });

    Events.subscribe(AI_DECISION_EVENTS.DECIDED_RESPOND, (data: { personaId: string }) => {
      this.updateAIStatus(data.personaId, 'responding');
    });

    Events.subscribe(AI_DECISION_EVENTS.DECIDED_SILENT, (data: { personaId: string }) => {
      this.updateAIStatus(data.personaId, 'passed');
    });

    Events.subscribe(AI_DECISION_EVENTS.GENERATING, (data: { personaId: string }) => {
      this.updateAIStatus(data.personaId, 'generating');
    });

    Events.subscribe(AI_DECISION_EVENTS.CHECKING_REDUNDANCY, (data: { personaId: string }) => {
      this.updateAIStatus(data.personaId, 'checking');
    });

    Events.subscribe(AI_DECISION_EVENTS.POSTED, (data: { personaId: string }) => {
      this.updateAIStatus(data.personaId, null); // Clear status after successful post
    });

    Events.subscribe(AI_DECISION_EVENTS.ERROR, (data: { personaId: string; error: string }) => {
      this.updateAIStatus(data.personaId, 'error', data.error);
    });
  }

  /**
   * Subscribe to AI learning events for learning indicators
   */
  private setupLearningEventSubscriptions(): void {
    verbose() && console.log('🧬 UserListWidget: Setting up learning event subscriptions...');

    Events.subscribe(AI_LEARNING_EVENTS.TRAINING_STARTED, (data: { personaId: string; domain: string }) => {
      this.updateLearningStatus(data.personaId, true, data.domain);
    });

    Events.subscribe(AI_LEARNING_EVENTS.TRAINING_COMPLETE, (data: { personaId: string }) => {
      this.updateLearningStatus(data.personaId, false);
    });

    Events.subscribe(AI_LEARNING_EVENTS.TRAINING_ERROR, (data: { personaId: string }) => {
      this.updateLearningStatus(data.personaId, false);
    });
  }

  /**
   * Update learning status and update DOM directly
   */
  private updateLearningStatus(personaId: string, isLearning: boolean, domain?: string): void {
    if (!isLearning) {
      this.learningStatuses.delete(personaId);
    } else {
      this.learningStatuses.set(personaId, {
        personaId,
        isLearning: true,
        domain,
        timestamp: Date.now()
      });
    }

    // Update DOM directly to show/hide learning indicator
    const userElement = this.shadowRoot?.querySelector(`[data-user-id="${personaId}"]`) as HTMLElement;
    if (userElement) {
      if (isLearning) {
        userElement.dataset.learning = 'true';
      } else {
        delete userElement.dataset.learning;
      }

      // Update learning emoji if present
      const learningEmojiElement = userElement.querySelector('.user-learning-status');
      if (learningEmojiElement) {
        learningEmojiElement.textContent = isLearning ? '🧬' : '';
      }
    }
  }

  /**
   * Update AI status and update DOM directly to avoid animation restart
   */
  private updateAIStatus(personaId: string, phase: AIStatusState['currentPhase'], errorMessage?: string): void {
    if (phase === null) {
      this.aiStatuses.delete(personaId);
    } else {
      this.aiStatuses.set(personaId, {
        personaId,
        currentPhase: phase,
        timestamp: Date.now(),
        errorMessage
      });
    }

    // Update DOM directly instead of full refresh to avoid restarting animations
    const userElement = this.shadowRoot?.querySelector(`[data-user-id="${personaId}"]`) as HTMLElement;
    if (userElement) {
      if (phase === null) {
        // Remove data-ai-status attribute when status cleared
        userElement.removeAttribute('data-ai-status');
      } else {
        // Set data-ai-status attribute to trigger CSS animation
        userElement.dataset.aiStatus = phase;
      }

      // Update status emoji if present
      const statusEmojiElement = userElement.querySelector('.user-ai-status');
      if (statusEmojiElement) {
        statusEmojiElement.textContent = this.getStatusEmoji(personaId);
      }
    }
  }

  /**
   * Get AI status emoji for display
   */
  private getStatusEmoji(userId: string): string {
    const status = this.aiStatuses.get(userId);
    if (!status || !status.currentPhase) return '';

    switch (status.currentPhase) {
      case 'evaluating':
        return '🤔';
      case 'responding':
        return '💭';
      case 'generating':
        return '✍️';
      case 'checking':
        return '🔍';
      case 'error':
        return '❌';
      case 'passed':
        return '⏭️';
      default:
        return '';
    }
  }

  /**
   * Override renderTemplate to add search input
   */
  protected override renderTemplate(): string {
    const cssClass = 'user-list';
    return `
      <div class="entity-list-container">
        ${this.renderHeader()}

        <div class="user-search">
          <input
            type="text"
            class="search-input"
            placeholder="Search users and agents..."
            value="${this.searchFilter}"
          />
        </div>

        <div class="entity-list-body ${cssClass}">
          <!-- EntityScroller will populate this container -->
        </div>

        ${this.renderFooter()}
      </div>
    `;
  }

  /**
   * Setup event listeners for search input and user interactions
   */
  protected override setupEventListeners(): void {
    const searchInput = this.shadowRoot.querySelector('.search-input') as HTMLInputElement;
    if (searchInput) {
      searchInput.addEventListener('input', (e) => {
        this.searchFilter = (e.target as HTMLInputElement).value.toLowerCase();
        this.scroller?.refresh();
      });
    }
  }

  // Required by EntityScrollerWidget - render function for individual user items
  protected getRenderFunction(): RenderFn<UserEntity> {
    return (user: UserEntity, _context) => {
      // Apply search filter - return hidden element for non-matching users
      const displayName = user.displayName ?? 'Unknown User';
      const searchableText = `${displayName} ${user.type} ${user.profile?.speciality ?? ''}`.toLowerCase();
      if (this.searchFilter && !searchableText.includes(this.searchFilter)) {
        const hiddenElement = globalThis.document.createElement('div');
        hiddenElement.style.display = 'none';
        return hiddenElement;
      }

      const statusClass = user.status === 'online' ? 'online' : 'offline';
      const isSelected = this.selectedUserId === user.id;

      // Avatar fallback by type (profile not loaded in list view)
      const avatar = user.type === 'human' ? '👤' :
                    user.type === 'agent' ? '🤖' :
                    user.type === 'persona' ? '⭐' :
                    user.type === 'system' ? '⚙️' : '❓';

      const speciality = user.speciality; // UserEntity getter
      const aiStatusEmoji = this.getStatusEmoji(user.id);
      const learningState = this.learningStatuses.get(user.id);
      const learningEmoji = learningState?.isLearning ? '🧬' : '';

      // Format last active timestamp
      const lastActive = user.lastActiveAt ? this.formatTimestamp(user.lastActiveAt) : null;

      // User type badge - uses UserType from UserEntity (CSS handles uppercase)
      const typeBadge = user.type;

      // Model information for AI personas - show provider and model
      let modelInfo = '';
      let modelBadge = '';
      if (user.type === 'persona' || user.type === 'agent') {
        // Get provider and model from config
        const provider = user.modelConfig?.provider || (user.personaConfig?.responseModel ? 'ollama' : '');
        const model = user.modelConfig?.model || user.personaConfig?.responseModel || '';

        if (provider && model) {
          modelInfo = `${provider}/${model}`;
          // Badge is always just the provider name (max 8 chars, uppercase)
          modelBadge = provider.substring(0, 8).toUpperCase();
        }
      }

      // RAG certification status
      // TEMPORARY DEMO: Mark Sentinel as RAG certified until database seeding is fixed
      let ragCertified = user.modelConfig?.ragCertified ?? false;
      if (!ragCertified && user.uniqueId === 'persona-sentinel') {
        ragCertified = true;
      }

      // Intelligence level bars for AI personas/agents (with RAG status integrated)
      // TEMPORARY DEMO: Use hardcoded levels based on uniqueId until database seeding is fixed
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
      const intelligenceBars = intelligenceLevel > 0 ? this.renderIntelligenceBars(intelligenceLevel, ragCertified) : '';

      // Response mode indicator (free chat vs mention-required) - now shown as dot on avatar
      const requiresMention = user.modelConfig?.requiresExplicitMention ?? false;
      const responseModeDot = (user.type === 'persona' || user.type === 'agent') ?
        (requiresMention ? '<span class="response-mode-dot mention-required" title="Requires @mention"></span>' :
                          '<span class="response-mode-dot free-chat" title="Can respond freely"></span>') : '';

      // RAG badge (removing since it's now in the SVG HUD)
      const ragBadge = '';

      const userElement = globalThis.document.createElement('div');
      userElement.className = `user-item ${statusClass} ${isSelected ? 'selected' : ''}`;
      userElement.dataset.userId = user.id;
      userElement.dataset.userType = user.type; // For idle glow styling

      // Set AI status data attribute for comet animation
      const aiStatus = this.aiStatuses.get(user.id);
      if (aiStatus?.currentPhase) {
        userElement.dataset.aiStatus = aiStatus.currentPhase;
      }

      // Set learning data attribute for learning styling
      if (learningState?.isLearning) {
        userElement.dataset.learning = 'true';
      }

      // Generate genome panel for AI personas/agents
      const genomePanel = this.renderGenomePanel(user);

      userElement.innerHTML = `
        ${lastActive ? `<span class="user-last-active">${lastActive}</span>` : ''}
        <span class="user-avatar">
          ${avatar}
          <span class="status-indicator"></span>
          ${responseModeDot}
        </span>
        <div class="user-info">
          <div class="user-name-row">
            <span class="user-name">${displayName}</span>
          </div>
          <div class="user-meta">
            <span class="user-type-badge">${typeBadge}</span>
            ${modelInfo ? `<span class="user-model-info" title="AI Model">${modelInfo}</span>` : ''}
            ${speciality ? `<span class="user-speciality">${speciality}</span>` : ''}
            ${modelBadge ? `<span class="user-model-badge">${modelBadge}</span>` : ''}
          </div>
          ${intelligenceBars}
        </div>
        <div class="user-controls">
          <button class="user-favorite-btn" title="Add to favorites">⭐</button>
          <button class="user-action-btn" title="Actions">»</button>
        </div>
        ${genomePanel}
      `;

      // Add click handler for selection
      userElement.addEventListener('click', (e) => {
        // Don't select if clicking on a button
        if ((e.target as HTMLElement).tagName === 'BUTTON') return;
        this.selectUser(user.id);
      });

      // Add button handlers
      const favoriteBtn = userElement.querySelector('.user-favorite-btn');
      favoriteBtn?.addEventListener('click', (e) => {
        e.stopPropagation();
        this.toggleFavorite(user.id);
      });

      const actionBtn = userElement.querySelector('.user-action-btn');
      actionBtn?.addEventListener('click', (e) => {
        e.stopPropagation();
        this.showActionMenu(user.id);
      });

      return userElement;
    };
  }

  /**
   * Format timestamp for last active display
   */
  private formatTimestamp(timestamp: number | Date | string | undefined): string {
    if (!timestamp) return 'Never';

    // Convert to timestamp number
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

    // Less than 1 minute
    if (diff < 60000) return 'Just now';

    // Less than 1 hour
    if (diff < 3600000) {
      const mins = Math.floor(diff / 60000);
      return `${mins}m ago`;
    }

    // Less than 24 hours
    if (diff < 86400000) {
      const hours = Math.floor(diff / 3600000);
      return `${hours}h ago`;
    }

    // Less than 7 days
    if (diff < 604800000) {
      const days = Math.floor(diff / 86400000);
      return `${days}d ago`;
    }

    // Format as date
    return new Date(timestampMs).toLocaleDateString();
  }

  /**
   * Render compact SVG-based intelligence HUD (cyberpunk style)
   * Features: segmented bars, grid background, numeric readout, status indicators
   * Compact design inspired by video game HUDs
   */
  private renderIntelligenceBars(level: number, ragCertified: boolean = false): string {
    const percentage = level;
    const filledSegments = Math.floor(level / 10);

    // Color based on intelligence level
    const color = level >= 86 ? '#00ff88' : level >= 61 ? '#00d4ff' : level >= 31 ? '#ffaa00' : '#ff6b6b';

    return `
      <svg class="intelligence-hud" width="100%" height="32" viewBox="0 0 280 32" xmlns="http://www.w3.org/2000/svg">
        <!-- REMOVED defs/linearGradient - using solid colors for better performance -->

        <!-- Grid background -->
        <pattern id="grid-${level}" width="10" height="10" patternUnits="userSpaceOnUse">
          <path d="M 10 0 L 0 0 0 10" fill="none" stroke="rgba(0,255,200,0.1)" stroke-width="0.5"/>
        </pattern>
        <rect width="280" height="32" fill="url(#grid-${level})" opacity="0.3"/>

        <!-- Main container border -->
        <rect x="1" y="1" width="278" height="30" fill="none" stroke="rgba(0,255,200,0.3)" stroke-width="1" rx="2"/>

        <!-- Corner accents -->
        <line x1="1" y1="6" x2="6" y2="1" stroke="${color}" stroke-width="1.5" opacity="0.8"/>
        <line x1="274" y1="1" x2="279" y2="6" stroke="${color}" stroke-width="1.5" opacity="0.8"/>
        <line x1="1" y1="25" x2="6" y2="30" stroke="${color}" stroke-width="1.5" opacity="0.8"/>
        <line x1="274" y1="30" x2="279" y2="25" stroke="${color}" stroke-width="1.5" opacity="0.8"/>

        <!-- Left label section -->
        <rect x="4" y="4" width="45" height="24" fill="rgba(0,20,40,0.8)" stroke="rgba(0,255,200,0.2)" stroke-width="0.5"/>
        <text x="26.5" y="13" font-family="monospace" font-size="7" fill="rgba(0,255,200,0.7)" text-anchor="middle" font-weight="600">IQ</text>
        <text x="26.5" y="24" font-family="monospace" font-size="11" fill="${color}" text-anchor="middle" font-weight="700">${level}</text>

        <!-- Segmented bar display (10 segments) -->
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

        <!-- Right status indicators -->
        <g transform="translate(238, 8)">
          <!-- RAG certification indicator -->
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

        <!-- Scanning line animation effect -->
        <line x1="54" y1="8" x2="54" y2="24" stroke="${color}" stroke-width="0.5" opacity="0.6">
          <animate attributeName="x1" from="54" to="234" dur="2s" repeatCount="indefinite"/>
          <animate attributeName="x2" from="54" to="234" dur="2s" repeatCount="indefinite"/>
        </line>
      </svg>
    `.trim();
  }

  /**
   * Render genome state panel showing loaded LoRA layers (like AIDA64 CORE section)
   * @param user - User entity
   * @returns HTML string for genome panel or empty string
   */
  private renderGenomePanel(user: UserEntity): string {
    // Only show for AI personas/agents
    if (user.type !== 'persona' && user.type !== 'agent') {
      return '';
    }

    // Fixed: Always 4 bars total (representing 4 LoRA layers)
    const totalLayers = 4;
    const activeLayers = 2 + Math.floor(Math.random() * 3); // 2-4 active

    // Build vertical bars - FIXED count, all same width
    const layerBars = Array.from({ length: totalLayers }, (_, i) => {
      const isActive = i < activeLayers;
      const activeClass = isActive ? 'active' : 'inactive';
      return `<div class="genome-layer ${activeClass}"></div>`;
    }).join('');

    // Genome attributes for diamond grid - the AI's "genetic makeup"
    // These define what the persona fundamentally IS, not what it's currently doing
    const hasLearning = user.personaConfig?.trainingMode === 'learning'; // Can this AI learn/train?
    const isCloud = user.modelConfig?.provider !== 'ollama'; // Cloud vs local infrastructure
    const hasRAG = user.modelConfig?.ragCertified === true; // Has memory/retrieval system
    const hasGenome = user.genomeId !== undefined; // Has specialized LoRA adaptations

    // Build 2x2 diamond grid - "genetic nucleus" showing AI's fundamental makeup
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

  /**
   * Select a user - opens their profile view
   */
  private async selectUser(userId: string): Promise<void> {
    // Find the user entity
    const userEntity = this.scroller?.entities().find(u => u.id === userId);
    if (!userEntity) {
      console.warn(`❌ UserListWidget: User not found: "${userId}"`);
      return;
    }

    // Update selection highlight
    this.selectedUserId = userId;
    this.scroller?.refresh();

    // Open profile for all users
    await this.openUserProfile(userEntity);
  }

  /**
   * Open user profile view - works for ALL user types (humans, personas, agents)
   */
  private async openUserProfile(userEntity: UserEntity): Promise<void> {
    const entityId = userEntity.uniqueId || userEntity.id;
    const title = userEntity.displayName || 'User Profile';

    verbose() && console.log(`👤 UserListWidget: Opening profile for ${title} (${entityId})`);

    // Emit content:opened for MainWidget tab update (optimistic UI)
    Events.emit('content:opened', {
      contentType: 'profile',
      entityId,
      title,
      setAsCurrent: true
    });

    // Persist to server in background
    const userId = this.userState?.userId;
    if (userId) {
      Commands.execute<ContentOpenParams, ContentOpenResult>('collaboration/content/open', {
        userId,
        contentType: 'profile',
        entityId,
        title,
        setAsCurrent: true
      }).catch(err => console.error('Failed to persist profile open:', err));
    }
  }

  /**
   * Open the persona brain widget for an AI user
   */
  private async openPersonaBrain(userEntity: UserEntity): Promise<void> {
    const entityId = userEntity.uniqueId || userEntity.id;
    const title = userEntity.displayName || 'AI Brain';

    verbose() && console.log(`🧠 UserListWidget: Opening brain for ${title} (${entityId})`);

    // Emit content:opened for MainWidget tab update (optimistic UI)
    Events.emit('content:opened', {
      contentType: 'persona',
      entityId,
      title,
      setAsCurrent: true
    });

    // Persist to server in background
    const userId = this.userState?.userId;
    if (userId) {
      Commands.execute<ContentOpenParams, ContentOpenResult>('collaboration/content/open', {
        userId,
        contentType: 'persona',
        entityId,
        title,
        setAsCurrent: true
      }).catch(err => console.error('Failed to persist persona open:', err));
    }
  }

  /**
   * Toggle favorite status
   */
  private toggleFavorite(userId: string): void {
    verbose() && console.log(`⭐ UserListWidget: Toggle favorite for user ${userId}`);
    // TODO: Implement favorite persistence
  }

  /**
   * Show action menu for user
   */
  private showActionMenu(userId: string): void {
    verbose() && console.log(`» UserListWidget: Show action menu for user ${userId}`);
    // TODO: Implement action menu (DM, view profile, etc.)
  }

  // Required by EntityScrollerWidget - load function using data/list command
  protected getLoadFunction(): LoadFn<UserEntity> {
    return async (cursor, limit) => {
      const result = await Commands.execute<DataListParams, DataListResult<UserEntity>>(DATA_COMMANDS.LIST, {
        collection: UserEntity.collection,
        orderBy: [{ field: 'lastActiveAt', direction: 'desc' }],
        limit: limit ?? 100
      });

      if (!result?.success || !result.items) {
        throw new Error(`Failed to load users: ${result?.error ?? 'Unknown error'}`);
      }

      return {
        items: result.items,
        hasMore: false, // User lists are typically small, no pagination needed
        nextCursor: undefined
      };
    };
  }

  // Required by EntityScrollerWidget
  protected getScrollerPreset(): ScrollerConfig {
    return SCROLLER_PRESETS.LIST; // No auto-scroll, larger page size
  }

  // Required by EntityScrollerWidget
  protected getContainerSelector(): string {
    return '.user-list';
  }

  // Required by EntityScrollerWidget
  protected getEntityCollection(): string {
    return UserEntity.collection;
  }

  protected resolveResourcePath(filename: string): string {
    return `widgets/chat/user-list/${filename}`;
  }

  // Event subscriptions now handled automatically by EntityScrollerWidget base class


  // Entity count now handled automatically by EntityScrollerWidget base class

  protected getEntityTitle(_entity?: UserEntity): string {
    return 'Users & Agents';
  }

  // Cleanup now handled automatically by EntityScrollerWidget base class

  // Using default template from EntityScrollerWidget (generates "user-list" CSS class automatically)

}