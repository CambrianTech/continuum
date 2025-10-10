/**
 * Bag of Words Server Command
 *
 * Orchestrates multi-persona conversations in chat rooms.
 * Handles room setup, persona participation, and conversation flow.
 */

import { BagOfWordsCommand } from '../shared/BagOfWordsCommand';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { BagOfWordsParams, BagOfWordsResult } from '../shared/BagOfWordsTypes';
import { createBagOfWordsResult } from '../shared/BagOfWordsTypes';
import { generateUUID } from '../../../../system/core/types/CrossPlatformUUID';

export class BagOfWordsServerCommand extends BagOfWordsCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('bag-of-words', context, subpath, commander);
  }

  /**
   * Execute bag-of-words conversation orchestration
   */
  async execute(params: BagOfWordsParams): Promise<BagOfWordsResult> {
    const conversationId = generateUUID();
    const startedAt = new Date();

    // Validate parameters
    if (!params.roomId) {
      throw new Error('roomId is required for bag-of-words');
    }

    if (!params.personaIds || params.personaIds.length === 0) {
      throw new Error('At least one personaId is required');
    }

    console.log(`🎭 BAG-OF-WORDS: Starting conversation in room ${params.roomId} with ${params.personaIds.length} personas`);

    // Default values
    const strategy = params.strategy ?? 'free-for-all';
    const responseDelay = params.responseDelay ?? 1000;
    const includeHumanObserver = params.includeHumanObserver ?? true;

    // TODO: Phase 1 - Room setup
    // 1. Verify room exists
    // 2. Add personas as participants
    // 3. Optionally add human as observer

    // TODO: Phase 2 - Send initial message
    if (params.initialMessage) {
      console.log(`📨 BAG-OF-WORDS: Sending initial message: "${params.initialMessage}"`);
      // Send initial message from system user
      // This will trigger persona responses via existing chat/send → recipe system
    }

    // TODO: Phase 3 - Monitor conversation
    // For now, just return success - actual conversation happens via recipe system
    // Personas will respond naturally based on existing ai/should-respond logic

    const participants = params.personaIds.map((personaId, index) => ({
      personaId,
      personaName: `Persona${index + 1}`, // TODO: Look up actual persona names
      messagesCount: 0 // TODO: Track during conversation
    }));

    console.log(`✅ BAG-OF-WORDS: Conversation session ${conversationId} initialized`);
    console.log(`   Strategy: ${strategy}`);
    console.log(`   Max turns: ${params.maxTurns ?? 'unlimited'}`);
    console.log(`   Topic: ${params.topic ?? 'unspecified'}`);

    return createBagOfWordsResult(params.context, conversationId, {
      messageCount: 0, // TODO: Track during conversation
      participants,
      status: 'active',
      startedAt
    });
  }
}
