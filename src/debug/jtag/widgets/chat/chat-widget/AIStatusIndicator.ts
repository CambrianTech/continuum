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
  private dismissedErrors = new Map<string, number>(); // errorKey -> dismissTimestamp
  private dismissDuration = 5 * 60 * 1000; // Don't show same error for 5 minutes after dismissal

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
    // Auto-remove immediately (message will appear in chat)
    setTimeout(() => {
      this.removeStatus(data.personaId);
    }, this.removeTimeout);
  }

  /**
   * Handle ERROR - AI encountered error, show error message with close button
   * Errors stay visible until user dismisses them manually
   */
  onError(data: AIErrorEventData): void {
    // Create a key to track this specific error
    const errorKey = `${data.personaId}:${data.error}`;

    // Check if this error was recently dismissed
    const dismissedTime = this.dismissedErrors.get(errorKey);
    if (dismissedTime && (Date.now() - dismissedTime) < this.dismissDuration) {
      // Error was recently dismissed, don't show it again
      return;
    }

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
   * Only creates full banners for ERROR states - other states tracked for header emoji display
   */
  private updateStatus(personaId: UUID, state: AIStatusState): void {
    const existing = this.activeStatuses.get(personaId);

    if (existing) {
      // Update existing status
      existing.currentPhase = state.currentPhase;
      existing.timestamp = state.timestamp;
      existing.errorMessage = state.errorMessage;

      // Only update element if it's an error (or remove element if changing from error to non-error)
      if (state.currentPhase === 'error') {
        if (existing.element) {
          this.updateStatusElement(existing.element, state);
        } else if (this.container) {
          // Create error banner if it doesn't exist
          const element = this.createStatusElement(state);
          existing.element = element;
          this.container.appendChild(element);
        }
      } else if (existing.element) {
        // Remove banner element for non-error states
        existing.element.remove();
        existing.element = undefined;
      }
    } else {
      // Create new status (only create banner element for errors)
      this.activeStatuses.set(personaId, state);

      if (state.currentPhase === 'error' && this.container) {
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

    if (status?.element) {
      // If this is an error, track it as dismissed
      if (status.currentPhase === 'error' && status.errorMessage) {
        const errorKey = `${personaId}:${status.errorMessage}`;
        this.dismissedErrors.set(errorKey, Date.now());

        // Clean up old dismissed errors (older than dismissDuration)
        const now = Date.now();
        for (const [key, time] of this.dismissedErrors.entries()) {
          if ((now - time) > this.dismissDuration) {
            this.dismissedErrors.delete(key);
          }
        }
      }

      // Fade out animation
      status.element.style.opacity = '0';
      setTimeout(() => {
        status.element?.remove();
      }, 300);
    }

    this.activeStatuses.delete(personaId);
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

  /**
   * Get count of error statuses (for header button display)
   */
  getErrorCount(): number {
    let count = 0;
    for (const status of this.activeStatuses.values()) {
      if (status.currentPhase === 'error') {
        count++;
      }
    }
    return count;
  }

  /**
   * Get status emoji for a specific persona (for header display)
   * Returns null if no active status
   */
  getStatusEmoji(personaId: UUID): string | null {
    const status = this.activeStatuses.get(personaId);
    if (!status || !status.currentPhase) return null;

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
      default:
        return '⏭️'; // passed/silent
    }
  }

  /**
   * Get all current AI statuses (for debugging or bulk operations)
   */
  getAllStatuses(): Map<UUID, AIStatusState> {
    return new Map(this.activeStatuses);
  }
}
