/**
 * Web Search Command - Server Implementation
 *
 * Searches the web using DuckDuckGo HTML scraping (no API key required)
 * Returns search results for AI consumption
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import type { WebSearchParams, WebSearchResult, SearchResult } from '../shared/WebSearchTypes';
import { createWebSearchResultFromParams } from '../shared/WebSearchTypes';

export class WebSearchServerCommand extends CommandBase<WebSearchParams, WebSearchResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('web/search', context, subpath, commander);
  }

  /**
   * Execute web search using DuckDuckGo HTML
   */
  async execute(params: JTAGPayload): Promise<WebSearchResult> {
    const searchParams = params as WebSearchParams;

    console.log(`🔍 SERVER: Searching web for: "${searchParams.query}"`);

    try {
      const maxResults = searchParams.maxResults ?? 10;

      // Encode query for URL
      const encodedQuery = encodeURIComponent(searchParams.query);
      const url = `https://duckduckgo.com/html/?q=${encodedQuery}`;

      // Fetch with proper headers to avoid bot detection
      const response = await fetch(url, {
        headers: {
          'User-Agent': 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36',
          'Accept': 'text/html,application/xhtml+xml',
          'Accept-Language': 'en-US,en;q=0.9',
        },
        redirect: 'follow'
      });

      if (!response.ok) {
        throw new Error(`DuckDuckGo returned ${response.status}`);
      }

      const html = await response.text();
      const results: SearchResult[] = [];

      // Parse DuckDuckGo HTML results
      // DuckDuckGo structure: <div class="result">
      const resultRegex = /<div class="result[^"]*"[\s\S]*?<\/div>/g;
      const matches = html.matchAll(resultRegex);

      for (const match of matches) {
        const resultHtml = match[0];

        // Extract title from result__a class
        const titleMatch = resultHtml.match(/<a[^>]*class="result__a"[^>]*>(.*?)<\/a>/);
        const title = titleMatch ? this.stripHtml(titleMatch[1]) : '';

        // Extract URL from result__url or uddg parameter
        const urlMatch = resultHtml.match(/<a[^>]*href="\/\/duckduckgo\.com\/l\/\?uddg=([^"&]*)/)
          || resultHtml.match(/<a[^>]*href="([^"]*)"[^>]*class="result__url"/);
        let resultUrl = urlMatch ? decodeURIComponent(urlMatch[1]) : '';

        // Extract snippet
        const snippetMatch = resultHtml.match(/<span class="result__snippet"[^>]*>(.*?)<\/span>/);
        const snippet = snippetMatch ? this.stripHtml(snippetMatch[1]) : '';

        if (title && resultUrl) {
          // Extract domain
          let domain = '';
          try {
            const urlObj = new URL(resultUrl);
            domain = urlObj.hostname;
          } catch {
            domain = resultUrl.split('/')[2] ?? resultUrl;
          }

          results.push({ title, url: resultUrl, snippet, domain });

          if (results.length >= maxResults) {
            break;
          }
        }
      }

      // Filter by domains if specified
      const filteredResults = searchParams.domains && searchParams.domains.length > 0
        ? results.filter(r => searchParams.domains!.some(d => r.domain.includes(d)))
        : results;

      console.log(`✅ SERVER: Found ${filteredResults.length} results for "${searchParams.query}"`);

      return createWebSearchResultFromParams(searchParams, {
        success: true,
        query: searchParams.query,
        results: filteredResults.slice(0, maxResults),
        totalResults: filteredResults.length
      });

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
