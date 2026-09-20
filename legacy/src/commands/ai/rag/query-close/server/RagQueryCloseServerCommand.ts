/**
 * RAG Query Close - Server Implementation
 *
 * Closes a query handle and frees resources
 */

import { RagQueryCloseCommand } from '../shared/RagQueryCloseCommand';
import type { RagQueryCloseParams, RagQueryCloseResult } from '../shared/RagQueryCloseTypes';
import type { JTAGContext } from '../../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../../daemons/command-daemon/shared/CommandBase';
import { closeQueryHandle } from '../../query-open/server/RagQueryOpenServerCommand';

export class RagQueryCloseServerCommand extends RagQueryCloseCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai/rag/query-close', context, subpath, commander);
  }

  async execute(params: RagQueryCloseParams): Promise<RagQueryCloseResult> {
    try {
      console.log('🔒 RAG Query Close: Closing handle', {
        queryHandle: params.queryHandle
      });

      // Close the handle
      const closed = closeQueryHandle(params.queryHandle);

      if (!closed) {
        console.log(`⚠️  Handle not found: ${params.queryHandle}`);
      } else {
        console.log(`✅ Handle closed: ${params.queryHandle}`);
      }

      return {
        success: true,
        closed,
        context: this.context,
        sessionId: params.sessionId
      };

    } catch (error) {
      console.error('❌ RAG Query Close failed:', error);
      return {
        success: false,
        error: error instanceof Error ? error.message : 'Unknown error',
        closed: false,
        context: this.context,
        sessionId: params.sessionId
      };
    }
  }
}
