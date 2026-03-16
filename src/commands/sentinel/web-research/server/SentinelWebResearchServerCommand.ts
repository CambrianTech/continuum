/**
 * Sentinel Web Research — Server Implementation
 *
 * Composes existing interface/web/search and interface/web/fetch commands
 * with LLM summarization to provide research capability to sentinel pipelines.
 *
 * Flow:
 * 1. Search the web for the query (interface/web/search)
 * 2. Fetch top N result pages (interface/web/fetch)
 * 3. Summarize findings with LLM (ai/generate via command)
 * 4. Return structured results
 *
 * Called from Rust web_research step via execute_ts_json("sentinel/web-research", ...).
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import { Commands } from '../../../../system/core/shared/Commands';
import type { SentinelWebResearchParams, SentinelWebResearchResult, WebResearchPage } from '../shared/SentinelWebResearchTypes';
import type { WebSearchParams, WebSearchResult } from '../../../interface/web/search/shared/WebSearchTypes';
import type { WebFetchParams, WebFetchResult } from '../../../interface/web/fetch/shared/WebFetchTypes';
import type { AIGenerateParams, AIGenerateResult } from '../../../ai/generate/shared/AIGenerateTypes';

export class SentinelWebResearchServerCommand extends CommandBase<SentinelWebResearchParams, SentinelWebResearchResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('sentinel/web-research', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<SentinelWebResearchResult> {
    const p = params as SentinelWebResearchParams;
    const query = p.query;
    const maxPages = p.maxPages ?? 3;
    const extract = p.extract;

    if (!query) {
      return transformPayload(params, {
        success: false,
        summary: '',
        pages: [],
        pagesFetched: 0,
        query: '',
        error: 'Missing required "query" parameter',
      });
    }

    try {
      // Step 1: Search the web
      const searchResult = await Commands.execute<WebSearchParams, WebSearchResult>('interface/web/search', {
        query,
        maxResults: maxPages * 2, // fetch extra in case some fail
      });

      if (!searchResult.success || !searchResult.results?.length) {
        return transformPayload(params, {
          success: false,
          summary: `No search results found for: ${query}`,
          pages: [],
          pagesFetched: 0,
          query,
          error: searchResult.error || 'No results found',
        });
      }

      // Step 2: Fetch top N pages concurrently
      const urlsToFetch = searchResult.results.slice(0, maxPages);
      const fetchPromises = urlsToFetch.map(async (result) => {
        try {
          const fetchResult = await Commands.execute<WebFetchParams, WebFetchResult>('interface/web/fetch', {
            url: result.url,
            format: 'text',
            maxLength: 15000, // Cap per page to avoid overwhelming LLM
          });

          if (fetchResult.success && fetchResult.content) {
            return {
              url: result.url,
              title: fetchResult.title || result.title,
              snippet: result.snippet,
              content: fetchResult.content.slice(0, 15000),
              relevance: '',
            } as WebResearchPage;
          }
          return null;
        } catch {
          return null;
        }
      });

      const fetchedPages = (await Promise.all(fetchPromises)).filter(
        (p): p is WebResearchPage => p !== null,
      );

      if (fetchedPages.length === 0) {
        // Fall back to snippets from search results
        const snippetSummary = searchResult.results
          .slice(0, maxPages)
          .map((r) => `**${r.title}** (${r.url})\n${r.snippet}`)
          .join('\n\n');

        return transformPayload(params, {
          success: true,
          summary: snippetSummary,
          pages: searchResult.results.slice(0, maxPages).map((r) => ({
            url: r.url,
            title: r.title,
            snippet: r.snippet,
            content: r.snippet,
            relevance: 'snippet-only',
          })),
          pagesFetched: 0,
          query,
        });
      }

      // Step 3: LLM summarization of fetched content
      const extractInstruction = extract
        ? `Focus on extracting: ${extract}`
        : 'Provide a comprehensive summary';

      const contentForLLM = fetchedPages
        .map((p, i) => `--- Page ${i + 1}: ${p.title} (${p.url}) ---\n${p.content.slice(0, 8000)}`)
        .join('\n\n');

      const summaryPrompt = `You are a research assistant. Summarize the following web pages in response to this query: "${query}"

${extractInstruction}

${contentForLLM}

Provide a concise, actionable summary. Include specific code examples, commands, or solutions when relevant. Cite sources by URL.`;

      let summary = '';
      try {
        const llmResult = await Commands.execute<AIGenerateParams, AIGenerateResult>('ai/generate', {
          messages: [{ role: 'user', content: summaryPrompt }],
          maxTokens: 2000,
        } as Partial<AIGenerateParams>);
        summary = llmResult.text || '';
      } catch {
        // LLM unavailable — concatenate snippets
        summary = fetchedPages
          .map((p) => `**${p.title}** (${p.url})\n${p.snippet || p.content.slice(0, 500)}`)
          .join('\n\n');
      }

      return transformPayload(params, {
        success: true,
        summary,
        pages: fetchedPages,
        pagesFetched: fetchedPages.length,
        query,
      });
    } catch (error: unknown) {
      const message = error instanceof Error ? error.message : String(error);
      return transformPayload(params, {
        success: false,
        summary: '',
        pages: [],
        pagesFetched: 0,
        query,
        error: message,
      });
    }
  }
}
