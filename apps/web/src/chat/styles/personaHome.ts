/** <chat-widget> styles, The persona home and arena: hero, cards, brain HUD, pathways, genome shelf, claims. Moved verbatim from ChatWidget.ts; the
 *  order of `chatWidgetStyles` is the cascade order. */
import { css } from 'lit';

export const personaHomeStyles = css`
    /* ================= PERSONA HOME =================
     * The persona's home activity (purpose "persona") — profile hero + the
     * Cognitive System View, styled to the reference HUD (docs/images/
     * persona-profile.png + persona-brain-hud.png). Every colour is a named
     * token; region glow rides the live faculty pulse via --region-level. */
    /* ── ARENA face (purpose "arena") — leaderboards from the real ledger ── */
    .arena-home {
      display: flex;
      flex-direction: column;
      gap: 14px;
      padding: 16px 18px;
      overflow-y: auto;
    }
    .arena-head {
      display: flex;
      align-items: center;
      gap: 10px;
    }
    .arena-title {
      font-size: 15px;
      font-weight: 800;
      letter-spacing: 3px;
      color: var(--hud-accent);
    }
    .a-feed-chip {
      font-size: 9px;
      letter-spacing: 1px;
      text-transform: uppercase;
      padding: 2px 7px;
      border-radius: 8px;
      border: 1px solid var(--panel-border);
      color: var(--text-dim);
    }
    .a-feed-chip[data-on] {
      color: var(--status-online);
      border-color: var(--status-online);
    }
    .a-rowcount {
      margin-left: auto;
      font-size: 10px;
      color: var(--text-dim);
    }
    .arena-live-run {
      display: flex;
      align-items: center;
      gap: 8px;
      padding: 8px 12px;
      border: 1px solid var(--hud-accent);
      border-radius: 8px;
      background: var(--hud-panel-background);
      font-size: 11px;
    }
    .a-live-dot {
      width: 8px;
      height: 8px;
      border-radius: 50%;
      background: var(--status-online);
      box-shadow: 0 0 6px var(--status-online);
      animation: pulse-dot 1.2s ease-in-out infinite;
    }
    @keyframes pulse-dot {
      50% { opacity: 0.35; }
    }
    .a-live-label {
      font-weight: 800;
      letter-spacing: 2px;
      font-size: 9px;
      color: var(--status-online);
    }
    .a-live-what { font-weight: 600; }
    .a-live-progress {
      flex: 1;
      height: 5px;
      border-radius: 3px;
      background: var(--hud-slot-background);
      overflow: hidden;
    }
    .a-live-fill {
      display: block;
      height: 100%;
      background: var(--hud-accent);
      transition: width var(--motion-slow) var(--motion-ease);
    }
    .a-live-count { font-variant-numeric: tabular-nums; color: var(--text-dim); }
    .a-live-task { color: var(--text-dim); font-style: italic; }
    .arena-board {
      border: 1px solid var(--panel-border);
      border-radius: 10px;
      background: var(--hud-panel-background);
      overflow: hidden;
    }
    .a-board-head {
      display: flex;
      align-items: baseline;
      gap: 8px;
      padding: 8px 12px;
      border-bottom: 1px solid var(--panel-border);
    }
    .a-board-name {
      font-weight: 700;
      font-size: 12px;
      letter-spacing: 1px;
      color: var(--hud-accent);
    }
    .a-board-count {
      margin-left: auto;
      font-size: 10px;
      color: var(--text-dim);
    }
    .arena-table {
      width: 100%;
      border-collapse: collapse;
      font-size: 11px;
    }
    .arena-table th {
      text-align: left;
      font-size: 9px;
      letter-spacing: 1px;
      text-transform: uppercase;
      color: var(--text-dim);
      padding: 5px 10px;
      border-bottom: 1px solid var(--panel-border);
    }
    .arena-table td {
      padding: 6px 10px;
      border-bottom: 1px solid var(--panel-border);
      vertical-align: middle;
    }
    .arena-row:last-child td { border-bottom: none; }
    .arena-row[data-excluded] {
      opacity: 0.5;
    }
    .arena-row[data-excluded] .a-model-name,
    .arena-row[data-excluded] .a-score-num {
      text-decoration: line-through;
    }
    .a-rank {
      width: 26px;
      font-weight: 800;
      color: var(--hud-accent);
      font-variant-numeric: tabular-nums;
    }
    .a-model-name { font-weight: 600; }
    .a-arm {
      margin-left: 6px;
      font-size: 8px;
      letter-spacing: 1px;
      padding: 1px 5px;
      border-radius: 6px;
      border: 1px solid var(--panel-border);
      color: var(--text-dim);
      text-transform: uppercase;
    }
    .a-arm[data-arm='OURS'] {
      color: var(--hud-accent);
      border-color: var(--hud-accent);
    }
    .a-score { display: flex; align-items: center; gap: 8px; min-width: 180px; }
    .a-score-bar {
      flex: 1;
      height: 6px;
      border-radius: 3px;
      background: var(--hud-slot-background);
      overflow: hidden;
    }
    .a-score-fill {
      display: block;
      height: 100%;
      background: var(--hud-accent);
      box-shadow: 0 0 4px var(--hud-accent-glow);
    }
    .a-score-num { font-variant-numeric: tabular-nums; font-weight: 700; }
    .a-score-pct { font-variant-numeric: tabular-nums; color: var(--text-dim); font-size: 10px; }
    .a-prov, .a-date { color: var(--text-dim); font-size: 10px; white-space: nowrap; }
    .a-awaiting {
      padding: 22px;
      text-align: center;
      color: var(--text-dim);
      border: 1px dashed var(--panel-border);
      border-radius: 10px;
    }

    .persona-home {
      max-width: 880px;
      margin: 0 auto;
      display: flex;
      flex-direction: column;
      gap: var(--spacing-lg);
      padding-bottom: var(--spacing-xl);
    }
    .p-awaiting-banner {
      padding: var(--spacing-sm) var(--spacing-md);
      border: 1px dashed var(--border-subtle);
      border-radius: var(--radius-md);
      color: var(--content-secondary);
      font-size: 12px;
      font-style: italic;
    }
    /* --- hero ---------------------------------------------------------- */
    .p-hero {
      display: flex;
      align-items: center;
      gap: var(--spacing-xl);
      padding: var(--spacing-xl) var(--spacing-md) var(--spacing-md);
    }
    .p-avatar {
      position: relative;
      width: 148px;
      height: 148px;
      border-radius: 50%;
      flex: none;
      display: grid;
      place-items: center;
      font-size: 64px;
      background: radial-gradient(circle at 50% 35%, rgba(0, 212, 255, 0.12), transparent 70%),
        var(--hud-panel-background);
      border: 3px solid var(--hud-accent-border);
      box-shadow: 0 0 24px var(--hud-accent-glow), inset 0 0 18px rgba(0, 0, 0, 0.5);
    }
    .p-avatar[data-online] {
      animation: alive-pulse 3.2s var(--motion-ease) infinite;
    }
    .p-avatar-img {
      position: absolute;
      inset: 0;
      width: 100%;
      height: 100%;
      border-radius: 50%;
      object-fit: cover;
      object-position: center top;
    }
    .p-presence-dot {
      position: absolute;
      bottom: 8px;
      right: 8px;
      width: 18px;
      height: 18px;
      border-radius: 50%;
      background: var(--status-offline);
      border: 3px solid var(--widget-surface-solid);
      z-index: 2;
    }
    .p-avatar[data-online] .p-presence-dot {
      background: var(--status-online);
      box-shadow: 0 0 8px var(--status-online);
    }
    .p-id {
      min-width: 0;
      display: flex;
      flex-direction: column;
      gap: var(--spacing-sm);
    }
    .p-name {
      margin: 0;
      font-size: 30px;
      font-weight: 700;
      letter-spacing: 0.01em;
      color: var(--content-primary);
      text-shadow: 0 0 14px var(--hud-accent-glow);
    }
    .p-handle {
      display: flex;
      align-items: baseline;
      gap: var(--spacing-sm);
      color: var(--content-secondary);
      font-size: 13px;
    }
    .p-dot {
      opacity: 0.5;
    }
    .p-online[data-on] {
      color: var(--status-online);
    }
    .p-chips {
      display: flex;
      flex-wrap: wrap;
      gap: var(--spacing-sm);
      align-items: center;
    }
    .p-chip {
      padding: 2px 10px;
      border: 1px solid var(--border-subtle);
      border-radius: var(--radius-lg);
      font-family: var(--font-mono);
      font-size: 10px;
      font-weight: 700;
      letter-spacing: 0.08em;
      text-transform: uppercase;
      color: var(--content-secondary);
      background: var(--widget-surface, rgba(255, 255, 255, 0.03));
    }
    .p-chip-model {
      border-color: var(--border-accent, rgba(0, 212, 255, 0.4));
      color: var(--content-accent);
      text-transform: none;
    }
    .p-chip-kind {
      border-color: var(--hud-accent-border);
      color: var(--hud-accent);
      text-shadow: 0 0 4px var(--hud-accent-glow);
    }
    .p-actions {
      display: flex;
      gap: var(--spacing-sm);
      margin-top: var(--spacing-xs);
    }
    .p-btn {
      padding: 5px 14px;
      border: 1px solid var(--border-subtle);
      border-radius: var(--radius-md);
      background: var(--button-secondary-background);
      color: var(--content-secondary);
      font-size: 12px;
      font-weight: 600;
      cursor: pointer;
      line-height: 1.5;
    }
    .p-btn:hover:not([disabled]) {
      color: var(--content-accent);
      border-color: var(--border-accent, rgba(0, 212, 255, 0.4));
    }
    .p-btn[disabled] {
      opacity: 0.45;
      cursor: default;
    }
    /* --- cards ---------------------------------------------------------- */
    .p-card {
      border: 1px solid var(--border-subtle);
      border-radius: var(--radius-lg);
      background: var(--widget-surface, rgba(255, 255, 255, 0.02));
      padding: var(--spacing-md) var(--spacing-lg) var(--spacing-lg);
      /* Chamfered HUD module — the persona tile's corner language, scaled up. */
      clip-path: polygon(0 0, calc(100% - 10px) 0, 100% 10px, 100% 100%, 10px 100%, 0 calc(100% - 10px));
    }
    .p-card-head {
      display: flex;
      align-items: center;
      gap: var(--spacing-sm);
      text-transform: uppercase;
      letter-spacing: 0.08em;
      font-size: 11px;
      font-weight: 700;
      color: var(--content-secondary);
      padding-bottom: var(--spacing-sm);
    }
    .p-live-chip {
      padding: 1px 8px;
      border: 1px solid var(--border-subtle);
      border-radius: var(--radius-sm);
      font-family: var(--font-mono);
      font-size: 9px;
      letter-spacing: 0.08em;
      color: var(--content-secondary);
    }
    .p-live-chip[data-on] {
      color: var(--status-online);
      border-color: var(--status-online);
      box-shadow: 0 0 6px rgba(0, 255, 136, 0.35);
      animation: live-pulse 2.4s ease-in-out infinite;
    }
    .p-card-head .cog-diamond {
      margin-left: auto;
    }
    .p-empty {
      color: var(--content-secondary);
      font-size: 12px;
      font-style: italic;
      padding: var(--spacing-sm) 0;
    }
    /* WHAT SHE SEES NOW — her window as a gauge, one row per grounding source. */
    .sight-window {
      height: 6px;
      border: 1px solid var(--border-subtle);
      border-radius: var(--radius-sm);
      background: rgba(255, 255, 255, 0.04);
      overflow: hidden;
      margin-bottom: var(--spacing-sm);
    }
    .sight-window > span {
      display: block;
      height: 100%;
      background: var(--status-online);
      box-shadow: 0 0 8px rgba(0, 255, 136, 0.45);
      transition: width 600ms ease;
    }
    .sight-sources {
      display: grid;
      gap: 6px;
    }
    .sight-row {
      display: grid;
      grid-template-columns: 7rem 10rem minmax(0, 1fr);
      gap: var(--spacing-sm);
      align-items: baseline;
      font-size: 12px;
    }
    .sight-source {
      font-family: var(--font-mono);
      font-size: 10px;
      letter-spacing: 0.08em;
      text-transform: uppercase;
      color: var(--status-online);
    }
    .sight-tokens {
      font-family: var(--font-mono);
      font-size: 10px;
      color: var(--content-secondary);
      font-variant-numeric: tabular-nums;
    }
    .sight-preview {
      color: var(--content-primary);
      opacity: 0.85;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
    @media (max-width: 720px) {
      .sight-row { grid-template-columns: 1fr; gap: 2px; }
      .sight-preview { white-space: normal; }
    }
    .p-facts {
      display: flex;
      flex-wrap: wrap;
      gap: var(--spacing-xl);
    }
    .p-fact {
      display: flex;
      flex-direction: column;
      gap: 2px;
    }
    .p-fact-label {
      font-size: 9px;
      letter-spacing: 0.08em;
      text-transform: uppercase;
      color: var(--content-secondary);
    }
    .p-fact-val {
      font-family: var(--font-mono);
      font-size: 13px;
      color: var(--content-primary);
    }
    /* --- brain HUD ------------------------------------------------------ */
    /* LIVE LEARNING FEED — the engram stream on the mind tab. Rows are real
       admitted engrams (age · kind · digest); newest first, compositor-cheap. */
    .engram-stream {
      display: flex;
      flex-direction: column;
      gap: 6px;
      max-height: 340px;
      overflow-y: auto;
    }
    .engram-row {
      display: grid;
      grid-template-columns: 64px 74px 1fr;
      gap: 8px;
      align-items: baseline;
      padding: 6px 8px;
      border-left: 2px solid var(--content-accent);
      background: color-mix(in srgb, var(--content-accent) 6%, transparent);
      border-radius: 4px;
      font-size: 12px;
      animation: engram-land 0.5s ease-out;
    }
    .engram-row[data-kind='Semantic'] { border-left-color: #b48cff; }
    .engram-row[data-kind='Procedural'] { border-left-color: #3fb950; }
    .engram-age { color: var(--text-muted); font-variant-numeric: tabular-nums; white-space: nowrap; }
    .engram-kind { text-transform: uppercase; font-size: 10px; letter-spacing: 0.06em; color: var(--text-muted); }
    .engram-content {
      color: var(--text-primary);
      overflow: hidden;
      display: -webkit-box;
      -webkit-line-clamp: 2;
      -webkit-box-orient: vertical;
      white-space: pre-line;
    }
    @keyframes engram-land {
      from { opacity: 0; transform: translateY(-3px); }
      to { opacity: 1; transform: none; }
    }
    @media (prefers-reduced-motion: reduce) {
      .engram-row { animation: none; }
    }
    .p-brain {
      background:
        radial-gradient(ellipse 70% 60% at 50% 40%, rgba(0, 212, 255, 0.06), transparent 70%),
        var(--hud-panel-background);
      border-color: var(--hud-accent-border);
    }
    .brain-grid {
      display: grid;
      grid-template-columns: minmax(150px, 1fr) minmax(200px, 1.4fr) minmax(150px, 1fr);
      grid-template-areas: 'left center right' 'wide wide wide';
      gap: var(--spacing-md);
      align-items: center;
      padding: var(--spacing-sm) 0;
    }
    .brain-col {
      display: flex;
      flex-direction: column;
      gap: var(--spacing-md);
      min-width: 0;
    }
    .brain-col:first-of-type {
      grid-area: left;
    }
    .brain-center {
      grid-area: center;
      display: grid;
      place-items: center;
    }
    .brain-col:last-of-type {
      grid-area: right;
    }
    .brain-wide {
      grid-area: wide;
      display: grid;
      justify-content: center;
    }
    .brain-wide .region {
      min-width: 280px;
    }
    .brain-mark {
      width: 100%;
      max-width: 300px;
      filter: drop-shadow(0 0 18px var(--hud-accent-glow));
      color: var(--hud-accent);
    }
    /* Legacy brain composition (ported from persona-brain/templates/brain-svg):
       gradient cortex ellipse + dashed hemisphere + neural net + orbit rings —
       every stroke rides the hud tokens via currentColor. */
    .brain-grad-in {
      stop-color: var(--hud-accent);
      stop-opacity: 0.14;
    }
    .brain-grad-out {
      stop-color: var(--hud-accent);
      stop-opacity: 0.02;
    }
    .brain-cortex {
      stroke: currentColor;
      stroke-width: 2;
      opacity: 0.5;
    }
    .brain-hemis {
      stroke: currentColor;
      stroke-width: 1;
      opacity: 0.35;
    }
    .brain-fissure {
      stroke: currentColor;
      stroke-width: 0.8;
      opacity: 0.25;
    }
    .brain-net circle {
      fill: currentColor;
      opacity: 0.6;
    }
    .brain-net line {
      stroke: currentColor;
      stroke-width: 0.6;
      opacity: 0.3;
    }
    .brain-ring {
      stroke: currentColor;
      stroke-width: 1;
      opacity: 0.12;
    }
    .brain-ring-dash {
      stroke: currentColor;
      stroke-width: 1;
      opacity: 0.08;
    }
    /* Region card — the framed HUD module with corner brackets, glowing with
     * its live level (border + shadow intensity ride --region-level). */
    .region {
      position: relative;
      border: 1px solid var(--hud-track-border);
      background: rgba(0, 10, 18, 0.55);
      padding: var(--spacing-sm) var(--spacing-md);
      --region-glow: calc(var(--region-level, 0) / 100);
      transition: border-color var(--motion-base) var(--motion-ease),
        box-shadow var(--motion-base) var(--motion-ease);
    }
    .region[data-live] {
      border-color: color-mix(in srgb, var(--hud-accent) calc(25% + 60% * var(--region-glow)), transparent);
      box-shadow: 0 0 calc(4px + 14px * var(--region-glow)) var(--hud-accent-glow);
    }
    /* HUD corner brackets — always framed, brighter when live. */
    .region::before,
    .region::after {
      content: '';
      position: absolute;
      width: 9px;
      height: 9px;
      pointer-events: none;
      opacity: 0.6;
    }
    .region::before {
      top: -1px;
      left: -1px;
      border-top: 2px solid var(--hud-accent);
      border-left: 2px solid var(--hud-accent);
    }
    .region::after {
      right: -1px;
      bottom: -1px;
      border-bottom: 2px solid var(--hud-accent);
      border-right: 2px solid var(--hud-accent);
    }
    .region[data-live]::before,
    .region[data-live]::after {
      opacity: 1;
    }
    .region:not([data-live]) {
      border-style: dashed;
      opacity: 0.75;
    }
    .region-face {
      display: flex;
      flex-direction: column;
      gap: 3px;
      cursor: pointer;
      list-style: none;
      user-select: none;
      text-align: center;
    }
    .region-face::-webkit-details-marker {
      display: none;
    }
    .region-name {
      font-family: var(--font-mono);
      font-size: 13px;
      font-weight: 700;
      letter-spacing: 0.18em;
      text-transform: uppercase;
      color: var(--content-primary);
      text-shadow: 0 0 8px var(--hud-accent-glow);
    }
    .region-role {
      font-family: var(--font-mono);
      font-size: 9px;
      letter-spacing: 0.14em;
      text-transform: uppercase;
      color: var(--hud-accent-dim);
    }
    .region-track {
      height: 4px;
      margin: 4px auto 0;
      width: 80%;
      border-radius: 2px;
      background: var(--hud-track-background);
      border: 1px solid var(--hud-track-border);
      overflow: hidden;
    }
    .region-fill {
      display: block;
      height: 100%;
      background: var(--region-hue, var(--content-accent));
      box-shadow: 0 0 5px var(--region-hue, var(--content-accent));
      transition: width var(--motion-base) var(--motion-ease);
    }
    /* One named hue per faculty — the SAME tokens the cognition compass uses. */
    .region[data-faculty='reason'] {
      --region-hue: var(--faculty-reason);
    }
    .region[data-faculty='recall'] {
      --region-hue: var(--faculty-recall);
    }
    .region[data-faculty='act'] {
      --region-hue: var(--faculty-act);
    }
    .region[data-faculty='focus'] {
      --region-hue: var(--faculty-focus);
    }
    .region[data-faculty='activity'] {
      --region-hue: var(--content-accent);
    }
    .region-status {
      font-family: var(--font-mono);
      font-size: 10px;
      letter-spacing: 0.1em;
      text-transform: uppercase;
      color: var(--region-hue, var(--content-accent));
      margin-top: 2px;
    }
    .region-status.awaiting {
      color: var(--content-secondary);
      font-style: italic;
      text-transform: none;
      letter-spacing: 0.02em;
    }
    .region-detail {
      margin-top: var(--spacing-sm);
      padding-top: var(--spacing-sm);
      border-top: 1px dashed var(--hud-track-border);
      display: flex;
      flex-direction: column;
      gap: 4px;
    }
    .region-detail-row {
      display: flex;
      justify-content: space-between;
      gap: var(--spacing-md);
      font-size: 11px;
    }
    .rd-label {
      font-family: var(--font-mono);
      letter-spacing: 0.08em;
      text-transform: uppercase;
      font-size: 9px;
      color: var(--content-secondary);
      flex: none;
    }
    .rd-val {
      font-family: var(--font-mono);
      color: var(--content-primary);
      text-align: right;
      min-width: 0;
    }
    .region-actions {
      display: flex;
      gap: var(--spacing-sm);
      margin-top: var(--spacing-xs);
    }
    .brain-stats {
      display: flex;
      gap: var(--spacing-lg);
      justify-content: center;
      padding-top: var(--spacing-md);
      border-top: 1px solid var(--hud-track-border);
      margin-top: var(--spacing-sm);
    }
    .b-stat {
      font-family: var(--font-mono);
      font-size: 11px;
      color: var(--content-secondary);
      font-variant-numeric: tabular-nums;
    }
    /* --- pathways ------------------------------------------------------- */
    .pathway-grid {
      /* The reference's 3×2 tile grid — three across at the reading width. */
      display: grid;
      grid-template-columns: repeat(3, minmax(0, 1fr));
      gap: var(--spacing-md);
    }
    @media (max-width: 720px) {
      .pathway-grid {
        grid-template-columns: repeat(2, minmax(0, 1fr));
      }
    }
    .pathway {
      display: flex;
      flex-direction: column;
      align-items: center;
      gap: 3px;
      padding: var(--spacing-md) var(--spacing-sm);
      border: 1px solid var(--border-subtle);
      border-radius: var(--radius-md);
      background: rgba(0, 10, 18, 0.35);
      text-decoration: none;
      color: var(--content-primary);
      transition: border-color var(--motion-fast) var(--motion-ease),
        box-shadow var(--motion-fast) var(--motion-ease);
    }
    a.pathway:hover,
    a.pathway:focus-visible {
      border-color: var(--border-accent, rgba(0, 212, 255, 0.4));
      box-shadow: 0 0 10px var(--hud-accent-glow);
      outline: none;
    }
    .pathway[data-disabled] {
      opacity: 0.45;
      cursor: default;
    }
    .pathway-glyph {
      font-size: 20px;
    }
    .pathway-label {
      font-weight: 600;
      font-size: 13px;
    }
    .pathway-sub {
      font-family: var(--font-mono);
      font-size: 8.5px;
      letter-spacing: 0.1em;
      text-transform: uppercase;
      color: var(--content-secondary);
    }
    /* --- genome shelf --------------------------------------------------- */
    .gene-shelf {
      display: flex;
      flex-wrap: wrap;
      gap: var(--spacing-sm);
    }
    .gene-chip {
      display: inline-flex;
      align-items: center;
      gap: 6px;
      padding: 4px 12px;
      border: 1px solid var(--hud-accent-border);
      border-radius: var(--radius-lg);
      font-family: var(--font-mono);
      font-size: 11px;
      color: var(--hud-accent);
      background: var(--hud-panel-background);
      box-shadow: 0 0 6px var(--hud-accent-glow);
    }
    .gene-slot-dot {
      width: 6px;
      height: 6px;
      border-radius: 1px;
      background: var(--hud-accent);
      box-shadow: 0 0 4px var(--hud-accent-glow);
    }
    /* --- claims feed ---------------------------------------------------- */
    ul.claims {
      list-style: none;
      margin: 0;
      padding: 0;
      display: flex;
      flex-direction: column;
      gap: var(--spacing-xs);
    }
    .claim {
      display: flex;
      align-items: baseline;
      gap: var(--spacing-md);
      padding: var(--spacing-xs) var(--spacing-sm);
      border-left: 2px solid var(--border-subtle);
      font-size: 13px;
    }
    .claim[data-state='in_progress'] {
      border-left-color: var(--content-accent);
    }
    .claim[data-state='review'] {
      border-left-color: var(--meter-par);
    }
    .claim[data-state='merged'] {
      border-left-color: var(--status-online);
    }
    .claim[data-state='blocked'] {
      border-left-color: var(--content-error);
    }
    .claim-state {
      font-family: var(--font-mono);
      font-size: 9px;
      letter-spacing: 0.08em;
      text-transform: uppercase;
      color: var(--content-secondary);
      flex: none;
      width: 78px;
    }
    .claim-title {
      min-width: 0;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
      flex: 1;
    }
    .claim-meta {
      display: inline-flex;
      gap: var(--spacing-sm);
      font-family: var(--font-mono);
      font-size: 10px;
      color: var(--content-secondary);
      flex: none;
    }
    .claim-priority {
      color: var(--meter-par);
    }
    /* A lapsed LEASE is takeable, not busy (the 2026-08-06 distinction):
     * grey the row, badge the fact, keep the holder named as who to ask. */
    .claim[data-lapsed] .claim-title {
      opacity: 0.55;
    }
    .claim-lapsed {
      font-size: 9px;
      text-transform: uppercase;
      letter-spacing: 0.06em;
      border: 1px solid var(--border-color, #2a2a3a);
      border-radius: 3px;
      padding: 0 4px;
      opacity: 0.7;
    }
    .claim-pr {
      font-size: 10px;
      text-decoration: none;
      border: 1px solid var(--border-color, #2a2a3a);
      border-radius: 3px;
      padding: 0 4px;
    }
    @media (prefers-reduced-motion: reduce) {
      .p-avatar[data-online],
      .p-live-chip[data-on] {
        animation: none;
      }
    }

`;
