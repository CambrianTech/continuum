/**
 * Bag of Words Server Command
 *
 * Generic coordination primitive for multi-agent interactions.
 * Pure orchestration - no domain-specific logic (chat, game, etc).
 *
 * Classical ML/NLP term: collection of items interacting without fixed order.
 * Strategies define interaction patterns, but implementation is generic.
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
   * Execute bag-of-words coordination
   *
   * Pure orchestration - returns coordination metadata.
   * Caller is responsible for implementing actual interaction logic.
   */
  async execute(params: BagOfWordsParams): Promise<BagOfWordsResult> {
    const sessionId = generateUUID();
    const startedAt = new Date();

    // Validate
    if (!params.roomId) {
      throw new Error('roomId (context) is required');
    }
    if (!params.personaIds || params.personaIds.length === 0) {
      throw new Error('personaIds (participants) required');
    }

    const strategy = params.strategy ?? 'free-for-all';

    console.log(`🎯 BAG-OF-WORDS: Coordinating ${params.personaIds.length} participants`);
    console.log(`   Context: ${params.roomId.slice(0, 8)}`);
    console.log(`   Strategy: ${strategy}`);
    console.log(`   Max turns: ${params.maxTurns ?? 'unlimited'}`);

    // Build participant metadata (generic - no entity lookups)
    const participants = params.personaIds.map((personaId, index) => ({
      personaId,
      personaName: `Participant-${index + 1}`, // Generic naming
      messagesCount: 0
    }));

    // Return coordination metadata
    // Actual interaction logic handled by caller (recipe, chat command, etc)
    return createBagOfWordsResult(params.context, sessionId, {
      messageCount: 0,
      participants,
      status: 'active',
      startedAt
    });
  }
}
