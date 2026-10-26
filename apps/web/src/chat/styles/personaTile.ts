/** <chat-widget> styles, The persona tile: avatar ring, identity line, vitals, genome panel, compass. Moved verbatim from ChatWidget.ts; the
 *  order of `chatWidgetStyles` is the cascade order. */
import { css } from 'lit';

export const personaTileStyles = css`
    /* ================= PERSONA TILE =================
     * The legacy persona-tile (tag legacy-archive-2026-10-23: legacy/src/widgets/chat/user-list), ported onto
     * the positron member card: avatar-in-ring + status dots · name · green-mono
     * identity chips · labelled vital meters · the GENOME instrument panel.
     * Every colour is a named theme token; spacing/type/motion ride the shared
     * scales, so a universe skin restyles the WHOLE tile by overriding tokens. */

    .member {
      position: relative;
      /* A designed grid, not accidental flex alignment: avatar | info | right
         rail, with the recency stamp and genome panel stacked in the right rail
         (no absolute positioning, no dodge margins — the reference layout). */
      display: grid;
      grid-template-columns: auto 1fr auto;
      grid-template-areas:
        'av info ago'
        'av info gen';
      grid-template-rows: auto 1fr;
      column-gap: var(--spacing-sm);
      row-gap: 2px;
      align-items: center;
      padding: var(--spacing-xs) var(--spacing-sm);
      /* Chamfered HUD module (notched corners), not a rounded row. */
      clip-path: polygon(0 0, calc(100% - 7px) 0, 100% 7px, 100% 100%, 7px 100%, 0 calc(100% - 7px));
      transition: background var(--motion-fast) var(--motion-ease);
    }
    .member .avatar {
      grid-area: av;
    }
    .member .info {
      grid-area: info;
    }
    /* Dormant mind: vitals wired, every cognition pulse dark, no live stream —
       the row recedes slightly; any pulse/afterglow/stream restores full
       brightness (transition makes waking visible). Opacity only: compositor-
       cheap, theme-neutral (the universe layer can restyle later, #260). */
    .member.dormant {
      opacity: 0.62;
      transition: opacity 0.9s ease;
    }
    .member.idle {
      opacity: 0.6;
    }
    .member.clickable {
      cursor: pointer;
    }
    .member:hover,
    .member.clickable:focus-visible {
      background: linear-gradient(90deg, rgba(0, 212, 255, 0.09), transparent 70%);
      outline: none;
    }
    /* HUD corner brackets on hover/focus — the framed-module affordance. */
    .member.clickable::before,
    .member.clickable::after {
      content: '';
      position: absolute;
      width: 7px;
      height: 7px;
      opacity: 0;
      transition: opacity var(--motion-fast) var(--motion-ease);
      pointer-events: none;
    }
    .member.clickable::before {
      top: 2px;
      left: 2px;
      border-top: 1px solid var(--content-accent);
      border-left: 1px solid var(--content-accent);
    }
    .member.clickable::after {
      right: 2px;
      bottom: 2px;
      border-bottom: 1px solid var(--content-accent);
      border-right: 1px solid var(--content-accent);
    }
    .member.clickable:hover::before,
    .member.clickable:hover::after,
    .member.clickable:focus-visible::before,
    .member.clickable:focus-visible::after {
      opacity: 1;
    }

    /* --- Avatar: image in a live ring, presence dot at 4 o'clock ----------- */
    .member .avatar {
      position: relative;
      width: 42px;
      height: 42px;
      border-radius: 50%;
      display: grid;
      place-items: center;
      font-size: 20px;
      flex: none;
      background: var(--border-subtle);
      border: 2px solid var(--border-subtle);
    }
    .member[data-kind='agent'] .avatar {
      border-color: var(--widget-border);
      box-shadow: 0 0 6px var(--hud-accent-glow);
    }
    .avatar-img {
      position: absolute;
      inset: 0;
      width: 100%;
      height: 100%;
      border-radius: 50%;
      object-fit: cover;
      /* Legacy spec (PersonaTile.ts:308): cover + CENTER TOP — VRM portraits
       * carry the face at the top of the frame; a centered crop lands on the
       * dark collar. Scene-lit live renders (the bevy scene compositor) are the
       * follow-up that replaces the dark field entirely. */
      object-position: center top;
    }
    /* Presence dot — the legacy status-indicator, bottom-right on the ring. */
    .member .status-dot {
      position: absolute;
      bottom: -2px;
      right: -2px;
      width: 12px;
      height: 12px;
      border-radius: 50%;
      background: var(--status-offline);
      border: 2px solid var(--widget-surface-solid);
      z-index: 2;
      transition: background var(--motion-base) var(--motion-ease),
        box-shadow var(--motion-base) var(--motion-ease);
    }
    .member.online .status-dot {
      background: var(--status-online);
      box-shadow: 0 0 5px var(--status-online);
    }
    /* Emotional-event emoji, over the avatar. */
    .emoji-overlay {
      position: absolute;
      top: -4px;
      right: -5px;
      font-size: 14px;
      line-height: 1;
      filter: drop-shadow(0 0 3px rgba(0, 0, 0, 0.85));
      z-index: 3;
    }

    /* --- The live ring: breathing at idle, comet arc while the mind works --
     * A slow "alive" breath on any online agent; the orbiting comet arc fires
     * on REAL state only — speaking (live token rail) and thinking (radiator)
     * carry their own named ring hues; error flares the border. Transform/
     * opacity-composited; reduced-motion collapses to static colour. */
    @keyframes alive-pulse {
      0%,
      100% {
        box-shadow: 0 0 5px var(--hud-accent-glow);
      }
      50% {
        box-shadow: 0 0 14px var(--border-accent);
      }
    }
    @keyframes comet-orbit {
      from {
        transform: rotate(0deg);
      }
      to {
        transform: rotate(360deg);
      }
    }
    .member[data-kind='agent'].online .avatar {
      animation: alive-pulse 3s var(--motion-ease) infinite;
    }
    .member .avatar::before {
      content: '';
      position: absolute;
      inset: -5px;
      border-radius: 50%;
      border: 3px solid transparent;
      opacity: 0;
      transition: opacity var(--motion-base) var(--motion-ease);
      pointer-events: none;
      z-index: 1;
    }
    .member .avatar[data-state='speaking']::before,
    .member .avatar[data-state='thinking']::before {
      opacity: 1;
      border-top-color: var(--comet-color);
      border-right-color: var(--comet-color);
      animation: comet-orbit 3.5s linear infinite;
    }
    .member .avatar[data-state='speaking'] {
      --comet-color: var(--ring-speaking);
      border-color: var(--ring-speaking);
    }
    .member .avatar[data-state='thinking'] {
      --comet-color: var(--ring-thinking);
      border-color: var(--ring-thinking);
    }
    .member .avatar[data-state='error'] {
      border-color: var(--content-error);
      box-shadow: 0 0 0 1px var(--content-error), 0 0 8px var(--content-error);
    }
    .member .avatar[data-state='idle'] {
      opacity: 0.7;
    }

    /* --- Identity column ---------------------------------------------------- */
    .member .info {
      display: flex;
      flex-direction: column;
      gap: 1px;
      min-width: 0;
    }
    /* Identity line — kind/runtime chips and the model loadout share ONE row
       (they were two stacked rows; the tile's height budget goes to live data). */
    .member .idline {
      display: flex;
      align-items: baseline;
      gap: var(--spacing-sm);
      min-width: 0;
    }
    .member .idline .loadout {
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
    .member .name {
      font-size: 14px;
      font-weight: 600;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
    .member .meta {
      display: flex;
      align-items: baseline;
      gap: var(--spacing-sm);
    }
    /* Green-mono identity chips — the legacy tile-type-badge look: bare glowing
     * mono caps, no pill boxes (PERSONA  ALIBABA in the reference). */
    .member .kind-badge,
    .runtime {
      font-family: var(--font-mono);
      font-size: var(--font-size-xs);
      font-weight: 700;
      text-transform: uppercase;
      letter-spacing: 0.12em;
      color: var(--hud-accent-dim);
      text-shadow: 0 0 4px var(--hud-accent-glow);
    }
    /* Recency stamp — the reference's "55m ago", quiet in the top-right. */
    .member .ago {
      grid-area: ago;
      justify-self: end;
      align-self: start;
      font-family: var(--font-mono);
      font-size: var(--font-size-xs);
      letter-spacing: 0.04em;
      color: var(--content-secondary);
      opacity: 0.75;
      pointer-events: none;
    }
    /* LOADOUT strip — the model backing the persona (model · size · ctx). */
    .member .loadout {
      display: flex;
      align-items: baseline;
      gap: var(--spacing-xs);
      font-family: var(--font-mono);
      font-size: var(--font-size-xs);
      font-variant-numeric: tabular-nums;
      color: var(--content-secondary);
      white-space: nowrap;
      overflow: hidden;
      text-overflow: ellipsis;
    }
    .member .loadout-part:first-of-type {
      color: var(--content-accent);
      letter-spacing: 0.02em;
    }
    .member .loadout-sep {
      opacity: 0.4;
    }

    /* --- Vital meters: the legacy INT/NRG/QUE stack, live ------------------ */
    .meters {
      /* 2×2 grid — four live bars in two rows, half the vertical spend of the
         stacked column, no data dropped. */
      display: grid;
      grid-template-columns: repeat(2, minmax(0, 1fr));
      gap: 2px var(--spacing-sm);
      margin-top: 2px;
    }
    .meter {
      display: flex;
      align-items: center;
      gap: var(--spacing-xs);
    }
    .meter-label {
      font-family: var(--font-mono);
      font-size: var(--font-size-2xs);
      font-weight: 700;
      letter-spacing: 0.08em;
      color: var(--hud-accent-dim);
      text-shadow: 0 0 3px var(--hud-accent-glow);
      width: 22px;
      flex: none;
    }
    .meter-track {
      position: relative;
      height: 5px;
      flex: 1;
      max-width: 72px;
      border-radius: 2px;
      background: var(--hud-track-background);
      border: 1px solid var(--hud-track-border);
      overflow: hidden;
    }
    .meter-fill {
      display: block;
      height: 100%;
      border-radius: 1px;
      background: var(--meter-color, var(--content-accent));
      box-shadow: 0 0 4px var(--meter-color, var(--content-accent));
      transition: width var(--motion-base) var(--motion-ease);
      overflow: hidden;
      position: relative;
    }
    .meter-val {
      font-family: var(--font-mono);
      font-size: var(--font-size-2xs);
      color: var(--meter-color, var(--content-accent));
      width: 14px;
      text-align: right;
      flex: none;
      font-variant-numeric: tabular-nums;
    }
    /* One named hue per vital — set once on the row, read by fill + value. */
    .meter[data-key='activity'] {
      --meter-color: var(--meter-act);
    }
    .meter[data-key='queue'] {
      --meter-color: var(--meter-que);
    }
    .meter[data-key='speed'] {
      --meter-color: var(--meter-spd);
    }
    .meter[data-key='size'] {
      --meter-color: var(--meter-par);
    }
    /* Live glint across a working persona's bars — a moving gauge, game-feel. */
    @keyframes meter-shimmer {
      from {
        transform: translateX(-120%);
      }
      to {
        transform: translateX(320%);
      }
    }
    .member.online .meter-fill::after {
      content: '';
      position: absolute;
      inset: 0;
      width: 40%;
      background: linear-gradient(90deg, transparent, rgba(255, 255, 255, 0.55), transparent);
      animation: meter-shimmer 2.4s linear infinite;
    }

    /* --- GENOME panel: the legacy instrument block, faithful ---------------
     * Rotated caption · four FULL-HEIGHT gene slots (dark until a gene pages
     * in — visible empty equipment slots, never half-mast bars) · the
     * cognition compass at top-right. */
    .genome-panel {
      grid-area: gen;
      justify-self: end;
      align-self: start;
      display: grid;
      grid-template-columns: auto 1fr auto;
      column-gap: var(--spacing-xs);
      align-items: end;
      min-height: 46px;
      padding: var(--spacing-xs) 5px;
      background: var(--hud-panel-background);
      border: 1px solid var(--hud-accent-border);
      border-radius: var(--radius-md);
      box-shadow: 0 0 8px var(--hud-accent-glow);
      transition: border-color var(--motion-fast) var(--motion-ease);
    }
    .member:hover .genome-panel {
      border-color: var(--hud-accent);
    }
    .genome-label {
      font-family: var(--font-mono);
      font-size: var(--font-size-2xs);
      font-weight: 700;
      letter-spacing: 0.08em;
      color: var(--hud-accent);
      text-shadow: 0 0 4px var(--hud-accent-glow);
      writing-mode: vertical-rl;
      transform: rotate(180deg);
      line-height: 1;
      align-self: center;
    }
    .genome-slots {
      /* Two rows of four (8 slots) — the loadout grid, row-major so genes
         fill left-to-right, top-to-bottom as they page in. */
      display: grid;
      grid-template-columns: repeat(4, auto);
      gap: 2px;
      justify-content: center;
    }
    .genome-slot {
      width: 7px;
      height: 17px;
      border-radius: 1px;
      background: var(--hud-slot-background);
      border: 1px solid var(--hud-slot-border);
      transition: background var(--motion-base) var(--motion-ease),
        border-color var(--motion-base) var(--motion-ease),
        box-shadow var(--motion-base) var(--motion-ease);
    }
    /* Page-in moment: the slot IGNITES (bright flash → settle) — game-feel on a
     * real state change only (the class flips when the radiator reports the
     * gene). Opacity-composited. */
    @keyframes gene-ignite {
      0% {
        opacity: 0.2;
      }
      60% {
        opacity: 1;
      }
      100% {
        opacity: 0.92;
      }
    }
    .genome-slot.lit {
      background: var(--hud-accent);
      border-color: var(--hud-accent);
      box-shadow: 0 0 5px var(--hud-accent-glow);
      animation: gene-ignite var(--motion-slow) var(--motion-ease);
    }
    /* Cognition compass — four faculty triangles, top-right of the panel. */
    .cog-diamond {
      width: 26px;
      height: 26px;
      flex: none;
      align-self: start;
      margin: 1px 0 0 1px;
    }
    .cog-tri {
      filter: drop-shadow(0 0 1.5px rgba(255, 255, 255, 0.25));
    }
    /* Element navigation (card 95844639): a tile element that navigates SIGNALS
     * it — pointer + a lift on hover (the affordance IS the invitation). */
    .element-link {
      cursor: pointer;
      transition: filter var(--motion-base) var(--motion-ease),
        transform var(--motion-base) var(--motion-ease);
    }
    .element-link:hover {
      filter: brightness(1.35) drop-shadow(0 0 4px var(--hud-accent-glow));
      transform: translateY(-1px);
    }

    @media (prefers-reduced-motion: reduce) {
      .member[data-kind='agent'].online .avatar,
      .member .avatar[data-state='speaking']::before,
      .member .avatar[data-state='thinking']::before,
      .member.online .meter-fill::after,
      .genome-slot.lit {
        animation: none;
      }
    }

    .what {
      overflow-y: auto;
      /* The widget keeps the reader's row still across prepends and trims above
         the viewport by adjusting scrollTop itself; browser scroll anchoring would
         apply the same correction a second time (Chrome, Firefox; not Safari). */
      overflow-anchor: none;
      padding: var(--spacing-md) var(--spacing-lg);
      /* Grid items default to min-width:auto and won't shrink below their content,
         so a long message makes .what overflow its track and clip at narrow widths.
         min-width:0 lets it honor the track so the bubbles wrap within the viewport. */
      min-width: 0;
    }
    .empty {
      color: var(--content-secondary);
      padding: var(--spacing-xl) var(--spacing-xs);
      text-align: center;
    }
`;
