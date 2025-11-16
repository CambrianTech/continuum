/**
 * RAG Query Close Command - Abstract Base
 *
 * Closes a query handle and releases resources
 */

import { CommandBase } from '../../../../../daemons/command-daemon/shared/CommandBase';
import type { RagQueryCloseParams, RagQueryCloseResult } from './RagQueryCloseTypes';

export abstract class RagQueryCloseCommand extends CommandBase<RagQueryCloseParams, RagQueryCloseResult> {
  getDescription(): string {
    return 'Close a query handle and cleanup resources';
  }
}
