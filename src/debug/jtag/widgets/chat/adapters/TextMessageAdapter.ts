/**
 * Text Message Adapter - Markdown rendering with syntax highlighting
 *
 * Handles plain text and markdown formatting with:
 * - Full markdown support (headings, lists, code blocks, etc.)
 * - Syntax highlighting for code blocks (highlight.js)
 * - Safe HTML rendering (sanitized by marked)
 */

import { AbstractMessageAdapter } from './AbstractMessageAdapter';
import type { ChatMessageEntity } from '../../../system/data/entities/ChatMessageEntity';
import type { TextContentData } from './AdapterTypes';
import { marked } from 'marked';
import hljs from 'highlight.js';

export class TextMessageAdapter extends AbstractMessageAdapter<TextContentData> {
  constructor() {
    super('text', {
      enableIntersectionObserver: false,
      lazyLoadContent: false,
      enableInteractions: true
    });

    // Configure marked for GitHub Flavored Markdown
    marked.setOptions({
      breaks: true,  // Convert \n to <br>
      gfm: true,     // GitHub Flavored Markdown
      pedantic: false
    });
  }

  parseContent(message: ChatMessageEntity): TextContentData | null {
    const text = message.content?.text;
    if (!text || typeof text !== 'string') {
      return null;
    }

    return {
      contentType: 'text',
      originalText: text,
      text: text,
      formatting: {
        // Detect markdown formatting
        code: text.includes('`'),
        codeBlock: text.match(/```(\w+)/)?.[1], // Extract language from code fence
        links: this.detectLinks(text)
      }
    };
  }

  renderContent(data: TextContentData, _currentUserId: string): string {
    try {
      // Parse markdown to HTML
      let htmlContent = marked.parse(data.text) as string;

      // Apply syntax highlighting to code blocks after markdown parsing
      htmlContent = this.applySyntaxHighlighting(htmlContent);

      // Make long error code blocks collapsible
      htmlContent = this.makeErrorsCollapsible(htmlContent);

      return `
        <div class="text-message-content markdown-body">
          ${htmlContent}
        </div>
      `;
    } catch (error) {
      console.error('Markdown rendering failed:', error);
      // Fallback to plain text with escaping
      return `<div class="text-message-content"><p>${this.escapeHtml(data.text)}</p></div>`;
    }
  }

  async handleContentLoading(_element: HTMLElement): Promise<void> {
    // Text content loads instantly, no async work needed
    return Promise.resolve();
  }

  getContentClasses(): string[] {
    return ['text-adapter', 'markdown-enabled'];
  }

  getCSS(): string {
    return `
      /* Text Message Adapter Styles */
      .content-type-text {
        line-height: 1.6;
      }

      .text-message-content {
        word-wrap: break-word;
        overflow-wrap: break-word;
      }

      /* Markdown Body Styles */
      .markdown-body {
        font-size: 14px;
        color: inherit;
      }

      .markdown-body h1,
      .markdown-body h2,
      .markdown-body h3,
      .markdown-body h4,
      .markdown-body h5,
      .markdown-body h6 {
        margin-top: 16px;
        margin-bottom: 8px;
        font-weight: 600;
        line-height: 1.25;
      }

      .markdown-body h1 { font-size: 1.5em; }
      .markdown-body h2 { font-size: 1.3em; }
      .markdown-body h3 { font-size: 1.15em; }

      .markdown-body p {
        margin-top: 0;
        margin-bottom: 10px;
      }

      .markdown-body ul,
      .markdown-body ol {
        padding-left: 2em;
        margin-top: 0;
        margin-bottom: 10px;
      }

      .markdown-body li {
        margin-bottom: 4px;
      }

      .markdown-body code {
        background-color: rgba(175, 184, 193, 0.2);
        padding: 0.2em 0.4em;
        border-radius: 3px;
        font-size: 85%;
        font-family: 'SF Mono', Monaco, 'Cascadia Code', 'Roboto Mono', Consolas, 'Courier New', monospace;
      }

      .markdown-body pre {
        background-color: #1e1e1e;
        color: #d4d4d4;
        padding: 12px;
        border-radius: 6px;
        overflow-x: auto;
        margin: 12px 0;
      }

      .markdown-body pre code {
        background-color: transparent;
        padding: 0;
        border-radius: 0;
        font-size: 13px;
        line-height: 1.45;
      }

      .markdown-body blockquote {
        border-left: 4px solid rgba(175, 184, 193, 0.4);
        padding-left: 16px;
        margin-left: 0;
        color: rgba(0, 0, 0, 0.6);
      }

      .markdown-body a {
        color: #0969da;
        text-decoration: none;
      }

      .markdown-body a:hover {
        text-decoration: underline;
      }

      .markdown-body hr {
        height: 0.25em;
        padding: 0;
        margin: 24px 0;
        background-color: rgba(175, 184, 193, 0.2);
        border: 0;
      }

      .markdown-body table {
        border-collapse: collapse;
        margin: 12px 0;
      }

      .markdown-body table th,
      .markdown-body table td {
        padding: 6px 13px;
        border: 1px solid rgba(175, 184, 193, 0.2);
      }

      .markdown-body table th {
        font-weight: 600;
        background-color: rgba(175, 184, 193, 0.1);
      }

      /* Syntax Highlighting - VS Code Dark+ theme colors */
      .hljs {
        display: block;
        overflow-x: auto;
        background: #1e1e1e;
        color: #d4d4d4;
      }

      .hljs-keyword,
      .hljs-selector-tag,
      .hljs-literal,
      .hljs-section,
      .hljs-link {
        color: #569cd6;
      }

      .hljs-function .hljs-keyword {
        color: #c586c0;
      }

      .hljs-subst {
        color: #d4d4d4;
      }

      .hljs-string,
      .hljs-title,
      .hljs-name,
      .hljs-type,
      .hljs-attribute,
      .hljs-symbol,
      .hljs-bullet,
      .hljs-addition,
      .hljs-variable,
      .hljs-template-tag,
      .hljs-template-variable {
        color: #ce9178;
      }

      .hljs-comment,
      .hljs-quote,
      .hljs-deletion {
        color: #6a9955;
      }

      .hljs-number,
      .hljs-regexp,
      .hljs-built_in {
        color: #b5cea8;
      }

      .hljs-class .hljs-title {
        color: #4ec9b0;
      }

      .hljs-meta {
        color: #d7ba7d;
      }

      .hljs-emphasis {
        font-style: italic;
      }

      .hljs-strong {
        font-weight: bold;
      }

      /* Collapsible Error Sections */
      .collapsible-error {
        border: 1px solid rgba(175, 184, 193, 0.3);
        border-radius: 6px;
        margin: 12px 0;
        background-color: rgba(255, 99, 71, 0.05);
      }

      .collapsible-error summary {
        padding: 8px 12px;
        cursor: pointer;
        font-weight: 600;
        user-select: none;
        background-color: rgba(255, 99, 71, 0.1);
        border-radius: 6px 6px 0 0;
        color: #d73a49;
      }

      .collapsible-error summary:hover {
        background-color: rgba(255, 99, 71, 0.15);
      }

      .collapsible-error[open] summary {
        border-bottom: 1px solid rgba(175, 184, 193, 0.3);
        border-radius: 6px 6px 0 0;
        margin-bottom: 0;
      }

      .collapsible-error pre {
        margin: 0;
        border-radius: 0 0 6px 6px;
      }

      /* Collapsible Long Content (neutral styling for non-errors) */
      .collapsible-long {
        border: 1px solid rgba(175, 184, 193, 0.3);
        border-radius: 6px;
        margin: 12px 0;
        background-color: rgba(175, 184, 193, 0.05);
      }

      .collapsible-long summary {
        padding: 8px 12px;
        cursor: pointer;
        font-weight: 600;
        user-select: none;
        background-color: rgba(175, 184, 193, 0.1);
        border-radius: 6px 6px 0 0;
        color: rgba(0, 0, 0, 0.7);
      }

      .collapsible-long summary:hover {
        background-color: rgba(175, 184, 193, 0.15);
      }

      .collapsible-long[open] summary {
        border-bottom: 1px solid rgba(175, 184, 193, 0.3);
        border-radius: 6px 6px 0 0;
        margin-bottom: 0;
      }

      .collapsible-long pre {
        margin: 0;
        border-radius: 0 0 6px 6px;
      }
    `;
  }

  /**
   * Make long error code blocks collapsible using <details> element
   * Detects actual error messages/stack traces (not code examples that mention errors)
   */
  private makeErrorsCollapsible(html: string): string {
    // Match <pre><code> blocks and make them collapsible if they're long or actual errors
    return html.replace(/<pre><code([^>]*)>([\s\S]*?)<\/code><\/pre>/g, (match, attrs: string, content: string) => {
      // Count lines in the code block
      const lines = content.trim().split('\n');
      const lineCount = lines.length;

      // Check if this looks like an actual stack trace (multiple lines with "at " pattern)
      const stackTraceLines = lines.filter(line => /^\s*at\s+\w+/.test(line));
      const isStackTrace = stackTraceLines.length >= 3;

      // Check if this looks like an actual error message (line starts with Error:, TypeError:, etc.)
      const hasActualErrorMessage = lines.some(line => /^\s*(Error|TypeError|ReferenceError|SyntaxError|Exception):/i.test(line));

      // Check if this is code with language-specific syntax highlighting (code example, not error log)
      const isCodeExample = /class="language-\w+"/.test(attrs);

      // Only make collapsible with error styling if it's an actual error/stack trace
      const isActualError = (isStackTrace || hasActualErrorMessage) && !isCodeExample;

      // Make collapsible if >20 lines (very long) OR if it's an actual error
      if (lineCount > 20 || isActualError) {
        const errorType = isActualError ?
          (content.match(/(Error|TypeError|ReferenceError|SyntaxError|Exception)/i)?.[0] || 'Error') :
          'Details';

        const cssClass = isActualError ? 'collapsible-error' : 'collapsible-long';

        return `<details class="${cssClass}">
<summary>${errorType} (${lineCount} lines) - click to expand</summary>
<pre><code${attrs}>${content}</code></pre>
</details>`;
      }

      // Return unchanged if short and not an error
      return match;
    });
  }

  /**
   * Apply syntax highlighting to code blocks in HTML
   * This is done post-markdown parsing to work with the newer marked API
   */
  private applySyntaxHighlighting(html: string): string {
    // Replace <code class="language-X"> blocks with highlighted versions
    return html.replace(/<code class="language-(\w+)">([\s\S]*?)<\/code>/g, (_match, lang: string, code: string) => {
      // Decode HTML entities that marked encoded
      const decodedCode = code
        .replace(/&lt;/g, '<')
        .replace(/&gt;/g, '>')
        .replace(/&amp;/g, '&')
        .replace(/&quot;/g, '"')
        .replace(/&#039;/g, "'");

      try {
        if (lang && hljs.getLanguage(lang)) {
          const highlighted = hljs.highlight(decodedCode, { language: lang }).value;
          return `<code class="language-${lang} hljs">${highlighted}</code>`;
        }
      } catch (e) {
        console.warn(`Syntax highlighting failed for language: ${lang}`, e);
      }

      // Fallback to auto-detection
      try {
        const highlighted = hljs.highlightAuto(decodedCode).value;
        return `<code class="language-${lang} hljs">${highlighted}</code>`;
      } catch (e) {
        return `<code class="language-${lang}">${code}</code>`;
      }
    });
  }

  /**
   * Detect links in text for metadata
   */
  private detectLinks(text: string): Array<{ url: string; text: string; start: number; end: number }> {
    const urlRegex = /(https?:\/\/[^\s]+)/g;
    const links: Array<{ url: string; text: string; start: number; end: number }> = [];
    let match;

    while ((match = urlRegex.exec(text)) !== null) {
      links.push({
        url: match[0],
        text: match[0],
        start: match.index,
        end: match.index + match[0].length
      });
    }

    return links;
  }

  /**
   * Escape HTML for safe rendering (fallback)
   */
  private escapeHtml(unsafe: string): string {
    return unsafe
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;')
      .replace(/'/g, '&#039;');
  }
}
