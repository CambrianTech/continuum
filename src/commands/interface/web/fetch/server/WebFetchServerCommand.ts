/**
 * Web Fetch Command - Server Implementation
 *
 * Fetches web pages and returns clean content
 * Handles HTML parsing and text extraction
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '@system/core/types/JTAGTypes';
import type { WebFetchParams, WebFetchResult } from '../shared/WebFetchTypes';
import { createWebFetchResultFromParams } from '../shared/WebFetchTypes';

export class WebFetchServerCommand extends CommandBase<WebFetchParams, WebFetchResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('interface/web/fetch', context, subpath, commander);
  }

  /**
   * Execute web fetch
   */
  async execute(params: JTAGPayload): Promise<WebFetchResult> {
    const fetchParams = params as WebFetchParams;

    console.log(`🌐 SERVER: Fetching URL: ${fetchParams.url}`);

    try {
      const format = fetchParams.format || 'text';
      const maxLength = fetchParams.maxLength || 50000;

      // Fetch the page - use Opera browser User-Agent to match user's browser
      const response = await fetch(fetchParams.url, {
        headers: {
          'User-Agent': 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36 OPR/124.0.0.0',
          'Accept': 'text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8',
          'Accept-Language': 'en-US,en;q=0.9',
          'Cache-Control': 'no-cache',
          'Pragma': 'no-cache',
          'Sec-Fetch-Dest': 'document',
          'Sec-Fetch-Mode': 'navigate',
          'Sec-Fetch-Site': 'none',
          'Sec-Fetch-User': '?1',
          'Upgrade-Insecure-Requests': '1'
        },
        redirect: 'follow'
      });

      if (!response.ok) {
        throw new Error(`HTTP ${response.status}: ${response.statusText}`);
      }

      const contentType = response.headers.get('content-type') || '';
      const finalUrl = response.url;

      // Get HTML content
      let html = await response.text();

      // Truncate if too long
      if (html.length > maxLength * 2) {
        html = html.substring(0, maxLength * 2);
      }

      // Extract title
      const titleMatch = html.match(/<title[^>]*>(.*?)<\/title>/i);
      const title = titleMatch ? this.stripHtml(titleMatch[1]) : undefined;

      // Process based on format
      let content: string;
      if (format === 'html') {
        content = html;
      } else if (format === 'markdown') {
        content = this.htmlToMarkdown(html);
      } else {
        // text format (default)
        content = this.htmlToText(html);
      }

      // Truncate content to max length
      if (content.length > maxLength) {
        content = content.substring(0, maxLength) + '\n\n[Content truncated...]';
      }

      console.log(`✅ SERVER: Fetched ${fetchParams.url} (${content.length} chars)`);

      return createWebFetchResultFromParams(fetchParams, {
        success: true,
        url: fetchParams.url,
        title,
        content,
        contentLength: content.length,
        contentType,
        finalUrl
      });

    } catch (error) {
      console.error(`❌ SERVER: Failed to fetch ${fetchParams.url}:`, error);

      return createWebFetchResultFromParams(fetchParams, {
        success: false,
        url: fetchParams.url,
        content: '',
        contentLength: 0,
        error: error instanceof Error ? error.message : 'Unknown error'
      });
    }
  }

  /**
   * Convert HTML to plain text
   */
  private htmlToText(html: string): string {
    // Remove script and style tags
    let text = html
      .replace(/<script[^>]*>[\s\S]*?<\/script>/gi, '')
      .replace(/<style[^>]*>[\s\S]*?<\/style>/gi, '')
      .replace(/<head[^>]*>[\s\S]*?<\/head>/gi, '');

    // Convert common block elements to newlines
    text = text
      .replace(/<\/p>/gi, '\n\n')
      .replace(/<br[^>]*>/gi, '\n')
      .replace(/<\/div>/gi, '\n')
      .replace(/<\/h[1-6]>/gi, '\n\n')
      .replace(/<\/li>/gi, '\n')
      .replace(/<\/tr>/gi, '\n');

    // Strip remaining HTML tags
    text = this.stripHtml(text);

    // Clean up whitespace
    text = text
      .replace(/\n{3,}/g, '\n\n')
      .replace(/[ \t]+/g, ' ')
      .trim();

    return text;
  }

  /**
   * Convert HTML to Markdown (basic implementation)
   */
  private htmlToMarkdown(html: string): string {
    // Remove script and style tags
    let md = html
      .replace(/<script[^>]*>[\s\S]*?<\/script>/gi, '')
      .replace(/<style[^>]*>[\s\S]*?<\/style>/gi, '');

    // Convert headings
    md = md.replace(/<h1[^>]*>(.*?)<\/h1>/gi, '# $1\n\n');
    md = md.replace(/<h2[^>]*>(.*?)<\/h2>/gi, '## $1\n\n');
    md = md.replace(/<h3[^>]*>(.*?)<\/h3>/gi, '### $1\n\n');

    // Convert links
    md = md.replace(/<a[^>]*href="([^"]*)"[^>]*>(.*?)<\/a>/gi, '[$2]($1)');

    // Convert bold and italic
    md = md.replace(/<strong[^>]*>(.*?)<\/strong>/gi, '**$1**');
    md = md.replace(/<b[^>]*>(.*?)<\/b>/gi, '**$1**');
    md = md.replace(/<em[^>]*>(.*?)<\/em>/gi, '*$1*');
    md = md.replace(/<i[^>]*>(.*?)<\/i>/gi, '*$1*');

    // Convert lists
    md = md.replace(/<li[^>]*>(.*?)<\/li>/gi, '- $1\n');

    // Convert paragraphs and divs
    md = md.replace(/<\/p>/gi, '\n\n');
    md = md.replace(/<br[^>]*>/gi, '\n');

    // Strip remaining HTML
    md = this.stripHtml(md);

    // Clean up whitespace
    md = md
      .replace(/\n{3,}/g, '\n\n')
      .trim();

    return md;
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
      .replace(/&hellip;/g, '...')
      .trim();
  }
}
