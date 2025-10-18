/**
 * RAG Inspect Command - Shared Base
 *
 * Inspect RAG context building for debugging and validation
 */

import { CommandBase } from '../../../../../daemons/command-daemon/shared/CommandBase';
import type { RAGInspectParams, RAGInspectResult } from './RAGInspectTypes';

export abstract class RAGInspectCommand extends CommandBase<RAGInspectParams, RAGInspectResult> {
  getDescription(): string {
    return 'Inspect RAG context building for a persona in a room';
  }
}
