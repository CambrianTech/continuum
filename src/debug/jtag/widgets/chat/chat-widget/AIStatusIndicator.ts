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

import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type {
  AIEvaluatingEventData,
  AIDecidedRespondEventData,
  AIDecidedSilentEventData,
  AIGeneratingEventData,
  AICheckingRedundancyEventData,
  AIPostedEventData,
  AIErrorEventData
} from '@system/events/shared/AIDecisionEvents';

/**
 * AI Status Phase - All possible AI decision phases
 */
type AIStatusPhase = 'evaluating' | 'responding' | 'generating' | 'checking' | 'error' | 'insufficient_funds' | 'rate_limited' | null;

/**
 * Phase display config - SINGLE SOURCE OF TRUTH for phase visualization
 */
const PHASE_CONFIG: Record<NonNullable<AIStatusPhase>, { emoji: string; labelTemplate: string; cssClass: string }> = {
  'evaluating': { emoji: '🤔', labelTemplate: '{name} is thinking...', cssClass: 'ai-status-thinking' },
  'responding': { emoji: '💭', labelTemplate: '{name} will respond', cssClass: 'ai-status-responding' },
  'generating': { emoji: '✍️', labelTemplate: '{name} is generating...', cssClass: 'ai-status-generating' },
  'checking': { emoji: '🔍', labelTemplate: '{name} is reviewing...', cssClass: 'ai-status-checking' },
  'error': { emoji: '❌', labelTemplate: '{name} error: {error}', cssClass: 'ai-status-error' },
  'insufficient_funds': { emoji: '💸', labelTemplate: '{name} out of funds', cssClass: 'ai-status-funds' },
  'rate_limited': { emoji: '⏳', labelTemplate: '{name} rate limited', cssClass: 'ai-status-rate-limited' },
};

// Default for null/passed state
const PASSED_CONFIG = { emoji: '⏭️', labelTemplate: '{name} passed', cssClass: 'ai-status-silent' };

/**
 * AI Status State - Tracks current decision phase for each AI
 */
interface AIStatusState {
  personaId: UUID;
  personaName: string;
  roomId: UUID;
  currentPhase: AIStatusPhase;
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
   * Handle adapter status event (insufficient_funds, rate_limited)
   * Shows funding/rate limit status for specific providers
   */
  onAdapterStatus(data: { providerId: string; status: string; message: string; timestamp: number }, personaId: UUID, personaName: string, roomId: UUID): void {
    if (data.status === 'insufficient_funds' || data.status === 'rate_limited') {
      this.updateStatus(personaId, {
        personaId,
        personaName,
        roomId,
        currentPhase: data.status as 'insufficient_funds' | 'rate_limited',
        timestamp: data.timestamp,
        errorMessage: data.message
      });
    }
  }

  /**
   * Handle ERROR - AI encountered error, show error message with close button
   * Errors stay visible until user dismisses them manually
   * Detects funding/quota errors to show 💸 instead of ❌
   */
  onError(data: AIErrorEventData): void {
    // Detect funding/quota errors to show appropriate indicator
    const errorLower = (data.error || '').toLowerCase();
    const isFundingError = errorLower.includes('insufficient_quota') ||
                          errorLower.includes('quota') ||
                          errorLower.includes('credits') ||
                          errorLower.includes('billing') ||
                          errorLower.includes('spending limit') ||
                          errorLower.includes('insufficient funds');
    const isRateLimited = errorLower.includes('rate limit') ||
                         (errorLower.includes('429') && !isFundingError);

    // Determine the appropriate phase based on error type
    const phase: 'error' | 'insufficient_funds' | 'rate_limited' =
      isFundingError ? 'insufficient_funds' :
      isRateLimited ? 'rate_limited' : 'error';

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
      currentPhase: phase,  // Use detected phase: 'error' | 'insufficient_funds' | 'rate_limited'
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

    // Use single source of truth config
    const config = currentPhase ? PHASE_CONFIG[currentPhase] : PASSED_CONFIG;
    const icon = config.emoji;
    const text = config.labelTemplate
      .replace('{name}', personaName)
      .replace('{error}', errorMessage || 'Unknown error');
    const className = `ai-status-indicator ${config.cssClass}`;

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

    // Use single source of truth config
    const config = PHASE_CONFIG[status.currentPhase];
    return config?.emoji ?? PASSED_CONFIG.emoji;
  }

  /**
   * Get all current AI statuses (for debugging or bulk operations)
   */
  getAllStatuses(): Map<UUID, AIStatusState> {
    return new Map(this.activeStatuses);
  }
}
