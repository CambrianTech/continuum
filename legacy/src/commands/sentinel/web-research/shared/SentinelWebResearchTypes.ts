/**
 * Sentinel Web Research — Types
 *
 * Composes interface/web/search + interface/web/fetch + LLM summarization.
 * Called from Rust web_research step via execute_ts_json.
 */

import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { CommandInput } from '@system/core/types/JTAGTypes';

export interface SentinelWebResearchParams extends CommandParams {
  /** Search query (already interpolated by Rust) */
  query: string;
  /** Max pages to fetch and summarize (default: 3) */
  maxPages?: number;
  /** What to extract: "code examples", "error solutions", etc. */
  extract?: string;
}

export interface WebResearchPage {
  url: string;
  title: string;
  snippet: string;
  content: string;
  relevance: string;
}

export interface SentinelWebResearchResult extends CommandResult {
  /** Overall summary of findings */
  summary: string;
  /** Individual page results */
  pages: WebResearchPage[];
  /** Total pages fetched */
  pagesFetched: number;
  /** Query that was searched */
  query: string;
}

export const SentinelWebResearch = {
  execute(params: CommandInput<SentinelWebResearchParams>): Promise<SentinelWebResearchResult> {
    return Commands.execute<SentinelWebResearchParams, SentinelWebResearchResult>(
      'sentinel/web-research',
      params as Partial<SentinelWebResearchParams>,
    );
  },
  commandName: 'sentinel/web-research' as const,
} as const;
