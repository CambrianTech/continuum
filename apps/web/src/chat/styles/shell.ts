/** <chat-widget> styles, The shell: columns, tabs, header controls, rails, the system HUD and its gauges. Moved verbatim from ChatWidget.ts; the
 *  order of `chatWidgetStyles` is the cascade order. */
import { css } from 'lit';

export const shellStyles = css`
    /* Styled ENTIRELY from the shared design tokens (apps/web/src/theme.css) — no
     * hardcoded colors, so a theme swap is a :root override and the same token
     * names port to other surfaces. */
    :host {
      /* The shell is ONE full-height grid (.panels) — Discord geometry: the
         rails run window-top to window-bottom; tabs, header, transcript, and
         compose all live inside the CENTER column. */
      display: flex;
      flex-direction: column;
      height: 100%;
      font: 14px/1.45 var(--font-primary, system-ui, sans-serif);
      color: var(--content-primary, #e0e6ed);
      background: var(--widget-surface-solid, #1a1f2e);
    }
    header.room {
      display: flex;
      align-items: baseline;
      justify-content: space-between;
      padding: var(--spacing-md) var(--spacing-lg);
      border-bottom: 1px solid var(--border-subtle);
      background: var(--widget-input-area-background);
    }
    .room-name {
      font-weight: 600;
      font-size: 15px;
      color: var(--content-accent);
    }
    .room-meta {
      display: flex;
      align-items: center;
      gap: var(--spacing-sm);
      color: var(--content-secondary);
      font-size: 12px;
    }
    /* A live pulse beats a raw UUID dumped in the header (the id lives in the tooltip). */
    .live {
      display: inline-flex;
      align-items: center;
      gap: 5px;
      text-transform: uppercase;
      letter-spacing: 0.08em;
      font-size: 10px;
      color: var(--content-secondary);
    }
    .live-dot {
      width: 6px;
      height: 6px;
      border-radius: 50%;
      background: var(--status-online, #3fb950);
      box-shadow: 0 0 6px var(--status-online, #3fb950);
      animation: live-pulse 2.4s ease-in-out infinite;
    }
    @keyframes live-pulse {
      0%, 100% { opacity: 1; }
      50% { opacity: 0.35; }
    }
    .panels {
      display: grid;
      /* Column widths are the per-user WorkspaceLayout intent (host CSS vars,
         set by the widget's drag handling + persisted) over target defaults;
         the slim tracks between are the drag handles. Columns run FULL HEIGHT
         (Discord/VS Code): no chrome bar spans the whole window. */
      grid-template-columns: var(--who-w, 280px) 6px 1fr;
      min-height: 0;
      flex: 1;
    }
    /* A populated ContextPanel opens the third column — the right contextual
       rail (participants summary, room info; the factory reference's right panel). */
    .panels[data-context] {
      grid-template-columns: var(--who-w, 280px) 6px 1fr 6px var(--ctx-w, 220px);
    }
    /* The CENTER column — tabs on top (VS Code-central), then the room header,
       then the transcript taking the remainder, then the host-supplied compose
       footer. All center-scoped; the rails never share a row with any of it. */
    .center {
      display: flex;
      flex-direction: column;
      min-width: 0;
      min-height: 0;
    }
    .center > .what {
      flex: 1;
      min-height: 0;
    }
    /* Column drag handle — invisible until hover/drag, a full-height slim hit
       target (6px track, wider invisible hit area via padding-box trick is
       unnecessary at 6px on touch: iPadOS pointer coalescing hits it fine). */
    .col-handle {
      cursor: col-resize;
      touch-action: none;
      background: transparent;
      transition: background 0.15s ease;
    }
    .col-handle:hover,
    .col-handle:active {
      background: var(--content-accent);
      opacity: 0.35;
    }
    .col-handle:focus-visible {
      outline: 2px solid var(--content-accent);
      outline-offset: -2px;
    }
    .context {
      border-left: 1px solid var(--border-subtle);
      overflow-y: auto;
      padding: var(--spacing-sm) 0;
      background: var(--sidebar-background);
    }
    /* TAB BAR — the open activities as icon+title tabs (the reference's tabbed
       center). A tab pick IS the rooms-rail select (same composed event). */
    .tab-bar {
      display: flex;
      gap: 3px;
      padding: 4px var(--spacing-md) 0;
      border-bottom: 1px solid var(--border-subtle);
      background: var(--widget-input-area-background);
      overflow-x: auto;
    }
    .tab {
      display: inline-flex;
      align-items: center;
      gap: 5px;
      max-width: 200px;
      overflow: hidden;
      text-overflow: ellipsis;
      transition: color 0.12s ease, border-color 0.12s ease;
      padding: 4px 8px 5px 10px;
      border: 1px solid var(--border-subtle);
      border-bottom: none;
      border-radius: 7px 7px 0 0;
      font-size: 12px;
      color: var(--content-secondary);
      cursor: pointer;
      white-space: nowrap;
      user-select: none;
    }
    .tab[data-status='active'] {
      background: var(--widget-surface-solid, #1a1f2e);
      color: var(--content-primary);
      border-color: var(--border-accent, rgba(0, 212, 255, 0.4));
      box-shadow: 0 -1px 8px rgba(0, 212, 255, 0.12);
    }
    .tab:hover {
      color: var(--content-primary);
    }
    .tab:focus-visible {
      outline: 1px solid var(--content-accent);
      outline-offset: -1px;
    }
    .tab-icon {
      font-size: 11px;
    }
    .tab-close {
      border: none;
      background: transparent;
      color: var(--content-secondary);
      font-size: 14px;
      line-height: 1;
      cursor: pointer;
      /* A REAL hit target (Joel: "super small hitbox") — ~22px square via
         padding + negative margin so the visual glyph stays compact while the
         clickable area meets the finger/pointer minimum. */
      padding: 6px 7px;
      margin: -6px -5px -6px -3px;
      border-radius: var(--radius-sm);
    }
    .tab-close:hover {
      color: var(--content-primary);
      background: var(--button-secondary-background);
    }
    /* Top-right header controls: version badge + Theme (real universe cycle) +
       the not-yet-wired chrome rendered honestly disabled. */
    .header-controls {
      display: inline-flex;
      gap: 4px;
      align-items: center;
    }
    .header-version {
      align-self: center;
    }
    .hdr-btn {
      padding: 2px 9px;
      border: 1px solid var(--border-subtle);
      border-radius: var(--radius-sm);
      background: var(--button-secondary-background);
      color: var(--content-secondary);
      font-size: 10.5px;
      font-weight: 600;
      cursor: pointer;
      line-height: 1.6;
    }
    .hdr-btn:hover:not([disabled]) {
      color: var(--content-accent);
      border-color: var(--border-accent, rgba(0, 212, 255, 0.4));
    }
    .hdr-btn[disabled] {
      opacity: 0.45;
      cursor: default;
    }
    .who {
      border-right: 1px solid var(--border-subtle);
      overflow-y: auto;
      padding: var(--spacing-sm) 0;
      background: var(--sidebar-background);
    }
    ul {
      list-style: none;
      margin: 0;
      padding: 0;
    }
    /* WHO panel header — the old "Users & Agents (N)" label. */
    .who-head {
      display: flex;
      align-items: center;
      justify-content: space-between;
      padding: var(--spacing-sm) var(--spacing-md) var(--spacing-xs);
      text-transform: uppercase;
      letter-spacing: 0.06em;
      font-size: 11px;
      font-weight: 700;
      color: var(--content-secondary);
    }
    .who-count {
      min-width: 18px;
      padding: 0 5px;
      text-align: center;
      border-radius: var(--radius-lg);
      background: var(--button-secondary-background);
      color: var(--content-accent);
      font-size: 10px;
    }
    /* VISUAL STATE LANGUAGE (Joel: "be more visual"): every run card carries
     * a severity stripe — state readable from across the room, before any
     * text. Working breathes; terminal states sit still. */
    @media (prefers-reduced-motion: no-preference) {
      /* Compositor-only breathe: animate OPACITY of a painted-once overlay —
       * box-shadow keyframes repainted every card every frame (Joel:
       * "animation issues, speed issues"). */
    }
    /* HOME INTERIOR — the orthographic dollhouse card. Quiet inks; the lit
     * window and glowing desk items are the only saturated marks. */
    .p-home-hint { font-size: 10px; text-transform: none; letter-spacing: 0; color: var(--content-tertiary, #667); margin-left: 8px; }
    .home-iso { width: 100%; max-width: 360px; display: block; margin: 0 auto; }
    .hi-floor { fill: rgba(255,255,255,0.04); stroke: var(--border-subtle, #334); stroke-width: 0.6; }
    .hi-wall-l { fill: rgba(255,255,255,0.02); stroke: var(--border-subtle, #334); stroke-width: 0.6; }
    .hi-wall-r { fill: rgba(255,255,255,0.05); stroke: var(--border-subtle, #334); stroke-width: 0.6; }
    .hi-window { fill: rgba(255,255,255,0.03); stroke: var(--border-subtle, #334); stroke-width: 0.6; }
    .hi-window.hi-lit { fill: rgba(255, 214, 100, 0.35); stroke: rgba(255, 214, 100, 0.6); }
    .hi-desk { fill: rgba(255,255,255,0.07); stroke: var(--border-subtle, #334); stroke-width: 0.6; }
    .hi-run { fill: rgba(53, 208, 224, 0.5); stroke: var(--content-accent, #35d0e0); stroke-width: 0.5; }
    .hi-shelf { stroke: var(--border-subtle, #334); stroke-width: 1; }
    .hi-trophy { fill: #d4a017; }
    .hi-stem { stroke: #3fb950; stroke-width: 1; }
    .hi-leaf { fill: rgba(63, 185, 80, 0.55); }
    .hi-self { fill: var(--content-accent, #35d0e0); }
    .hi-legend { display: flex; gap: var(--spacing-md); justify-content: center; font-size: 10px; color: var(--content-tertiary, #667); margin-top: 4px; flex-wrap: wrap; }
    /* The form curve — a thin growth polyline in metrics rows. */
    .metrics-spark {
      width: 72px;
      height: 16px;
      align-self: center;
      margin-left: var(--spacing-sm);
    }
    .metrics-spark path {
      fill: none;
      stroke: var(--content-accent, #35d0e0);
      stroke-width: 1;
      stroke-linejoin: round;
    }
    .metrics-spark circle {
      fill: var(--content-accent, #35d0e0);
    }
    /* HOVER LIFE — interactive things answer the pointer (transform-only,
     * compositor-cheap; the fun half of "polish till it's fun"). */
    .cell[data-selectable] {
      transition: transform 0.12s ease;
    }
    .element-link { cursor: pointer; }
    .element-link:hover { text-decoration: underline; text-underline-offset: 2px; }
    /* The native home frame — a bevy-rendered image, displayed, never
     * rasterized here. */
    .p-home-frame {
      display: block;
      max-width: 360px;
      width: 100%;
      margin: 0 auto;
      border-radius: 6px;
      border: 1px solid var(--border-subtle);
    }
    /* Profile tab bar — one job per tab (PAGES-IA.md). */
    .p-tabs {
      display: flex;
      gap: var(--spacing-xs);
      margin: var(--spacing-md) 0 var(--spacing-sm);
      border-bottom: 1px solid var(--border-subtle);
      padding-bottom: var(--spacing-xs);
    }
    .p-tab {
      background: none;
      border: 1px solid transparent;
      border-radius: var(--radius-sm, 4px);
      color: var(--content-secondary);
      font-size: 12px;
      padding: 4px 12px;
      cursor: pointer;
      letter-spacing: 0.02em;
    }
    .p-tab[data-active] {
      color: var(--content-accent);
      border-color: var(--border-subtle);
      background: var(--button-secondary-background, rgba(255,255,255,0.04));
    }
    .p-idnums {
      display: flex;
      gap: var(--spacing-sm);
      margin: 4px 0;
    }
    .p-idnum {
      font-size: 11px;
      padding: 1px 8px;
      border-radius: 999px;
      border: 1px solid var(--border-subtle);
      color: var(--content-secondary);
    }
    /* Record & awards — trophy chips: identity earned from verdicts. */
    .p-trophies {
      display: flex;
      gap: var(--spacing-sm);
      flex-wrap: wrap;
      margin-bottom: var(--spacing-sm);
    }
    .p-trophy {
      font-size: 11px;
      padding: 2px 8px;
      border-radius: 999px;
      border: 1px solid var(--border-subtle);
      color: var(--content-secondary);
    }
    .p-trophy-wins {
      border-color: #3fb950;
      color: #3fb950;
    }
    /* MICRO-SPEEDOMETERS — the tile speedline: two thin arc gauges (decode
     * t/s, prefill t/s), needle position IS the reading. Thin lines, ~22px
     * each, no extra tile height. */
    .speedline {
      display: inline-flex;
      gap: var(--spacing-sm);
      align-items: flex-end;
      margin-right: var(--spacing-xs);
    }
    .speedo {
      display: inline-flex;
      flex-direction: column;
      align-items: center;
      line-height: 1;
    }
    .speedo-svg { width: 22px; height: 12px; display: block; }
    .speedo-arc {
      fill: none;
      stroke: var(--border-subtle, #334);
      stroke-width: 1.5;
      stroke-linecap: round;
    }
    .speedo-needle {
      stroke: var(--content-accent, #35d0e0);
      stroke-width: 1;
      transition: transform 0.6s cubic-bezier(0.22, 1, 0.36, 1);
      transform-origin: 11px 11px;
    }
    .speedo[data-key='pfx'] .speedo-needle { stroke: #b48cff; }
    .speedo-label {
      font-size: 7px;
      letter-spacing: 0.05em;
      color: var(--content-tertiary, #667);
      margin-top: 1px;
    }
    /* PAINT BUDGET (Joel: "Lit animations are already overdrawing"):
     * off-screen cells cost zero paint/layout — the browser skips rendering
     * until they scroll into view. Transcript rows are windowed instead
     * (transcriptWindow.ts): off-screen rows are not in the DOM at all, and
     * the rows that are must report their real height to be measured. */
    .cells > li {
      content-visibility: auto;
      contain-intrinsic-size: auto 72px;
    }
    .project-workspace { padding: var(--spacing-md); min-width: 0; }
    .project-heading { display: flex; align-items: baseline; gap: 1rem; flex-wrap: wrap; }
    .project-heading h2 { margin: 0; }
    .project-cards { list-style: none; padding: 0; display: grid; grid-template-columns: repeat(auto-fit, minmax(min(100%, 17rem), 1fr)); gap: 0.75rem; }
    .project-card { border: 1px solid var(--border-color, #384350); border-radius: 0.6rem; padding: 1rem; overflow-wrap: anywhere; }
    .project-card h3 { font-size: 1rem; margin: 0.6rem 0; }
    .project-card-meta { display: flex; justify-content: space-between; gap: 0.5rem; font-size: 0.8rem; }
    .project-description { white-space: pre-wrap; }
    /* THE WORKING WAVE (Joel, 2026-08-30: "active tasks wave as they are
     * active, as a gradient cyclic"): anything mid-work carries a slow cyclic
     * gradient sweep across its label — alive reads as MOVING, done reads as
     * still. Text-clip sweep, one shared class, honest to reduced-motion. */
    @media (prefers-reduced-motion: no-preference) {
      .wave-active {
        background: linear-gradient(
          90deg,
          var(--content-secondary, #8aa) 20%,
          var(--content-accent, #35d0e0) 50%,
          var(--content-secondary, #8aa) 80%
        );
        background-size: 200% 100%;
        -webkit-background-clip: text;
        background-clip: text;
        color: transparent;
        animation: wave-sweep 2.6s linear infinite;
      }
      @keyframes wave-sweep {
        from { background-position: 200% 0; }
        to { background-position: -200% 0; }
      }
    }
    /* The Activities panel never buries Users & Agents: the room list scrolls
     * inside its own bounds past ~6 rows (Joel: "it's also pushed the user
     * list all the way down"). Ephemeral work rooms live in one quiet
     * disclosure, closed by default. */
    .rooms-cells {
      max-height: 240px;
      overflow-y: auto;
    }
    .rooms-work > summary {
      cursor: pointer;
      font-size: 11px;
      text-transform: uppercase;
      letter-spacing: 0.05em;
      color: var(--content-tertiary, #667);
      padding: var(--spacing-xs) var(--spacing-md);
    }
    /* Rail tree (#2632): a child activity nests under its parent room with a
     * quiet indent + branch tick — activity == room == tab, now with lineage. */
    .cell[data-nested] {
      padding-left: 26px;
      position: relative;
    }
    .cell[data-nested]::before {
      content: '└';
      position: absolute;
      left: 12px;
      color: var(--content-tertiary, #556);
      font-size: 10px;
    }
    /* One stacked global widget in the left rail (Metrics · Rooms · Users & Agents).
     * The rail is a vertical stack; a hairline separates each widget module. Draggable
     * heights + reorder land in task #185 (this is the static stack it resizes). */
    .rail-widget {
      display: block;
      border-bottom: 1px solid var(--border-subtle);
    }
    /* The system HUD keeps ONE height across its faces — a rail that reflows
     * every time a face changes reads as broken chrome (Joel, 2026-08-30:
     * "dynamically change heights all the time"). Reserve the tallest face. */
    .rail-widget[data-widget='system'] {
      min-height: 128px;
    }
    .rail-widget:last-child {
      border-bottom: none;
    }
    /* Continuon header — the sidebar's identity mark: breathing orb + wordmark +
     * version chip + the tiny live-activity ticker (the old header's scrolling log). */
    .continuon {
      display: flex;
      align-items: center;
      gap: var(--spacing-sm);
      padding: var(--spacing-sm) var(--spacing-md);
      min-height: 44px;
    }
    .continuon-orb {
      width: 14px;
      height: 14px;
      border-radius: 50%;
      flex: none;
      background: radial-gradient(circle at 35% 35%, #7df2c8, #0f7a54 65%, #063325);
      box-shadow: 0 0 8px rgba(63, 185, 80, 0.55), inset 0 0 3px rgba(255, 255, 255, 0.35);
    }
    @keyframes continuon-breathe {
      0%, 100% { box-shadow: 0 0 5px rgba(63, 185, 80, 0.35), inset 0 0 3px rgba(255, 255, 255, 0.3); }
      50% { box-shadow: 0 0 13px rgba(63, 185, 80, 0.85), inset 0 0 4px rgba(255, 255, 255, 0.5); }
    }
    .continuon-orb[data-alive='yes'] {
      animation: continuon-breathe 3.2s ease-in-out infinite;
    }
    .continuon-orb[data-alive='no'] {
      filter: grayscale(0.8);
      opacity: 0.6;
    }
    /* CONNECTION STATE ON THE ORB (with the favicon, the designed status
       channel — never a text banner). Feed state outranks the alive breath:
       amber = connecting/reconnecting/cached (retrying quietly), red = closed. */
    .continuon-orb[data-feed='connecting'],
    .continuon-orb[data-feed='cached'],
    .continuon-orb[data-feed='reconnecting'] {
      background: radial-gradient(circle at 35% 35%, #ffd98a, #a8741a 65%, #4a3208);
      box-shadow: 0 0 8px rgba(216, 165, 63, 0.6), inset 0 0 3px rgba(255, 255, 255, 0.35);
      animation: continuon-retry 1.1s ease-in-out infinite;
    }
    .continuon-orb[data-feed='closed'] {
      background: radial-gradient(circle at 35% 35%, #ff9a8a, #a83226 65%, #4a0f08);
      box-shadow: 0 0 8px rgba(229, 83, 75, 0.6), inset 0 0 3px rgba(255, 255, 255, 0.35);
      animation: none;
    }
    @keyframes continuon-retry {
      0%, 100% { opacity: 1; }
      50% { opacity: 0.45; }
    }
    @media (prefers-reduced-motion: reduce) {
      .continuon-orb[data-alive='yes'] {
        animation: none;
      }
    }
    .continuon-id {
      min-width: 0;
      flex: none;
    }
    .continuon-row {
      display: flex;
      align-items: baseline;
      gap: var(--spacing-sm);
    }
    .continuon-wordmark {
      font-size: 16px;
      font-weight: 700;
      letter-spacing: 0.02em;
      color: var(--content-accent);
      text-shadow: 0 0 8px rgba(0, 212, 255, 0.35);
    }
    .continuon-version {
      font-family: var(--font-mono);
      font-size: 8px;
      padding: 1px 5px;
      border: 1px solid var(--border-accent, rgba(0, 212, 255, 0.4));
      border-radius: var(--radius-sm);
      color: var(--content-accent);
      background: rgba(0, 212, 255, 0.08);
      font-variant-numeric: tabular-nums;
    }
    .continuon-tagline {
      font-size: 8.5px;
      letter-spacing: 0.06em;
      color: var(--content-secondary);
      white-space: nowrap;
    }
    /* The live log ticker — right-aligned column of the last turns, log-tail style. */
    .continuon-ticker {
      flex: 1;
      min-width: 0;
      align-self: stretch;
      display: flex;
      flex-direction: column;
      justify-content: center;
      gap: 1px;
      overflow: hidden;
      border-left: 1px solid var(--border-subtle);
      padding-left: var(--spacing-sm);
    }
    .continuon-tick {
      font-family: var(--font-mono);
      font-size: 7.5px;
      line-height: 1.35;
      color: var(--content-secondary);
      white-space: nowrap;
      overflow: hidden;
      text-overflow: ellipsis;
    }
    .continuon-tick:last-child {
      color: var(--content-accent);
      opacity: 0.85;
    }
    /* AI Performance widget — the live team-cognition stat row (HERE / THINKING /
     * GENOME), the honest roster-derived slice of the old AI PERFORMANCE panel. */
    .metrics-row {
      display: flex;
      gap: var(--spacing-xs);
      padding: 2px var(--spacing-md) var(--spacing-sm);
    }
    .metric {
      display: flex;
      flex-direction: column;
      align-items: flex-start;
      flex: 1;
      padding: var(--spacing-xs) var(--spacing-sm);
      border-radius: var(--radius-sm);
      background: var(--widget-surface, rgba(255, 255, 255, 0.03));
    }
    .metric-val {
      font-size: 18px;
      font-weight: 700;
      font-variant-numeric: tabular-nums;
      line-height: 1.1;
      color: var(--content-primary);
    }
    .metric-label {
      font-size: 9px;
      letter-spacing: 0.08em;
      text-transform: uppercase;
      color: var(--content-secondary);
    }
    .metric[data-tone='good'] .metric-val {
      color: var(--status-online, #3fb950);
    }
    .metric[data-tone='accent'] .metric-val {
      color: var(--content-accent);
    }
    .metric[data-tone='muted'] .metric-val {
      color: var(--content-secondary);
    }
    /* SYS|AI face chips + honest window chip on the <sys-panel> header. */
    sys-panel {
      display: block;
    }
    .face-chips {
      display: inline-flex;
      gap: 2px;
      margin-left: auto;
      margin-right: var(--spacing-sm);
    }
    .face-chip {
      padding: 1px 7px;
      border: 1px solid var(--border-subtle);
      border-radius: var(--radius-sm);
      background: transparent;
      color: var(--content-secondary);
      font-size: 8.5px;
      font-weight: 700;
      letter-spacing: 0.06em;
      cursor: pointer;
      line-height: 1.5;
    }
    .face-chip:hover:not([disabled]) {
      color: var(--content-primary);
      border-color: var(--border-accent, rgba(0, 212, 255, 0.4));
    }
    .face-chip[data-active] {
      background: var(--content-accent);
      border-color: var(--content-accent);
      color: var(--surface, #0b0d12);
    }
    .face-chip[disabled] {
      opacity: 0.4;
      cursor: default;
    }
    .gauge-window {
      /* Keep the derived unit honest: "3m" is minutes; uppercase would read as months. */
      text-transform: none;
      font-family: var(--font-mono);
      font-size: 8.5px;
      padding: 1px 5px;
      border: 1px solid var(--border-subtle);
      border-radius: var(--radius-sm);
      color: var(--content-secondary);
      font-variant-numeric: tabular-nums;
    }
    /* SYS gauge (brick 2) — the multi-series resource sparkline + legend. */
    .gauge {
      padding: 2px var(--spacing-md) var(--spacing-sm);
    }
    .gauge svg {
      display: block;
      width: 100%;
      height: 56px;
      border-radius: var(--radius-sm);
      background: var(--widget-surface, rgba(255, 255, 255, 0.03));
    }
    .gauge-grid {
      stroke: var(--border-subtle);
      stroke-width: 0.5;
    }
    .gauge-legend {
      display: flex;
      gap: var(--spacing-md);
      padding-top: var(--spacing-xs);
    }
    .gauge-key {
      display: flex;
      align-items: center;
      gap: 4px;
      font-size: 10px;
    }
    .gauge-dot {
      width: 6px;
      height: 6px;
      border-radius: 50%;
    }
    .gauge-label {
      letter-spacing: 0.08em;
      color: var(--content-secondary);
    }
    .gauge-val {
      font-variant-numeric: tabular-nums;
      font-weight: 700;
      color: var(--content-primary);
    }
    /* SERVING glass box (#141 slice 1) — header line, bandit arm chips with
     * reward bars, and the pager's event cards. Sparklines reuse .gauge. */
    .serving-line {
      display: flex;
      align-items: baseline;
      gap: var(--spacing-sm);
      padding: 0 var(--spacing-md) var(--spacing-xs);
      font-size: 11px;
      min-width: 0;
    }
    .serving-model {
      font-family: var(--font-mono);
      font-weight: 700;
      color: var(--content-primary);
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
      min-width: 0;
    }
    .serving-meta {
      flex-shrink: 0;
      font-size: 9.5px;
      letter-spacing: 0.05em;
      color: var(--content-secondary);
      font-variant-numeric: tabular-nums;
    }
    .serving-degraded {
      color: var(--status-warning, #e0a458);
    }
    .serving-arms {
      display: flex;
      gap: 4px;
      padding: var(--spacing-xs) var(--spacing-md) 0;
    }
    .serving-arm {
      position: relative;
      flex: 1;
      min-width: 0;
      padding: 2px 0 4px;
      text-align: center;
      border: 1px solid var(--border-subtle);
      border-radius: var(--radius-sm);
      overflow: hidden;
    }
    .serving-arm[data-chosen] {
      border-color: var(--accent-primary);
    }
    .serving-arm[data-chosen] .arm-label {
      color: var(--accent-primary);
      font-weight: 700;
    }
    .arm-label {
      font-family: var(--font-mono);
      font-size: 8.5px;
      color: var(--content-secondary);
      font-variant-numeric: tabular-nums;
    }
    .arm-bar {
      position: absolute;
      left: 0;
      bottom: 0;
      height: 2px;
      background: var(--accent-primary);
      opacity: 0.85;
      border-radius: 1px;
    }
    ul.serving-events {
      list-style: none;
      margin: 0;
      padding: var(--spacing-xs) var(--spacing-md) var(--spacing-sm);
      display: flex;
      flex-direction: column;
      gap: 3px;
    }
    .serving-event {
      display: flex;
      align-items: baseline;
      gap: 6px;
      font-size: 9.5px;
      padding: 2px 6px;
      border-left: 2px solid var(--border-subtle);
      background: var(--widget-surface, rgba(255, 255, 255, 0.03));
      border-radius: 0 var(--radius-sm) var(--radius-sm) 0;
      color: var(--content-secondary);
    }
    .serving-event[data-kind='decay-switch'] {
      border-left-color: var(--accent-primary);
    }
    .serving-event[data-kind='serve-start'] {
      border-left-color: var(--status-success, #4caf7d);
    }
    .event-token {
      flex-shrink: 0;
      font-family: var(--font-mono);
      font-size: 8.5px;
      color: var(--content-tertiary, var(--content-secondary));
      font-variant-numeric: tabular-nums;
    }
    .event-detail {
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
    /* HUD cycle/pin toggle — the far-left corner control of the one graph
     * control: ⟳ while auto-cycling faces, ◉ when pinned. */
    .hud-toggle {
      appearance: none;
      border: 1px solid var(--border-subtle);
      background: transparent;
      color: var(--content-secondary);
      border-radius: 50%;
      width: 16px;
      height: 16px;
      font-size: 9px;
      line-height: 1;
      padding: 0;
      cursor: pointer;
      flex-shrink: 0;
      display: inline-flex;
      align-items: center;
      justify-content: center;
    }
    .hud-toggle[data-cycling] {
      color: var(--accent-primary);
      border-color: var(--accent-primary);
    }
    .hud-toggle:hover {
      color: var(--content-primary);
    }
`;
