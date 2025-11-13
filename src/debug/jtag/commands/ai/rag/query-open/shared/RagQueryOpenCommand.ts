/**
 * RAG Query Open Command - Abstract Base
 *
 * Opens a semantic similarity search and returns a handle for iteration
 */

import { CommandBase } from '../../../../../daemons/command-daemon/shared/CommandBase';
import type { RagQueryOpenParams, RagQueryOpenResult } from './RagQueryOpenTypes';

export abstract class RagQueryOpenCommand extends CommandBase<RagQueryOpenParams, RagQueryOpenResult> {
  getDescription(): string {
    return 'Open a semantic code search query and return handle for iteration';
  }
}
