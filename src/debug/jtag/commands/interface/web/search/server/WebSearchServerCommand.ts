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

export class WebSearchServerCommand extends CommandBase<WebSearchParams, WebSearchResult> {

  private static readonly BRAVE_API_KEY = process.env.BRAVE_SEARCH_API_KEY;

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('interface/web/search', context, subpath, commander);
  }

  /**
   * Execute web search - uses Brave API if available, falls back to DuckDuckGo
   */
  async execute(params: JTAGPayload): Promise<WebSearchResult> {
    const searchParams = params as WebSearchParams;

    console.log(`🔍 SERVER: Searching web for: "${searchParams.query}"`);

    try {
      if (WebSearchServerCommand.BRAVE_API_KEY) {
        return await this.searchWithBrave(searchParams);
      } else {
        console.log('⚠️ No BRAVE_SEARCH_API_KEY set, using DuckDuckGo fallback');
        return await this.searchWithDuckDuckGo(searchParams);
      }
    } catch (error) {
      console.error(`❌ SERVER: Web search failed:`, error);

      return createWebSearchResultFromParams(searchParams, {
        success: false,
        query: searchParams.query,
        results: [],
        totalResults: 0,
        error: error instanceof Error ? error.message : 'Unknown error'
      });
    }
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
