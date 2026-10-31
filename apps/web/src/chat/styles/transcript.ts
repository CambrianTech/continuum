/** <chat-widget> styles, The transcript: reading column, message rows, receipts, code blocks, typing line. Moved verbatim from ChatWidget.ts; the
 *  order of `chatWidgetStyles` is the cascade order. */
import { css } from 'lit';

export const transcriptStyles = css`
    /* A centered reading column — the conversation shouldn't hug the left edge of a
       wide desktop panel with a dead void on the right. */
    .messages {
      max-width: 880px;
      margin: 0 auto;
    }
    /* "N new messages · Jump to present": a reader parked far up (transcriptPager.ts). */
    .jump-present {
      display: block;
      margin: 0 auto var(--spacing-xs);
      padding: 4px 12px;
      border: 1px solid var(--border-color, #384350);
      border-radius: 999px;
      background: var(--surface-raised, #1d2430);
      color: var(--content-accent, #35d0e0);
      font: inherit;
      font-size: 0.85rem;
      cursor: pointer;
    }
    /* Stands in for rows outside the rendered window (transcriptWindow.ts). */
    .messages > .transcript-spacer {
      list-style: none;
      padding: 0;
      margin: 0;
    }
    .messages .msg {
      display: flex;
      gap: var(--spacing-sm);
      padding: 6px 0;
    }
    /* Tool-act receipt rows (#243) — the collapsed "Read 2 files, ran a command ›"
       line between speech, expanding IN PLACE via native <details>. Quiet by
       design: the work is one gesture away, never shouting over the words. */
    .messages .act-group {
      list-style: none;
      padding: 1px 0;
      margin-left: calc(28px + var(--spacing-sm));
      font-size: 11px;
      color: var(--content-secondary);
    }
    .act-group summary {
      display: inline-flex;
      align-items: center;
      gap: 6px;
      cursor: pointer;
      user-select: none;
      padding: 2px 8px;
      border-radius: var(--radius-sm);
      border: 1px solid var(--border-subtle);
      background: color-mix(in srgb, var(--surface, #0b1220) 40%, transparent);
    }
    .act-group summary::-webkit-details-marker { display: none; }
    .act-group summary::after { content: '›'; opacity: 0.6; transition: transform 0.15s; }
    .act-group details[open] summary::after { transform: rotate(90deg); }
    .act-gear { opacity: 0.75; }
    .act-gear.act-failed { color: var(--status-warning, #e0a458); }
    .act-actor { font-weight: 600; color: var(--content-primary); }
    .act-count {
      font-size: 9px;
      padding: 0 5px;
      border-radius: 8px;
      background: color-mix(in srgb, var(--content-secondary) 18%, transparent);
      font-variant-numeric: tabular-nums;
    }
    .act-list {
      list-style: none;
      margin: 4px 0 2px;
      padding: 0 0 0 14px;
      border-left: 1px solid var(--border-subtle);
    }
    .act-item {
      display: flex;
      align-items: baseline;
      gap: 7px;
      padding: 1.5px 0;
      font-family: var(--font-mono, ui-monospace, monospace);
      font-size: 10.5px;
    }
    .act-item .act-mark { color: var(--status-success, #4caf7d); }
    .act-item.act-failed .act-mark { color: var(--status-error, #e05858); }
    .act-tool { color: var(--accent-primary); }
    .act-obj {
      color: var(--content-secondary);
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
      max-width: 420px;
    }
    .act-time { margin-left: auto; opacity: 0.5; font-size: 9px; }
    /* Continuation rows (same sender, grouped upstream): tuck the body into the
       sender's column — tight runs, the classic chat grouping. */
    .messages .msg.continues {
      padding: 1px 0;
      margin-left: calc(28px + var(--spacing-sm));
    }
    .msg-glyph {
      flex: none;
      width: 28px;
      height: 28px;
      display: flex;
      align-items: center;
      justify-content: center;
    }
    .msg-avatar {
      width: 28px;
      height: 28px;
      border-radius: 50%;
      object-fit: cover;
      object-position: center top;
      border: 1px solid var(--hud-accent-border);
    }
    /* The flex child holding the message text MUST be allowed to shrink below its
       content's intrinsic width, or the bubble overflows its container and clips
       (mid-word) instead of wrapping — the canonical flexbox min-width:auto trap.
       Applies at every width; the bubble should never overflow the viewport. */
    .msg-body {
      min-width: 0;
      flex: 1;
    }
    .msg-head {
      display: flex;
      align-items: baseline;
      gap: var(--spacing-sm);
    }
    .sender {
      font-weight: 600;
      color: var(--content-accent);
    }
    .heard {
      margin-left: 6px;
      padding: 0 6px;
      border: 1px solid var(--status-online);
      border-radius: var(--radius-sm);
      font-family: var(--font-mono);
      font-size: 9px;
      letter-spacing: 0.06em;
      color: var(--status-online);
      opacity: 0.85;
    }
    .time {
      color: var(--content-secondary);
      font-size: 11px;
    }
    .content {
      white-space: pre-wrap;
      word-break: break-word;
      /* Cap the measure — prose past ~68ch is hard to read; a lovable chat never
         stretches a bubble the full 1000px+ of a desktop panel. */
      max-width: 68ch;
      background: var(--message-assistant-background);
      border: 1px solid var(--message-assistant-border);
      border-radius: var(--radius-lg);
      padding: var(--spacing-sm) var(--spacing-md);
      margin-top: 3px;
    }
    /* Digest tier ([[perception-resolution-contract]]) — a flooding message renders
       collapsed: head + a mechanical tail line + an expand affordance. No animation
       on toggle — an instant swap is simple and already honors reduced-motion. */
    .digest-tail {
      margin-top: var(--spacing-sm);
      padding-top: var(--spacing-xs);
      border-top: 1px dashed var(--border-subtle);
      font-family: var(--font-mono);
      font-size: 11px;
      color: var(--content-secondary);
      font-variant-numeric: tabular-nums;
    }
    .digest-histogram {
      /* The repetition callout — the named degenerate pattern, warmed so it reads
         as the anomaly signal, not more body text. */
      color: #ffb020;
    }
    button.digest-toggle {
      display: inline-block;
      margin-top: var(--spacing-xs);
      padding: 2px 10px;
      border: 1px solid var(--border-subtle);
      border-radius: var(--radius-sm);
      background: var(--button-secondary-background);
      color: var(--content-secondary);
      font-size: 11px;
      font-weight: 600;
      letter-spacing: 0.04em;
      cursor: pointer;
    }
    button.digest-toggle:hover,
    button.digest-toggle:focus-visible {
      color: var(--content-accent);
      border-color: var(--border-accent);
      outline: none;
    }
    /* Fenced code + commands — personas speak these constantly; a monospace block reads
       as an action, not noise. Scrolls inside itself so a long command never widens the bubble. */
    /* Expandable code block — a summary bar you can toggle; long commands/outputs stay
       collapsed so they never bury the conversation. */
    .code-collapsible {
      margin: 7px 0 3px;
      background: rgba(0, 0, 0, 0.32);
      border: 1px solid var(--border-subtle);
      border-radius: 7px;
      overflow: hidden;
    }
    .code-collapsible summary {
      cursor: pointer;
      padding: 6px 11px;
      font-family: var(--font-mono);
      font-size: 10px;
      letter-spacing: 0.05em;
      text-transform: uppercase;
      color: var(--content-secondary);
      display: flex;
      align-items: center;
      gap: 8px;
      list-style: none;
      user-select: none;
    }
    .code-collapsible summary::-webkit-details-marker {
      display: none;
    }
    .code-collapsible summary::before {
      content: '▸';
      color: var(--content-accent);
      font-size: 9px;
    }
    .code-collapsible[open] summary::before {
      content: '▾';
    }
    .code-collapsible .code-count {
      margin-left: auto;
      opacity: 0.7;
      text-transform: none;
      letter-spacing: 0;
    }
    /* Line-numbered body: gutter + code side by side, one shared line-height so
       numbers stay glued to their lines; only the code pane scrolls horizontally. */
    .code-body {
      display: flex;
      align-items: flex-start;
    }
    .code-gutter {
      flex: none;
      padding: 2px 0 8px 11px;
      text-align: right;
      min-width: 2.5ch;
      user-select: none;
      white-space: pre;
      font-family: var(--font-mono);
      font-size: 12.5px;
      line-height: 1.5;
      color: var(--content-secondary);
      opacity: 0.5;
    }
    .code-collapsible pre {
      flex: 1;
      min-width: 0;
      margin: 0;
      padding: 2px 11px 8px 10px;
      overflow-x: auto;
      font-family: var(--font-mono);
      font-size: 12.5px;
      line-height: 1.5;
      color: var(--content-accent);
    }
    .code-collapsible pre code {
      white-space: pre;
    }
    /* "+K more lines" expander for big blocks — the head stays visible, the rest
       is one click away; expanded, the gutter numbering continues seamlessly. */
    .code-more summary {
      cursor: pointer;
      padding: 4px 11px 6px;
      font-family: var(--font-mono);
      font-size: 11px;
      color: var(--content-secondary);
      list-style: none;
      user-select: none;
      border-top: 1px solid var(--border-subtle);
    }
    .code-more summary:hover {
      color: var(--content-accent);
    }
    .code-more summary::-webkit-details-marker {
      display: none;
    }
    .code-more[open] summary {
      display: none;
    }
    /* highlight.js dark palette (github-dark), scoped into the shadow root. */
    .hljs {
      color: #c9d1d9;
    }
    .hljs-comment,
    .hljs-quote {
      color: #8b949e;
      font-style: italic;
    }
    .hljs-keyword,
    .hljs-selector-tag,
    .hljs-built_in,
    .hljs-name,
    .hljs-tag {
      color: #ff7b72;
    }
    .hljs-string,
    .hljs-attr,
    .hljs-template-tag,
    .hljs-addition {
      color: #a5d6ff;
    }
    .hljs-title,
    .hljs-section,
    .hljs-type {
      color: #d2a8ff;
    }
    .hljs-number,
    .hljs-literal,
    .hljs-variable,
    .hljs-selector-attr {
      color: #79c0ff;
    }
    .hljs-symbol,
    .hljs-bullet,
    .hljs-meta {
      color: #56d364;
    }
    .hljs-attribute {
      color: #ffa657;
    }
    .hljs-emphasis {
      font-style: italic;
    }
    .hljs-strong {
      font-weight: 600;
    }
    .code-block {
      margin: 7px 0 3px;
      padding: 8px 11px;
      background: rgba(0, 0, 0, 0.32);
      border: 1px solid var(--border-subtle);
      border-radius: 7px;
      overflow-x: auto;
      font-family: var(--font-mono);
      font-size: 12.5px;
      line-height: 1.5;
      color: var(--content-accent);
    }
    .code-block code {
      white-space: pre;
    }
    .inline-code {
      font-family: var(--font-mono);
      font-size: 0.88em;
      background: rgba(0, 0, 0, 0.28);
      border: 1px solid var(--border-subtle);
      border-radius: 4px;
      padding: 1px 5px;
      color: var(--content-accent);
    }
    form.compose {
      display: flex;
      gap: var(--spacing-sm);
      padding: var(--spacing-md) var(--spacing-lg);
      border-top: 1px solid var(--border-subtle);
      background: var(--widget-input-area-background);
    }
    input {
      flex: 1;
      padding: var(--spacing-sm) var(--spacing-md);
      border: 1px solid var(--input-border);
      border-radius: var(--radius-lg);
      background: var(--input-background);
      color: var(--input-text);
      font: inherit;
    }
    input:focus {
      outline: none;
      border-color: var(--input-border-focus);
    }
    input::placeholder {
      color: var(--input-placeholder);
    }
    button {
      padding: var(--spacing-sm) var(--spacing-lg);
      border: 0;
      border-radius: var(--radius-lg);
      background: var(--button-primary-background);
      color: var(--button-primary-text);
      font: inherit;
      font-weight: 600;
      cursor: pointer;
    }
    button[disabled] {
      opacity: 0.5;
      cursor: default;
    }
    /* "(xyz, abc) is responding…" — ONE grey line pinned between the last
       message and the compose box; the name list updates as turns start/settle.
       Never wraps, never grows: overflow elides. */
    .responding-line {
      padding: 3px 14px 4px;
      font-size: 11.5px;
      color: var(--content-secondary);
      white-space: nowrap;
      overflow: hidden;
      text-overflow: ellipsis;
      opacity: 0.85;
    }
    .send-error {
      color: var(--content-error);
      font-size: 12px;
      padding: 0 var(--spacing-lg) var(--spacing-sm);
    }
    .connecting {
      display: grid;
      place-items: center;
      flex: 1;
      color: var(--content-secondary);
    }
    .render-error {
      padding: var(--spacing-lg);
      color: var(--content-error);
      font-family: var(--font-mono);
      font-size: 13px;
      white-space: pre-wrap;
    }

`;
