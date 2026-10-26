/** <chat-widget> styles, The settings face and the canvas region (design bench). Moved verbatim from ChatWidget.ts; the
 *  order of `chatWidgetStyles` is the cascade order. */
import { css } from 'lit';

export const settingsCanvasStyles = css`
    /* ================= SETTINGS FACE ================= */
    /* The operator panel: quiet sections, the covenant as a readable document,
     * one primary action. Same tokens as everything else — settings should
     * feel like the calm room of the house. */
    .settings {
      max-width: 720px;
      margin: 0 auto;
      padding: var(--spacing-md) var(--spacing-lg);
      display: flex;
      flex-direction: column;
      gap: 14px;
      overflow-y: auto;
    }
    .set-title {
      font-size: 18px;
      font-weight: 800;
      letter-spacing: 0.02em;
    }
    .set-awaiting, .set-error {
      padding: var(--spacing-md);
      color: var(--content-secondary);
    }
    .set-error {
      border: 1px solid color-mix(in srgb, var(--status-warning, #e0a458) 45%, transparent);
      border-radius: var(--radius-sm);
      color: var(--status-warning, #e0a458);
    }
    .set-fallback { margin-top: 6px; font-size: 11px; opacity: 0.8; }
    .set-section {
      border: 1px solid var(--border-subtle);
      border-radius: var(--radius-sm);
      background: color-mix(in srgb, var(--surface, #0b1220) 60%, transparent);
      padding: 12px 14px;
      display: flex;
      flex-direction: column;
      gap: 8px;
    }
    .set-head {
      display: flex;
      align-items: center;
      justify-content: space-between;
    }
    .set-head h3 {
      font-size: 12px;
      text-transform: uppercase;
      letter-spacing: 0.1em;
      color: var(--content-secondary);
    }
    .set-state {
      font-size: 10px;
      text-transform: uppercase;
      letter-spacing: 0.08em;
      padding: 2px 8px;
      border-radius: 999px;
      border: 1px solid var(--border-subtle);
      color: var(--content-secondary);
    }
    .set-state[data-on] {
      border-color: color-mix(in srgb, var(--status-success, #4caf7d) 45%, transparent);
      color: var(--status-success, #4caf7d);
      background: color-mix(in srgb, var(--status-success, #4caf7d) 10%, transparent);
    }
    .set-sub { font-size: 12px; color: var(--content-secondary); line-height: 1.5; }
    .set-covenant {
      font-size: 11px;
      line-height: 1.55;
      white-space: pre-wrap;
      padding: 10px 12px;
      border-left: 3px solid var(--accent-primary);
      background: color-mix(in srgb, var(--accent-primary) 5%, transparent);
      border-radius: 0 var(--radius-sm) var(--radius-sm) 0;
      max-height: 260px;
      overflow-y: auto;
    }
    .set-receipt { font-size: 10.5px; color: var(--content-secondary); }
    .set-actions { display: flex; gap: 8px; }
    .set-btn {
      font: inherit;
      font-size: 12px;
      padding: 6px 14px;
      border-radius: var(--radius-sm);
      border: 1px solid var(--border-subtle);
      background: transparent;
      color: var(--content-primary);
      cursor: pointer;
    }
    .set-btn-primary {
      border-color: color-mix(in srgb, var(--accent-primary) 55%, transparent);
      background: color-mix(in srgb, var(--accent-primary) 14%, transparent);
      color: var(--accent-primary);
      font-weight: 700;
    }
    .set-btn:focus-visible { outline: 2px solid var(--accent-primary); outline-offset: 2px; }
    .set-count {
      font-variant-numeric: tabular-nums;
      font-size: 11px;
      color: var(--content-secondary);
    }
    .set-table-wrap { overflow-x: auto; }
    .set-table {
      width: 100%;
      border-collapse: collapse;
      font-size: 11.5px;
    }
    .set-table th {
      text-align: left;
      font-size: 9.5px;
      text-transform: uppercase;
      letter-spacing: 0.08em;
      color: var(--content-secondary);
      padding: 4px 8px;
      border-bottom: 1px solid var(--border-subtle);
    }
    .set-table td { padding: 5px 8px; border-bottom: 1px solid color-mix(in srgb, var(--border-subtle) 45%, transparent); }
    .set-gene { font-weight: 600; }
    .set-base { color: var(--content-secondary); font-size: 10.5px; }
    .set-num { font-variant-numeric: tabular-nums; }
    .set-ok { color: var(--status-success, #4caf7d); }
    .set-dim { color: var(--content-secondary); opacity: 0.7; }
    .set-table [data-lift='up'] { color: var(--status-success, #4caf7d); }
    .set-table [data-lift='down'] { color: var(--status-warning, #e0a458); }

    /* ================= CANVAS REGION (design-bench) ================= */
    /* The run room's stage (purpose "canvas"): the persona's rendered page
     * live in a sandboxed frame (or the last observed pixels), a compact
     * facts header, and the craft scorecard under the stage. The STAGE gets
     * the room — header and score stay quiet chrome. Same tokens as every
     * face; the artifact paints its own colours inside the sandbox. */
    .canvas-region {
      display: flex;
      flex-direction: column;
      gap: var(--spacing-sm);
      padding: var(--spacing-md) var(--spacing-lg);
      height: 100%;
      min-height: 0;
      overflow-y: auto;
    }
    .canvas-head {
      display: flex;
      align-items: baseline;
      gap: var(--spacing-sm);
      min-width: 0;
      flex-wrap: wrap;
    }
    .canvas-title {
      font-family: var(--font-mono);
      font-weight: 700;
      font-size: 13px;
      color: var(--content-primary);
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
      min-width: 0;
    }
    .canvas-head-facts {
      margin-left: auto;
      display: inline-flex;
      align-items: baseline;
      gap: var(--spacing-sm);
      flex-shrink: 0;
    }
    .canvas-persona {
      font-weight: 700;
      font-size: 11px;
      color: var(--content-primary);
    }
    .canvas-observed {
      font-size: 10px;
      color: var(--content-secondary);
      font-variant-numeric: tabular-nums;
    }
    .canvas-chip {
      font-size: 8.5px;
      letter-spacing: 0.08em;
      text-transform: uppercase;
      padding: 1px 6px;
      border-radius: 999px;
      border: 1px solid var(--border-subtle);
      color: var(--content-secondary);
      font-variant-numeric: tabular-nums;
      white-space: nowrap;
    }
    /* The gate chip: all-clean settles into success; a failing gate is the
     * alarm tone — the scorecard below carries the receipt. */
    .canvas-chip-score[data-clean='yes'] {
      border-color: color-mix(in srgb, var(--status-success, #4caf7d) 45%, transparent);
      color: var(--status-success, #4caf7d);
      background: color-mix(in srgb, var(--status-success, #4caf7d) 10%, transparent);
    }
    .canvas-chip-score[data-clean='no'] {
      border-color: color-mix(in srgb, var(--status-error, #d9534f) 45%, transparent);
      color: var(--status-error, #d9534f);
      background: color-mix(in srgb, var(--status-error, #d9534f) 10%, transparent);
    }
    /* The stage — the page itself. The frame fills the region's remainder;
     * a light inset border keeps the artifact's own background honest against
     * the shell without recolouring it. */
    .canvas-stage {
      flex: 1;
      min-height: 240px;
      display: flex;
      border: 1px solid var(--border-subtle);
      border-radius: var(--radius-sm);
      overflow: hidden;
      background: var(--widget-surface, rgba(255, 255, 255, 0.03));
    }
    .canvas-stage-frame {
      flex: 1;
      width: 100%;
      border: none;
      background: #fff; /* a page with no painted body is honestly white, as a browser tab is */
    }
    .canvas-stage-shot {
      flex: 1;
      width: 100%;
      min-width: 0;
      object-fit: contain;
      object-position: top left;
    }
    /* The craft scorecard — gate rows (failures lead), receipts inline. */
    .canvas-score {
      display: flex;
      flex-direction: column;
      gap: 4px;
    }
    ul.canvas-checks {
      display: flex;
      flex-direction: column;
      gap: 3px;
    }
    .canvas-check {
      display: flex;
      align-items: baseline;
      gap: 7px;
      font-size: 10.5px;
      padding: 3px 8px;
      border-left: 2px solid var(--border-subtle);
      border-radius: 0 var(--radius-sm) var(--radius-sm) 0;
      background: var(--widget-surface, rgba(255, 255, 255, 0.03));
      color: var(--content-secondary);
      min-width: 0;
    }
    .canvas-check[data-passed='no'] {
      border-left-color: var(--status-error, #d9534f);
    }
    .canvas-check[data-passed='yes'] {
      border-left-color: var(--status-success, #4caf7d);
    }
    .canvas-check-dot {
      flex-shrink: 0;
      width: 7px;
      height: 7px;
      border-radius: 50%;
      align-self: center;
      background: var(--status-success, #4caf7d);
    }
    .canvas-check[data-passed='no'] .canvas-check-dot {
      background: var(--status-error, #d9534f);
    }
    .canvas-check-tier {
      flex-shrink: 0;
      font-size: 8px;
      font-weight: 700;
      letter-spacing: 0.1em;
      text-transform: uppercase;
      color: var(--content-secondary);
    }
    .canvas-check-name {
      color: var(--content-primary);
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
      min-width: 0;
    }
    .canvas-check-detail {
      margin-left: auto;
      flex-shrink: 0;
      font-family: var(--font-mono);
      font-size: 9.5px;
      font-variant-numeric: tabular-nums;
    }
    .canvas-judge {
      align-self: flex-end;
      font-size: 9.5px;
      letter-spacing: 0.06em;
      text-transform: uppercase;
      color: var(--content-secondary);
      font-variant-numeric: tabular-nums;
    }
    .canvas-snapshot-banner,
    .canvas-awaiting {
      font-size: 9.5px;
      font-style: italic;
      color: var(--content-secondary);
    }
    .canvas-awaiting p:first-child {
      font-size: 12px;
      font-style: normal;
      color: var(--content-primary);
    }
    .canvas-awaiting-sub {
      margin-top: 4px;
    }

`;
