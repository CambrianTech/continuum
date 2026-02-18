/**
 * Web Search Command - Server Implementation
 *
 * Searches the web using Brave Search API (free tier: 2000 queries/month)
 * Falls back to DuckDuckGo HTML if no API key configured.
 *
 * Set BRAVE_SEARCH_API_KEY in environment to enable Brave Search.
 * Get free key at: https://brave.com/search/api/
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '@system/core/types/JTAGTypes';
import type { WebSearchParams, WebSearchResult, SearchResult } from '../shared/WebSearchTypes';
import { createWebSearchResultFromParams } from '../shared/WebSearchTypes';
import { searchRateLimiter } from './SearchRateLimiter';

export class WebSearchServerCommand extends CommandBase<WebSearchParams, WebSearchResult> {

  private static readonly BRAVE_API_KEY = process.env.BRAVE_SEARCH_API_KEY;

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('interface/web/search', context, subpath, commander);
  }

  /**
   * Execute web search with caching, deduplication, and rate limiting.
   *
   * Priority chain:
   * 1. Return cached result if available (24hr TTL)
   * 2. Deduplicate: if same query is in-flight, share the promise
   * 3. Use Brave API if key set AND quota available
   * 4. DuckDuckGo if no Brave key or quota exhausted
   */
  async execute(params: JTAGPayload): Promise<WebSearchResult> {
    const searchParams = params as WebSearchParams;
    const maxResults = searchParams.maxResults ?? 10;

    console.log(`🔍 SERVER: Searching web for: "${searchParams.query}" (Brave remaining: ${searchRateLimiter.braveRemaining})`);

    // 1. Check cache
    const cached = searchRateLimiter.getCached(searchParams.query, maxResults, searchParams.domains);
    if (cached) {
      console.log(`📦 SERVER: Cache hit for "${searchParams.query}"`);
      return createWebSearchResultFromParams(searchParams, {
        success: true,
        query: searchParams.query,
        results: cached.results,
        totalResults: cached.totalResults,
      });
    }

    // 2. Check in-flight deduplication
    const inflight = searchRateLimiter.getInflight(searchParams.query, maxResults, searchParams.domains);
    if (inflight) {
      console.log(`🔄 SERVER: Deduplicating in-flight request for "${searchParams.query}"`);
      const shared = await inflight;
      return createWebSearchResultFromParams(searchParams, {
        success: true,
        query: searchParams.query,
        results: shared.results,
        totalResults: shared.totalResults,
      });
    }

    // 3. Execute search with deduplication tracking
    const searchPromise = this._executeSearch(searchParams);
    const cleanup = searchRateLimiter.setInflight(
      searchParams.query, maxResults, searchParams.domains, searchPromise,
    );

    try {
      const result = await searchPromise;

      // Cache successful results
      searchRateLimiter.setCached(
        searchParams.query, maxResults, searchParams.domains,
        result.results, result.totalResults,
      );

      return createWebSearchResultFromParams(searchParams, {
        success: true,
        query: searchParams.query,
        results: result.results,
        totalResults: result.totalResults,
      });
    } catch (error) {
      console.error(`❌ SERVER: Web search failed:`, error);
      return createWebSearchResultFromParams(searchParams, {
        success: false,
        query: searchParams.query,
        results: [],
        totalResults: 0,
        error: error instanceof Error ? error.message : 'Unknown error',
      });
    } finally {
      cleanup();
    }
  }

  /**
   * Internal search execution — picks provider based on availability
   */
  private async _executeSearch(searchParams: WebSearchParams): Promise<{ results: SearchResult[]; totalResults: number }> {
    if (WebSearchServerCommand.BRAVE_API_KEY && searchRateLimiter.braveAvailable) {
      const result = await this.searchWithBrave(searchParams);
      return { results: result.results, totalResults: result.totalResults };
    }

    if (WebSearchServerCommand.BRAVE_API_KEY && !searchRateLimiter.braveAvailable) {
      console.log('⚠️ SERVER: Brave API quota exhausted, using DuckDuckGo');
    } else {
      console.log('⚠️ SERVER: No BRAVE_SEARCH_API_KEY set, using DuckDuckGo');
    }

    const result = await this.searchWithDuckDuckGo(searchParams);
    return { results: result.results, totalResults: result.totalResults };
  }

  /**
   * Search using Brave Search API (recommended)
   * Free tier: 2000 queries/month
   * Docs: https://api.search.brave.com/app/documentation/web-search/get-started
   */
  private async searchWithBrave(searchParams: WebSearchParams): Promise<WebSearchResult> {
    const maxResults = searchParams.maxResults ?? 10;
    const encodedQuery = encodeURIComponent(searchParams.query);

    const url = `https://api.search.brave.com/res/v1/web/search?q=${encodedQuery}&count=${maxResults}`;

    const response = await fetch(url, {
      headers: {
        'Accept': 'application/json',
        'X-Subscription-Token': WebSearchServerCommand.BRAVE_API_KEY!
      }
    });

    if (!response.ok) {
      const errorText = await response.text();
      throw new Error(`Brave API error ${response.status}: ${errorText}`);
    }

    searchRateLimiter.recordBraveRequest();
    const data = await response.json();
    const results: SearchResult[] = [];

    // Parse Brave's response format
    if (data.web?.results) {
      for (const item of data.web.results) {
        results.push({
          title: item.title || '',
          url: item.url || '',
          snippet: item.description || '',
          domain: item.meta_url?.hostname || new URL(item.url).hostname
        });

        if (results.length >= maxResults) break;
      }
    }

    // Filter by domains if specified
    const filteredResults = searchParams.domains?.length
      ? results.filter(r => searchParams.domains!.some(d => r.domain.includes(d)))
      : results;

    console.log(`✅ SERVER: Brave found ${filteredResults.length} results for "${searchParams.query}"`);

    return createWebSearchResultFromParams(searchParams, {
      success: true,
      query: searchParams.query,
      results: filteredResults,
      totalResults: filteredResults.length
    });
  }

  /**
   * Fallback: Search using DuckDuckGo HTML scraping (no API key needed)
   * Less reliable but works without configuration
   */
  private async searchWithDuckDuckGo(searchParams: WebSearchParams): Promise<WebSearchResult> {
    const maxResults = searchParams.maxResults ?? 10;
    const encodedQuery = encodeURIComponent(searchParams.query);

    // DuckDuckGo HTML interface
    const url = `https://html.duckduckgo.com/html/?q=${encodedQuery}`;

    const response = await fetch(url, {
      headers: {
        'User-Agent': 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36',
        'Accept': 'text/html'
      }
    });

    if (!response.ok) {
      throw new Error(`DuckDuckGo returned ${response.status}`);
    }

    const html = await response.text();
    const results: SearchResult[] = [];

    // Parse DuckDuckGo results - they use class="result__a" for links
    const resultRegex = /<a[^>]*class="result__a"[^>]*href="([^"]*)"[^>]*>(.*?)<\/a>[\s\S]*?<a[^>]*class="result__snippet"[^>]*>(.*?)<\/a>/gi;

    let match;
    while ((match = resultRegex.exec(html)) !== null && results.length < maxResults) {
      let resultUrl = match[1];

      // DuckDuckGo wraps URLs in redirect
      if (resultUrl.includes('uddg=')) {
        const decoded = decodeURIComponent(resultUrl.split('uddg=')[1]?.split('&')[0] || '');
        if (decoded) resultUrl = decoded;
      }

      const title = this.stripHtml(match[2]);
      const snippet = this.stripHtml(match[3]);

      if (title && resultUrl && !resultUrl.includes('duckduckgo.com')) {
        let domain = '';
        try {
          domain = new URL(resultUrl).hostname;
        } catch {
          domain = resultUrl.split('/')[2] || '';
        }

        results.push({ title, url: resultUrl, snippet, domain });
      }
    }

    // Filter by domains if specified
    const filteredResults = searchParams.domains?.length
      ? results.filter(r => searchParams.domains!.some(d => r.domain.includes(d)))
      : results;

    console.log(`✅ SERVER: DuckDuckGo found ${filteredResults.length} results for "${searchParams.query}"`);

    return createWebSearchResultFromParams(searchParams, {
      success: true,
      query: searchParams.query,
      results: filteredResults,
      totalResults: filteredResults.length
    });
  }

  /**
   * Strip HTML tags and decode entities
   */
  private stripHtml(html: string): string {
    return html
      .replace(/<[^>]*>/g, '')
      .replace(/&nbsp;/g, ' ')
      .replace(/&amp;/g, '&')
      .replace(/&lt;/g, '<')
      .replace(/&gt;/g, '>')
      .replace(/&quot;/g, '"')
      .replace(/&#39;/g, "'")
      .replace(/&mdash;/g, '—')
      .replace(/&ndash;/g, '–')
      .trim();
  }

}
