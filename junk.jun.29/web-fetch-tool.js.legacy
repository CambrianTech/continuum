/**
 * WebFetch Tool - handles web page fetching
 */

const fetch = require('node-fetch');

class WebFetchTool {
  constructor() {
    this.name = 'WebFetch';
  }

  async processResponse(response) {
    const results = [];
    const matches = response.match(/WEBFETCH:\s*(https?:\/\/[^\s\n]+)/gi);
    
    if (matches) {
      for (const match of matches) {
        const urlMatch = match.match(/WEBFETCH:\s*(https?:\/\/[^\s\n]+)/i);
        if (urlMatch) {
          const url = urlMatch[1];
          console.log(`🌐 WebFetch executing: ${url}`);
          
          try {
            const content = await this.fetchPage(url);
            results.push({
              tool: 'WebFetch',
              command: url,
              result: content,
              success: true
            });
          } catch (error) {
            results.push({
              tool: 'WebFetch',
              command: url,
              result: `Error: ${error.message}`,
              success: false
            });
          }
        }
      }
    }
    
    return results;
  }

  async fetchPage(url) {
    const response = await fetch(url);
    const text = await response.text();
    
    // Simple HTML to text conversion
    const plainText = text
      .replace(/<script[^>]*>[\s\S]*?<\/script>/gi, '')
      .replace(/<style[^>]*>[\s\S]*?<\/style>/gi, '')
      .replace(/<[^>]+>/g, ' ')
      .replace(/\s+/g, ' ')
      .trim();
    
    return plainText.substring(0, 2000);
  }
}

module.exports = WebFetchTool;