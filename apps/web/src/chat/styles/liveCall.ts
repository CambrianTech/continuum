/** <chat-widget> styles, The live call face: tiles, media, speaking state, controls, the mobile rule. Moved verbatim from ChatWidget.ts; the
 *  order of `chatWidgetStyles` is the cascade order. */
import { css } from 'lit';

export const liveCallStyles = css`
    /* ================= LIVE CALL FACE =================
     * The room's call grid (purpose "live") — the reference's Teams-style
     * avatar tiles (docs/images/live-session-avatars.png): per-participant
     * tiles, name tags bottom-left, presence dot top-left, SPEAKING green
     * border driven by the live token rail, caption strip, call-controls bar.
     * Every colour a named token; game-HUD polish on REAL state only. */
    .hdr-live[data-active] {
      color: var(--status-online, #3fb950);
      border-color: var(--status-online, #3fb950);
      box-shadow: 0 0 8px rgba(63, 185, 80, 0.4);
    }
    .live-room {
      display: flex;
      flex-direction: column;
      min-height: 100%;
      gap: var(--spacing-md);
    }
    .live-head {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: var(--spacing-md);
    }
    .live-title {
      display: inline-flex;
      align-items: center;
      gap: 7px;
      font-size: 13px;
      font-weight: 600;
      letter-spacing: 0.04em;
      color: var(--content-secondary);
    }
    .live-title-dot {
      width: 7px;
      height: 7px;
      border-radius: 50%;
      background: var(--status-online, #3fb950);
      box-shadow: 0 0 7px var(--status-online, #3fb950);
      animation: live-pulse 2.4s ease-in-out infinite;
    }
    /* Honest capability tag: tiles are avatars + live presence until the
       browser media plane carries real tracks. */
    .live-plane-chip {
      font-family: var(--font-mono);
      font-size: 8.5px;
      letter-spacing: 0.08em;
      text-transform: uppercase;
      padding: 2px 8px;
      border: 1px dashed var(--border-subtle);
      border-radius: var(--radius-sm);
      color: var(--content-secondary);
    }
    .live-empty {
      flex: 1;
      display: grid;
      place-items: center;
      color: var(--content-secondary);
      font-style: italic;
    }
    /* The staged media-permission card — reason before the native ask,
       honest recovery after a deny (POSITRON-MEDIA-PERMISSIONS.md). */
    .media-ask {
      position: absolute;
      top: 18%;
      left: 50%;
      transform: translateX(-50%);
      z-index: 5;
      max-width: 380px;
      padding: 16px 18px;
      border-radius: 10px;
      border: 1px solid var(--border-subtle);
      background: var(--widget-input-area-background);
      box-shadow: 0 12px 40px rgba(0, 0, 0, 0.5);
    }
    .media-ask-title {
      font-weight: 700;
      margin-bottom: 6px;
    }
    .media-ask-body {
      font-size: 13px;
      color: var(--text-muted);
      line-height: 1.5;
      margin-bottom: 12px;
    }
    .media-ask-actions {
      display: flex;
      gap: 8px;
    }
    .media-ask-continue {
      background: var(--content-accent);
      color: #04121a;
      border: none;
      border-radius: 6px;
      padding: 7px 14px;
      font-weight: 600;
      cursor: pointer;
    }
    .media-ask-dismiss {
      background: transparent;
      color: var(--text-muted);
      border: 1px solid var(--border-subtle);
      border-radius: 6px;
      padding: 7px 14px;
      cursor: pointer;
    }
    .live-grid {
      flex: 1;
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
      gap: var(--spacing-md);
      align-content: center;
    }
    /* PANEL composition — TikTok's host-focused layout: the ACTIVE SPEAKER
       takes the full bleed, everyone else shrinks to a right rail (the
       reference's panel-vs-grid canonical). Focus follows the REAL token
       rail — whoever is streaming speaks the stage. */
    .live-panel {
      flex: 1;
      display: grid;
      grid-template-columns: 1fr 132px;
      gap: var(--spacing-md);
      min-height: 0;
    }
    .live-stage {
      display: grid;
      min-height: 0;
    }
    .live-stage .live-tile {
      aspect-ratio: auto;
      height: 100%;
    }
    .live-stage .live-avatar {
      width: 132px;
      height: 132px;
      font-size: 64px;
    }
    .live-rail {
      display: flex;
      flex-direction: column;
      gap: var(--spacing-sm);
      overflow-y: auto;
      min-height: 0;
    }
    .live-rail .live-tile {
      aspect-ratio: 4 / 3;
      flex: none;
    }
    .live-rail .live-avatar {
      width: 44px;
      height: 44px;
      font-size: 22px;
    }
    .live-rail .live-name {
      font-size: 9px;
    }
    /* MOBILE full-bleed — the TikTok-native register (card 0dd1123c): the
       call face IS the screen; head + caption + controls float as overlays,
       the rail hugs the right edge, controls become a floating bottom bar. */
    @media (max-width: 720px) {
      .live-room {
        padding: 0;
        gap: 0;
        position: relative;
      }
      .live-head {
        position: absolute;
        top: 0;
        left: 0;
        right: 0;
        z-index: 3;
        padding: var(--spacing-md);
        background: linear-gradient(180deg, rgba(0, 0, 0, 0.55), transparent);
      }
      .live-panel {
        grid-template-columns: 1fr;
      }
      .live-panel .live-stage .live-tile,
      .live-grid[data-count='1'] .live-tile {
        border-radius: 0;
        border: none;
      }
      .live-rail {
        position: absolute;
        right: var(--spacing-sm);
        top: 64px;
        bottom: 120px;
        width: 84px;
        z-index: 2;
      }
      .live-caption {
        position: absolute;
        left: var(--spacing-md);
        right: 100px;
        bottom: 88px;
        z-index: 3;
        background: rgba(0, 0, 0, 0.45);
        border-radius: var(--radius-lg);
        padding: var(--spacing-sm) var(--spacing-md);
      }
      .live-controls {
        position: absolute;
        left: 0;
        right: 0;
        bottom: 0;
        z-index: 3;
        padding: var(--spacing-md);
        background: linear-gradient(0deg, rgba(0, 0, 0, 0.55), transparent);
      }
    }
    /* Wide rooms cap at 4 columns worth of tile width via the minmax above;
       small screens fall to 1–2 columns naturally. */
    /* LIVE MEDIA fills its tile — video and the WS-tee canvas alike. This
       block was MISSING entirely: the elements sat intrinsic-size in a
       centered grid, so streams rendered as clipped corners with the glyph
       floating on top (Joel's screenshot, 2026-08-31). */
    .lt-video {
      position: absolute;
      inset: 0;
      width: 100%;
      height: 100%;
      object-fit: cover;
    }
    /* A tile with live media hides its glyph/portrait fallbacks — the stream
       IS the face. */
    .live-tile:has(.lt-video) .lt-glyph,
    .live-tile:has(.lt-video) .live-avatar {
      display: none;
    }
    .live-tile {
      position: relative;
      aspect-ratio: 16 / 10;
      border-radius: var(--radius-lg);
      overflow: hidden;
      display: grid;
      place-items: center;
      background:
        radial-gradient(ellipse 80% 65% at 50% 30%, rgba(120, 150, 210, 0.28), transparent 75%),
        linear-gradient(180deg, rgba(70, 90, 140, 0.35), rgba(10, 14, 26, 0.9)),
        var(--hud-panel-background);
      border: 2px solid var(--border-subtle);
      transition: border-color var(--motion-base) var(--motion-ease),
        box-shadow var(--motion-base) var(--motion-ease),
        transform var(--motion-fast) var(--motion-ease);
    }
    .live-tile:hover {
      transform: translateY(-2px);
      border-color: var(--border-accent, rgba(0, 212, 255, 0.4));
      box-shadow: 0 4px 18px rgba(0, 0, 0, 0.45);
    }
    .live-tile:not([data-active]) {
      opacity: 0.55;
      filter: saturate(0.6);
    }
    /* SPEAKING — the reference's green active-speaker border, breathing while
       REAL tokens flow on the live rail (never a timer animation). */
    @keyframes live-speaking-pulse {
      0%, 100% {
        box-shadow: 0 0 10px rgba(63, 185, 80, 0.55), inset 0 0 14px rgba(63, 185, 80, 0.12);
      }
      50% {
        box-shadow: 0 0 24px rgba(63, 185, 80, 0.95), inset 0 0 22px rgba(63, 185, 80, 0.2);
      }
    }
    .live-tile[data-speaking] {
      border-color: var(--status-online, #3fb950);
      animation: live-speaking-pulse 1.6s ease-in-out infinite;
    }
    .lt-glyph {
      font-size: 52px;
      opacity: 0.9;
    }
    .lt-img {
      position: absolute;
      inset: 0;
      width: 100%;
      height: 100%;
      object-fit: cover;
      /* Legacy tile spec: VRM portraits carry the face at the top of the
         frame — a centered crop lands on the collar. */
      object-position: center top;
    }
    .lt-status {
      position: absolute;
      top: 8px;
      left: 8px;
      width: 10px;
      height: 10px;
      border-radius: 50%;
      background: var(--status-offline, #555);
      border: 2px solid rgba(0, 0, 0, 0.5);
      z-index: 2;
    }
    .lt-status[data-on] {
      background: var(--status-online, #3fb950);
      box-shadow: 0 0 6px var(--status-online, #3fb950);
    }
    .lt-name {
      position: absolute;
      left: 8px;
      bottom: 8px;
      display: inline-flex;
      align-items: center;
      gap: 5px;
      max-width: calc(100% - 16px);
      padding: 2px 9px;
      border-radius: var(--radius-sm);
      background: rgba(0, 0, 0, 0.62);
      color: #fff;
      font-size: 11.5px;
      font-weight: 600;
      letter-spacing: 0.02em;
      white-space: nowrap;
      overflow: hidden;
      text-overflow: ellipsis;
      z-index: 2;
    }
    .lt-wave {
      font-size: 10px;
    }
    /* The live transcript line — the active speaker's streaming turn. */
    .live-caption {
      align-self: center;
      max-width: min(760px, 92%);
      padding: var(--spacing-sm) var(--spacing-lg);
      border-radius: var(--radius-md);
      background: rgba(0, 0, 0, 0.68);
      border: 1px solid var(--border-subtle);
      color: #fff;
      font-size: 14px;
      line-height: 1.4;
      text-align: center;
    }
    .live-caption-name {
      color: var(--status-online, #3fb950);
      font-weight: 700;
      margin-right: 6px;
    }
    .live-caret {
      opacity: 0.8;
    }
    /* Call controls — the reference's bottom-center bar. Only real actions
       enabled: CC (caption strip) and hang-up; the rest honestly disabled. */
    .live-controls {
      align-self: center;
      display: flex;
      gap: var(--spacing-md);
      padding: var(--spacing-sm) 0 var(--spacing-md);
    }
    .live-btn {
      position: relative;
      width: 46px;
      height: 46px;
      border-radius: 50%;
      border: 1px solid var(--border-subtle);
      background: var(--button-secondary-background);
      color: var(--content-primary);
      font-size: 17px;
      display: grid;
      place-items: center;
      cursor: pointer;
      padding: 0;
      transition: border-color var(--motion-fast) var(--motion-ease),
        box-shadow var(--motion-fast) var(--motion-ease);
    }
    .live-btn:hover:not([disabled]) {
      border-color: var(--border-accent, rgba(0, 212, 255, 0.4));
      box-shadow: 0 0 10px var(--hud-accent-glow);
    }
    .live-btn[disabled] {
      opacity: 0.38;
      cursor: default;
    }
    .live-btn[data-on] {
      border-color: var(--content-accent);
      color: var(--content-accent);
      box-shadow: 0 0 8px var(--hud-accent-glow);
    }
    .live-btn[data-danger] {
      background: #b62324;
      border-color: #d33;
      color: #fff;
    }
    .live-btn[data-danger]:hover:not([disabled]) {
      border-color: #ff6b6b;
      box-shadow: 0 0 12px rgba(255, 60, 60, 0.55);
    }
    .live-btn-glyph {
      font-size: inherit;
      font-weight: 700;
      letter-spacing: 0.02em;
    }
    .live-btn-badge {
      position: absolute;
      top: -4px;
      right: -6px;
      min-width: 17px;
      padding: 0 4px;
      border-radius: var(--radius-lg);
      background: var(--content-accent);
      color: var(--surface, #0b0d12);
      font-size: 9.5px;
      font-weight: 700;
      line-height: 15px;
      font-variant-numeric: tabular-nums;
    }
    @media (prefers-reduced-motion: reduce) {
      .live-title-dot,
      .live-tile[data-speaking] {
        animation: none;
      }
      .live-tile:hover {
        transform: none;
      }
    }

    /* The MOBILE adaptation rule (@media modality:mobile via viewport) — LAST in the
       sheet so it wins by source order (media queries add no specificity). The desktop
       three-panel is wrong on a phone, so the presentation is DERIVED, not reflowed:
       the conversation (primary) takes the screen; the roster (secondary) collapses to a
       compact horizontal "who's here" avatar strip on top; secondary per-member detail
       (kind badge, runtime, vitals) is DROPPED — a phone shows who's present, not a full
       dossier per row. Best UX for THIS portal, not the desktop shrunk. */
    @media (max-width: 720px) {
      .panels {
        grid-template-columns: 1fr;
        grid-template-rows: auto 1fr;
      }
      /* No column drag on the single-column phone layout. */
      .col-handle {
        display: none;
      }
      .who {
        border-right: none;
        border-bottom: 1px solid var(--border-subtle);
        overflow-x: auto;
        overflow-y: hidden;
      }
      .who-head {
        padding-bottom: 2px;
      }
      /* Roster reflows to a horizontal, thumb-scrollable avatar strip. */
      .roster {
        display: flex;
        flex-direction: row;
        gap: var(--spacing-md);
        padding: 2px var(--spacing-md) var(--spacing-sm);
      }
      .member {
        flex-direction: column;
        align-items: center;
        gap: 3px;
        width: 60px;
        flex: none;
      }
      .member .info {
        align-items: center;
        width: 60px;
      }
      .member .name {
        font-size: 11px;
        max-width: 60px;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
      }
      /* Mobile drops the secondary per-member detail — presence, not a dossier. */
      .member .meta,
      .member .loadout,
      .member .meters,
      .member .ago,
      .genome-panel {
        display: none;
      }

      /* LIVE takes the phone (TikTok-native, card 0dd1123c): when the call face
         is the content, the who-strip and tab chrome yield — the face IS the
         screen; head/caption/controls float as overlays per the reference set. */
      .panels:has(.live-room) {
        grid-template-rows: 1fr;
      }
      .panels:has(.live-room) .who,
      .panels:has(.live-room) .tab-bar,
      :host([data-live-mobile]) .tabs {
        display: none;
      }
    }


`;
