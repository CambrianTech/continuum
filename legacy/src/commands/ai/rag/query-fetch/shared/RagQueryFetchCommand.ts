/**
 * RAG Query Fetch Command - Abstract Base
 *
 * Fetches results from an open query handle at any position
 * Supports bidirectional navigation and random access
 */

import { CommandBase } from '../../../../../daemons/command-daemon/shared/CommandBase';
import type { RagQueryFetchParams, RagQueryFetchResult } from './RagQueryFetchTypes';

export abstract class RagQueryFetchCommand extends CommandBase<RagQueryFetchParams, RagQueryFetchResult> {
  getDescription(): string {
    return 'Fetch results from a query handle with bidirectional navigation';
  }
}
