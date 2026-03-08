/**
 * PersonaEngagementDecider - Determines if a persona should engage with a message
 *
 * Extracted from PersonaResponseGenerator to isolate dormancy/engagement logic.
 * Handles dormancy levels (active, mention-only, human-only) with @mention failsafe.
 */

import type { ProcessableMessage } from './QueueItemTypes';

export interface DormancyState {
  level: 'active' | 'mention-only' | 'human-only';
}

export class PersonaEngagementDecider {
  private personaName: string;
  private log: (message: string, ...args: any[]) => void;

  constructor(personaName: string, log: (message: string, ...args: any[]) => void) {
    this.personaName = personaName;
    this.log = log;
  }

  /**
   * Check if persona should respond to message based on dormancy level
   *
   * Dormancy filtering:
   * - Level 0 (active): Respond to everything
   * - Level 1 (mention-only): Only respond to @mentions
   * - Level 2 (human-only): Only respond to humans OR @mentions
   *
   * CRITICAL: @mentions ALWAYS work as failsafe - no sleep mode blocks mentions
   */
  shouldRespondToMessage(
    message: ProcessableMessage,
    dormancyState?: DormancyState
  ): boolean {
    if (!dormancyState) {
      return true;
    }

    const dormancyLevel = dormancyState.level;

    const mentionsPersona = message.content.text.includes(`@${this.personaName.toLowerCase()}`) ||
                            message.content.text.includes(`@${this.personaName}`);

    // FAILSAFE: @mentions ALWAYS wake - regardless of dormancy level
    if (mentionsPersona) {
      if (dormancyLevel !== 'active') {
        this.log(`✨ ${this.personaName}: @mention detected, waking from ${dormancyLevel} mode`);
      }
      return true;
    }

    if (dormancyLevel === 'active') {
      return true;
    }

    if (dormancyLevel === 'mention-only') {
      this.log(`💤 ${this.personaName}: Dormant (mention-only), skipping message`);
      return false;
    }

    if (dormancyLevel === 'human-only') {
      const isHumanSender = message.senderType === 'human';
      if (isHumanSender) {
        return true;
      }
      this.log(`💤 ${this.personaName}: Dormant (human-only), skipping AI message`);
      return false;
    }

    return true;
  }
}
