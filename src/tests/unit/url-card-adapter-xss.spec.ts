/**
 * URLCardAdapter XSS hardening tests (#1159).
 *
 * Asserts that every interpolation site in `renderContent` escapes
 * attacker-controlled input AND that `href="${url}"` neutralizes
 * `javascript:` / `data:` / `vbscript:` schemes. These are the gaps
 * left open by PR-1 (which only closed the `innerHTML` Lit-reactivity
 * hole) and called out in the PR-1 doc comment as "the URL-metadata
 * XSS surface" requiring a follow-up PR.
 */

import { describe, it, expect } from 'vitest';
import { URLCardAdapter } from '../../widgets/chat/adapters/URLCardAdapter';

type RenderableData = {
  url: string;
  title?: string;
  description?: string;
  siteName?: string;
  favicon?: string;
  imageUrl?: string;
  domain: string;
  isSecure: boolean;
  originalText: string;
};

function renderWith(overrides: Partial<RenderableData>): string {
  const adapter = new URLCardAdapter();
  const data: RenderableData = {
    url: 'https://example.com/x',
    title: 'Title',
    description: 'Description',
    siteName: 'example.com',
    favicon: 'https://example.com/favicon.ico',
    domain: 'example.com',
    isSecure: true,
    originalText: 'check this https://example.com/x',
    ...overrides,
  };
  // renderContent is the string-builder path; renderMessageElement
  // runs the same string through `template.innerHTML` materialization,
  // so the string-level escape is the load-bearing surface.
  return adapter.renderContent(data as never, 'user-id');
}

describe('URLCardAdapter XSS — per-field HTML escape', () => {
  it('escapes <script> in the additional-text slot (originalText)', () => {
    const html = renderWith({
      url: 'https://example.com/x',
      originalText: '<script>alert(1)</script> https://example.com/x',
    });
    expect(html).not.toContain('<script>alert(1)</script>');
    expect(html).toContain('&lt;script&gt;alert(1)&lt;/script&gt;');
  });

  it('escapes <script> in the title field', () => {
    const html = renderWith({ title: '<script>alert("title")</script>' });
    expect(html).not.toContain('<script>alert("title")</script>');
    expect(html).toContain('&lt;script&gt;');
  });

  it('escapes <script> in the description field', () => {
    const html = renderWith({ description: '<img src=x onerror=alert(1)>' });
    expect(html).not.toContain('<img src=x onerror=alert(1)>');
    expect(html).toContain('&lt;img src=x onerror=alert(1)&gt;');
  });

  it('escapes <script> in the siteName field', () => {
    const html = renderWith({ siteName: '"><script>alert("siteName")</script>' });
    expect(html).not.toContain('"><script>alert("siteName")</script>');
    expect(html).toContain('&lt;script&gt;');
    expect(html).toContain('&quot;&gt;&lt;script&gt;');
  });

  it('escapes the favicon URL (belt-and-suspenders)', () => {
    const html = renderWith({
      favicon: 'https://google.com/favicons?domain=evil"onerror=alert(1)',
    });
    expect(html).not.toContain('"onerror=alert(1)');
    expect(html).toContain('&quot;onerror=alert(1)');
  });

  it('escapes the domain field (used in 3 places)', () => {
    const html = renderWith({ domain: '"><script>alert("domain")</script>' });
    expect(html).not.toContain('"><script>alert("domain")</script>');
    expect(html).toContain('&quot;&gt;&lt;script&gt;');
  });
});

describe('URLCardAdapter XSS — attribute-context escape', () => {
  it('escapes double-quote breakout in the URL attribute (data-url + title=)', () => {
    const html = renderWith({
      url: 'https://example.com/x"><script>alert(1)</script>',
    });
    expect(html).not.toContain('"><script>');
    expect(html).toMatch(/data-url="https:\/\/example\.com\/x&quot;&gt;&lt;script&gt;/);
    expect(html).toMatch(/title="https:\/\/example\.com\/x&quot;&gt;&lt;script&gt;/);
  });

  it('escapes & properly so &amp; is not double-encoded', () => {
    const html = renderWith({ title: 'A & B' });
    expect(html).toContain('A &amp; B');
    expect(html).not.toContain('&amp;amp;');
  });
});

describe('URLCardAdapter XSS — href scheme neutralization', () => {
  it('neutralizes javascript: URL in the href slot', () => {
    const html = renderWith({ url: 'javascript:alert(1)' });
    expect(html).toMatch(/href="#"/);
    expect(html).not.toMatch(/href="javascript:/i);
  });

  it('neutralizes case-mixed JavaScript: URL in the href slot', () => {
    const html = renderWith({ url: 'JaVaScRiPt:alert(1)' });
    expect(html).toMatch(/href="#"/);
    expect(html).not.toMatch(/href="JaVaScRiPt:/);
  });

  it('neutralizes data: URL in the href slot', () => {
    const html = renderWith({ url: 'data:text/html,<script>alert(1)</script>' });
    expect(html).toMatch(/href="#"/);
    expect(html).not.toMatch(/href="data:/);
  });

  it('neutralizes vbscript: URL in the href slot', () => {
    const html = renderWith({ url: 'vbscript:msgbox(1)' });
    expect(html).toMatch(/href="#"/);
    expect(html).not.toMatch(/href="vbscript:/);
  });
});

describe('URLCardAdapter XSS — href whitelist preservation', () => {
  it('preserves http://, https://, mailto:, tel:, ftp: in the href slot', () => {
    for (const safeUrl of [
      'http://example.com/x',
      'https://example.com/x',
      'mailto:hi@example.com',
      'tel:+15555550123',
      'ftp://ftp.example.com/file',
    ]) {
      const html = renderWith({ url: safeUrl });
      expect(html).toContain(`href="${safeUrl}"`);
    }
  });

  it('preserves protocol-relative URLs in the href slot', () => {
    const html = renderWith({ url: '//cdn.example.com/asset' });
    expect(html).toContain('href="//cdn.example.com/asset"');
  });

  it('preserves same-document fragment URLs in the href slot', () => {
    const html = renderWith({ url: '#section-1' });
    expect(html).toContain('href="#section-1"');
  });

  it('treats empty/whitespace URL as #', () => {
    const empty = renderWith({ url: '' });
    expect(empty).toMatch(/href="#"/);
    const ws = renderWith({ url: '   ' });
    expect(ws).toMatch(/href="#"/);
  });
});
