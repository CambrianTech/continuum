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
      const url = `https://www.google.com/search?q=${encodedQuery}&num=${maxResults}`;

      // Fetch with proper headers to look like a real browser
      const response = await fetch(url, {
        headers: {
          'User-Agent': 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36',
          'Accept': 'text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8',
          'Accept-Language': 'en-US,en;q=0.9',
          'Accept-Encoding': 'gzip, deflate, br',
          'DNT': '1',
          'Connection': 'keep-alive',
          'Upgrade-Insecure-Requests': '1'
        },
        redirect: 'follow'
      });

      if (!response.ok) {
        throw new Error(`Google returned ${response.status}`);
      }

      const html = await response.text();
      const results: SearchResult[] = [];

      // Parse Google search results
      // Google structure: <div class="g"> or <div data-sokoban-container>
      // Look for divs containing search results
      const resultDivRegex = /<div[^>]*class="[^"]*\bg\b[^"]*"[^>]*>[\s\S]*?<\/div>/g;
      const matches = html.matchAll(resultDivRegex);

      for (const match of matches) {
        const resultHtml = match[0];

        // Extract title from <h3> tag
        const titleMatch = resultHtml.match(/<h3[^>]*>(.*?)<\/h3>/s);
        const title = titleMatch ? this.stripHtml(titleMatch[1]) : '';

        // Extract URL from href in <a> tag
        const urlMatch = resultHtml.match(/<a[^>]*href="([^"]*)"[^>]*>/);
        let resultUrl = urlMatch ? urlMatch[1] : '';

        // Google URLs are sometimes /url?q=actual_url format
        if (resultUrl.startsWith('/url?q=')) {
          const actualUrl = resultUrl.match(/\/url\?q=([^&]*)/);
          if (actualUrl) {
            resultUrl = decodeURIComponent(actualUrl[1]);
          }
        }

        // Skip Google's internal links
        if (!resultUrl || resultUrl.startsWith('/') || resultUrl.includes('google.com/search')) {
          continue;
        }

        // Extract snippet from <div> with specific class patterns
        const snippetMatch = resultHtml.match(/<div[^>]*class="[^"]*\b(VwiC3b|s3v9rd|st)\b[^"]*"[^>]*>(.*?)<\/div>/s)
          || resultHtml.match(/<span[^>]*class="[^"]*\bst\b[^"]*"[^>]*>(.*?)<\/span>/s);
        const snippet = snippetMatch ? this.stripHtml(snippetMatch[2]) : '';

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
