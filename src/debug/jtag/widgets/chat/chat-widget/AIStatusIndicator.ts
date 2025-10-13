/**
 * AI Status Indicator - Manages ephemeral AI decision status display
 *
 * Displays subtle, temporary status rows for AI thinking/responding/deciding
 * These are NOT chat messages - they're real-time UI feedback that disappears after AI decision
 *
 * Visual Design:
 * - Small, subtle row above where AI message will appear
 * - Shows persona name + current phase (thinking, responding, etc.)
 * - Animated pulse/spinner for active states
 * - Auto-removes after AI posts or decides to stay silent
 */

import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import {
  AI_DECISION_EVENTS,
  type AIEvaluatingEventData,
  type AIDecidedRespondEventData,
  type AIDecidedSilentEventData,
  type AIGeneratingEventData,
  type AICheckingRedundancyEventData,
  type AIPostedEventData,
  type AIErrorEventData
} from '../../../system/events/shared/AIDecisionEvents';

/**
 * AI Status State - Tracks current decision phase for each AI
 */
interface AIStatusState {
  personaId: UUID;
  personaName: string;
  roomId: UUID;
  currentPhase: 'evaluating' | 'responding' | 'generating' | 'checking' | 'error' | null;
  timestamp: number;
  errorMessage?: string; // Error message for display
  element?: HTMLElement; // DOM element for this status indicator
}

/**
 * AI Status Indicator Manager
 * Manages ephemeral status indicators for multiple AIs simultaneously
 */
export class AIStatusIndicator {
  private activeStatuses = new Map<UUID, AIStatusState>(); // personaId -> status
  private container?: HTMLElement;
  private removeTimeout = 2000; // Auto-remove after 2 seconds

  constructor(container?: HTMLElement) {
    this.container = container;
  }

  /**
   * Set container where status indicators should be rendered
   */
  setContainer(container: HTMLElement): void {
    this.container = container;
  }

  /**
   * Handle EVALUATING event - AI started thinking
   */
  onEvaluating(data: AIEvaluatingEventData): void {
    this.updateStatus(data.personaId, {
      personaId: data.personaId,
      personaName: data.personaName,
      roomId: data.roomId,
      currentPhase: 'evaluating',
      timestamp: data.timestamp
    });
  }

  /**
   * Handle DECIDED_RESPOND - AI will respond, show "responding" status
   */
  onDecidedRespond(data: AIDecidedRespondEventData): void {
    this.updateStatus(data.personaId, {
      personaId: data.personaId,
      personaName: data.personaName,
      roomId: data.roomId,
      currentPhase: 'responding',
      timestamp: data.timestamp
    });
  }

  /**
   * Handle DECIDED_SILENT - AI won't respond, remove status after delay
   */
  onDecidedSilent(data: AIDecidedSilentEventData): void {
    // Brief "passed" indication, then remove
    this.updateStatus(data.personaId, {
      personaId: data.personaId,
      personaName: data.personaName,
      roomId: data.roomId,
      currentPhase: null,
      timestamp: data.timestamp
    });

    // Auto-remove after delay
    setTimeout(() => {
      this.removeStatus(data.personaId);
    }, this.removeTimeout);
  }

  /**
   * Handle GENERATING - AI is generating response text
   */
  onGenerating(data: AIGeneratingEventData): void {
    this.updateStatus(data.personaId, {
      personaId: data.personaId,
      personaName: data.personaName,
      roomId: data.roomId,
      currentPhase: 'generating',
      timestamp: data.timestamp
    });
  }

  /**
   * Handle CHECKING_REDUNDANCY - AI is checking if response is redundant
   */
  onCheckingRedundancy(data: AICheckingRedundancyEventData): void {
    this.updateStatus(data.personaId, {
      personaId: data.personaId,
      personaName: data.personaName,
      roomId: data.roomId,
      currentPhase: 'checking',
      timestamp: data.timestamp
    });
  }

  /**
   * Handle POSTED - AI posted response, remove status after delay
   */
  onPosted(data: AIPostedEventData): void {
    console.log(`🔧 AIStatusIndicator: onPosted called for ${data.personaName} (${data.personaId})`);
    console.log(`🔧 AIStatusIndicator: Current active statuses:`, Array.from(this.activeStatuses.keys()));

    // Auto-remove immediately (message will appear in chat)
    setTimeout(() => {
      console.log(`🔧 AIStatusIndicator: Removing status for ${data.personaName} after ${this.removeTimeout}ms`);
      this.removeStatus(data.personaId);
    }, this.removeTimeout);
  }

  /**
   * Handle ERROR - AI encountered error, show error message with close button
   * Errors stay visible until user dismisses them manually
   */
  onError(data: AIErrorEventData): void {
    // Show error status with message - no auto-remove, user must dismiss
    this.updateStatus(data.personaId, {
      personaId: data.personaId,
      personaName: data.personaName,
      roomId: data.roomId,
      currentPhase: 'error',
      timestamp: data.timestamp,
      errorMessage: data.error
    });

    // No auto-remove - user must click X button to dismiss
  }

  /**
   * Update or create status indicator for AI
   */
  private updateStatus(personaId: UUID, state: AIStatusState): void {
    const existing = this.activeStatuses.get(personaId);

    if (existing) {
      // Update existing status
      existing.currentPhase = state.currentPhase;
      existing.timestamp = state.timestamp;

      if (existing.element) {
        this.updateStatusElement(existing.element, state);
      }
    } else {
      // Create new status
      this.activeStatuses.set(personaId, state);

      if (this.container) {
        const element = this.createStatusElement(state);
        state.element = element;
        this.container.appendChild(element);
      }
    }
  }

  /**
   * Remove status indicator for AI
   */
  private removeStatus(personaId: UUID): void {
    const status = this.activeStatuses.get(personaId);
    console.log(`🔧 AIStatusIndicator: removeStatus called for ${personaId}, found status:`, !!status);

    if (status?.element) {
      console.log(`🔧 AIStatusIndicator: Removing element for ${status.personaName}`);
      // Fade out animation
      status.element.style.opacity = '0';
      setTimeout(() => {
        status.element?.remove();
        console.log(`✅ AIStatusIndicator: Element removed for ${status.personaName}`);
      }, 300);
    }

    this.activeStatuses.delete(personaId);
    console.log(`🔧 AIStatusIndicator: Deleted from activeStatuses. Remaining count: ${this.activeStatuses.size}`);
  }

  /**
   * Create DOM element for status indicator
   */
  private createStatusElement(state: AIStatusState): HTMLElement {
    const element = document.createElement('div');
    element.className = 'ai-status-indicator';
    element.setAttribute('data-persona-id', state.personaId);

    this.updateStatusElement(element, state);

    return element;
  }

  /**
   * Update status element content based on current phase
   */
  private updateStatusElement(element: HTMLElement, state: AIStatusState): void {
    const { personaName, currentPhase, errorMessage, personaId } = state;

    let icon = '';
    let text = '';
    let className = 'ai-status-indicator';

    switch (currentPhase) {
      case 'evaluating':
        icon = '🤔';
        text = `${personaName} is thinking...`;
        className += ' ai-status-thinking';
        break;
      case 'responding':
        icon = '💭';
        text = `${personaName} will respond`;
        className += ' ai-status-responding';
        break;
      case 'generating':
        icon = '✍️';
        text = `${personaName} is generating...`;
        className += ' ai-status-generating';
        break;
      case 'checking':
        icon = '🔍';
        text = `${personaName} is reviewing...`;
        className += ' ai-status-checking';
        break;
      case 'error':
        icon = '❌';
        text = `${personaName} error: ${errorMessage || 'Unknown error'}`;
        className += ' ai-status-error';
        break;
      default:
        icon = '⏭️';
        text = `${personaName} passed`;
        className += ' ai-status-silent';
    }

    element.className = className;

    // Always show close button for manual dismissal
    element.innerHTML = `
      <span class="ai-status-icon">${icon}</span>
      <span class="ai-status-text">${text}</span>
      <button class="ai-status-close" data-persona-id="${personaId}" title="Dismiss">×</button>
    `;

    // Add click handler for close button
    const closeButton = element.querySelector('.ai-status-close');
    if (closeButton) {
      closeButton.addEventListener('click', () => {
        this.removeStatus(personaId);
      });
    }
  }

  /**
   * Clear all status indicators (e.g., when switching rooms)
   */
  clearAll(): void {
    for (const [personaId] of this.activeStatuses) {
      this.removeStatus(personaId);
    }
  }

  /**
   * Get count of active AI statuses
   */
  getActiveCount(): number {
    return this.activeStatuses.size;
  }
}
