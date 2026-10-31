/** <chat-widget> styles, The serving console, the grid and nodes strips, the rooms widget and selectable cells. Moved verbatim from ChatWidget.ts; the
 *  order of `chatWidgetStyles` is the cascade order. */
import { css } from 'lit';

export const consoleStyles = css`
    /* SERVING CONSOLE (purpose="serving") — the machine room center-stage:
     * per-node panels, headline tok/s numeral, full-width instrument, arm
     * bank, control-loop feed. Console legibility: big numerals, wide
     * instruments, generous rhythm. */
    .srv-console {
      display: flex;
      flex-direction: column;
      gap: var(--spacing-lg);
      padding: var(--spacing-lg);
      overflow-y: auto;
      height: 100%;
    }
    .srv-snapshot {
      align-self: flex-start;
      font-size: 10px;
      letter-spacing: 0.08em;
      text-transform: uppercase;
      padding: 2px 8px;
      border: 1px solid var(--border-subtle);
      border-radius: var(--radius-sm);
      color: var(--content-secondary);
    }
    .srv-node {
      border: 1px solid var(--border-subtle);
      border-radius: var(--radius-md, 8px);
      background: var(--widget-surface, rgba(255, 255, 255, 0.02));
      padding: var(--spacing-md) var(--spacing-lg) var(--spacing-lg);
      display: flex;
      flex-direction: column;
      gap: var(--spacing-md);
    }
    .srv-banner {
      display: flex;
      align-items: baseline;
      gap: var(--spacing-md);
      min-width: 0;
    }
    .srv-node-name {
      display: inline-flex;
      align-items: center;
      gap: 6px;
      font-size: 11px;
      letter-spacing: 0.08em;
      text-transform: uppercase;
      color: var(--content-secondary);
      flex-shrink: 0;
    }
    .srv-node-name[data-local] {
      color: var(--content-primary);
    }
    .srv-local-chip {
      font-size: 8px;
      letter-spacing: 0.1em;
      padding: 1px 5px;
      border-radius: var(--radius-sm);
      background: var(--accent-primary);
      color: var(--surface, #0a0e14);
      text-transform: uppercase;
    }
    .srv-model {
      font-family: var(--font-mono);
      font-size: 16px;
      font-weight: 700;
      color: var(--content-primary);
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
      min-width: 0;
    }
    .srv-pulse {
      width: 8px;
      height: 8px;
      border-radius: 50%;
      background: var(--status-warning, #e0a458);
      flex-shrink: 0;
      align-self: center;
    }
    .srv-pulse[data-ready='true'] {
      background: var(--status-success, #4caf7d);
      box-shadow: 0 0 6px var(--status-success, #4caf7d);
    }
    .srv-lanes {
      font-size: 11px;
      color: var(--content-secondary);
      font-variant-numeric: tabular-nums;
      flex-shrink: 0;
    }
    .srv-degraded {
      font-size: 11px;
      color: var(--status-warning, #e0a458);
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
    .srv-headline {
      margin-left: auto;
      display: inline-flex;
      align-items: baseline;
      gap: 6px;
      flex-shrink: 0;
    }
    .srv-headline-num {
      font-family: var(--font-mono);
      font-size: 34px;
      font-weight: 700;
      line-height: 1;
      color: var(--accent-primary);
      font-variant-numeric: tabular-nums;
    }
    .srv-headline-unit {
      font-size: 10px;
      letter-spacing: 0.08em;
      text-transform: uppercase;
      color: var(--content-secondary);
    }
    .srv-instrument .gauge {
      padding: 0;
    }
    .srv-instrument .gauge svg {
      height: 120px;
    }
    .srv-section-label {
      display: block;
      font-size: 9px;
      letter-spacing: 0.12em;
      text-transform: uppercase;
      color: var(--content-secondary);
      margin-bottom: 4px;
    }
    .srv-bank-arms {
      padding: 0;
      gap: 6px;
    }
    .srv-bank .serving-arm {
      padding: 5px 0 7px;
    }
    .srv-bank .arm-label {
      font-size: 11px;
    }
    .arm-reward {
      display: block;
      font-family: var(--font-mono);
      font-size: 9px;
      color: var(--content-secondary);
      font-variant-numeric: tabular-nums;
    }
    .srv-bank .arm-bar {
      height: 3px;
    }
    .srv-feed-events {
      padding: 0;
      gap: 4px;
    }
    .srv-feed .serving-event {
      font-size: 11px;
      padding: 4px 8px;
    }
    .srv-awaiting {
      margin: auto;
      text-align: center;
      color: var(--content-secondary);
    }
    .srv-awaiting-title {
      font-size: 14px;
      letter-spacing: 0.1em;
      text-transform: uppercase;
      margin-bottom: 6px;
    }
    .srv-awaiting-line {
      font-size: 11px;
    }
    /* GRID view — the node panel's sections (resources / serving) and the
     * node name sized as the panel's identity when no model banner leads. */
    .grid-node-name {
      font-size: 14px;
      letter-spacing: 0.1em;
    }
    .grid-section {
      display: flex;
      flex-direction: column;
      gap: 4px;
    }
    .grid-section .gauge {
      padding: 0;
    }
    .grid-section .gauge svg {
      height: 88px;
    }
    /* NODES strip — the factory sidebar's "1/1 nodes online": pulse dot + host
     * name + role chip per attested node. */
    .nodes-online {
      display: inline-flex;
      align-items: center;
      gap: 5px;
      margin-left: auto;
      font-size: 9px;
      letter-spacing: 0.05em;
      color: var(--content-secondary);
    }
    ul.nodes {
      list-style: none;
      margin: 0;
      padding: 0 var(--spacing-md) var(--spacing-sm);
    }
    .node-row {
      display: flex;
      align-items: center;
      gap: var(--spacing-sm);
      padding: 2px 0;
      font-size: 12px;
      color: var(--content-primary);
    }
    .node-dot {
      width: 7px;
      height: 7px;
      border-radius: 50%;
      background: var(--status-offline, #555);
      flex: none;
    }
    .node-dot[data-on] {
      background: var(--status-online, #3fb950);
      box-shadow: 0 0 5px var(--status-online, #3fb950);
    }
    .node-name {
      font-family: var(--font-mono);
      font-size: 11px;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
    .node-role {
      margin-left: auto;
      font-size: 8px;
      text-transform: uppercase;
      letter-spacing: 0.08em;
      padding: 1px 6px;
      clip-path: polygon(4px 0, 100% 0, 100% 100%, 0 100%);
      background: rgba(0, 212, 255, 0.14);
      color: var(--content-accent);
    }
    /* Rooms widget — the live room set (brick 1) drawn by <rooms-panel>: filter
     * facets (All/Rooms/DMs — a facet over the neutral group key), purpose
     * descriptions under names, unread pills, and the (honestly disabled)
     * start-conversation affordance. Light-DOM element → styled from here. */
    rooms-panel {
      display: block;
    }
    .rooms-facets {
      display: inline-flex;
      gap: 2px;
      margin-left: auto;
      margin-right: var(--spacing-sm);
    }
    .rooms-facet {
      padding: 1px 7px;
      border: 1px solid var(--border-subtle);
      border-radius: var(--radius-sm);
      background: transparent;
      color: var(--content-secondary);
      font-size: 8.5px;
      font-weight: 600;
      letter-spacing: 0.05em;
      cursor: pointer;
      line-height: 1.5;
    }
    .rooms-facet:hover {
      color: var(--content-primary);
      border-color: var(--border-accent, rgba(0, 212, 255, 0.4));
    }
    .rooms-facet[data-active] {
      background: var(--content-accent);
      border-color: var(--content-accent);
      color: var(--surface, #0b0d12);
    }
    .rooms-empty {
      padding: var(--spacing-xs) var(--spacing-md) var(--spacing-sm);
      color: var(--content-secondary);
      font-size: 11px;
      font-style: italic;
    }
    .rooms-start {
      display: block;
      width: calc(100% - 2 * var(--spacing-md));
      margin: 2px var(--spacing-md) var(--spacing-sm);
      padding: 3px 0;
      border: 1px dashed var(--border-subtle);
      border-radius: var(--radius-sm);
      background: transparent;
      color: var(--content-secondary);
      font-size: 10.5px;
      text-align: center;
    }
    .rooms-start[disabled] {
      opacity: 0.55;
      cursor: default;
    }
    ul.cells {
      list-style: none;
      margin: 0;
      padding: 0 var(--spacing-sm) var(--spacing-sm);
    }
    .cell {
      display: flex;
      align-items: center;
      gap: var(--spacing-sm);
      padding: var(--spacing-xs) var(--spacing-sm);
      border-radius: var(--radius-sm);
      color: var(--content-secondary);
      font-size: 13px;
      cursor: default;
    }
    .cell[data-status='active'] {
      background: var(--button-secondary-background);
      color: var(--content-primary);
      border-left: 2px solid var(--content-accent);
    }
    /* Selectable cells (the rooms rail) — a pick is a nav/select round-trip; the
     * active highlight moves only when the refocused envelope arrives. */
    .cell[data-selectable] {
      cursor: pointer;
    }
    .cell[data-selectable]:hover {
      background: var(--widget-surface, rgba(255, 255, 255, 0.05));
      color: var(--content-primary);
    }
    .cell[data-selectable]:focus-visible {
      outline: 1px solid var(--content-accent);
      outline-offset: -1px;
    }
    .cell-body {
      flex: 1;
      min-width: 0;
    }
    .cell-title {
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
    .cell-subtitle {
      font-size: 11px;
      color: var(--content-secondary);
    }
    .cell-count {
      min-width: 18px;
      padding: 0 5px;
      text-align: center;
      border-radius: var(--radius-lg);
      background: var(--content-accent);
      color: var(--surface, #0b0d12);
      font-size: 10px;
      font-weight: 700;
      font-variant-numeric: tabular-nums;
      line-height: 16px;
    }
    /* #186 LIVE COMPASS: the cognition diamond's triangles glow + fade smoothly as the
     * radiator pushes new faculty levels (~2s). This CSS transition IS the "steady glow"
     * lane — a faculty firing brightens its triangle, then it eases back toward dark.
     * The per-event FLASH (lane B) rides on top of this. Honors reduced-motion. */
    .cog-tri {
      transition: opacity 0.7s ease, fill 0.7s ease;
    }
    @media (prefers-reduced-motion: reduce) {
      .cog-tri {
        transition: none;
      }
    }
`;
