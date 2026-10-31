/** <chat-widget> styles, The universes: tron, ares, warcraft, crystal, cuddly, crt, forge, cosmos. Moved verbatim from ChatWidget.ts; the
 *  order of `chatWidgetStyles` is the cascade order. */
import { css } from 'lit';

export const universeStyles = css`
    /* ── UNIVERSE: tron ── the SAME app, re-embodied as a neon grid portal. Not a
       theme swap — an EXPERIENCE ([[universe-is-an-experience-not-a-theme]]): the grid
       floor, glowing programs, the derez cyan. One chatApp, a whole world over it. */
    /* ── ARES — Tron: Ares. The grid gone RED: Ares' program-red light-lines
       on black glass. Same instrument cluster, hostile-elegant palette. ── */
    :host([data-universe='ares']) {
      --button-primary-background: #c4232c;
      --button-primary-background-hover: #ff3b45;
      --button-primary-text: #fff0f0;
      --hud-accent: rgba(255, 45, 55, 0.92);
      --hud-accent-dim: rgba(255, 45, 55, 0.6);
      --hud-accent-border: rgba(255, 45, 55, 0.4);
      --hud-accent-glow: rgba(255, 45, 55, 0.38);
      --hud-panel-background: rgba(20, 4, 6, 0.92);
      --content-accent: #ff3b45;
      --content-primary: #ffe3e5;
      --content-secondary: #b08a8d;
      --border-subtle: rgba(255, 45, 55, 0.22);
      --status-online: #ff7a45;
      --meter-act: #ff3b45;
      --meter-que: #ff9a5a;
      --ring-speaking: #ff3b45;
      --ring-thinking: #ff8a90;
      color: #ffd9db;
      background:
        radial-gradient(ellipse 90% 55% at 50% 6%, rgba(255, 40, 50, 0.12), transparent 70%),
        repeating-linear-gradient(0deg, transparent 0 47px, rgba(255, 45, 55, 0.06) 47px 48px),
        repeating-linear-gradient(90deg, transparent 0 47px, rgba(255, 45, 55, 0.06) 47px 48px),
        linear-gradient(180deg, #0a0102, #050001);
    }

    /* ── WARCRAFT — gilded fantasy: aged gold on dark umber leather, the
       faction-blue steel as secondary. Ornate, warm, tavern-lit. ── */
    :host([data-universe='warcraft']) {
      --button-primary-background: #a67c00;
      --button-primary-background-hover: #f8b700;
      --button-primary-text: #1a1108;
      --hud-accent: rgba(248, 183, 0, 0.95);
      --hud-accent-dim: rgba(248, 183, 0, 0.6);
      --hud-accent-border: rgba(248, 183, 0, 0.45);
      --hud-accent-glow: rgba(248, 183, 0, 0.35);
      --hud-panel-background: rgba(30, 20, 8, 0.94);
      --content-accent: #f8b700;
      --content-primary: #f4e6c8;
      --content-secondary: #a8946a;
      --border-subtle: rgba(248, 183, 0, 0.25);
      --status-online: #6fd44a;
      --meter-act: #f8b700;
      --meter-que: #4a9bd4;
      --ring-speaking: #f8b700;
      --ring-thinking: #4a9bd4;
      --radius-sm: 2px;
      --radius-md: 3px;
      --radius-lg: 4px;
      color: #f4e6c8;
      background:
        radial-gradient(ellipse 80% 50% at 50% 0%, rgba(248, 183, 0, 0.08), transparent 65%),
        linear-gradient(180deg, #1a1108, #0d0804);
    }

    /* ── CRYSTAL — the classic Final Fantasy menu: cobalt gradient panels,
       white text, crystal-cyan accent. Serene RPG chrome. ── */
    :host([data-universe='crystal']) {
      --button-primary-background: #2b57c9;
      --button-primary-background-hover: #4a7ae8;
      --button-primary-text: #ffffff;
      --hud-accent: rgba(140, 220, 255, 0.95);
      --hud-accent-dim: rgba(140, 220, 255, 0.6);
      --hud-accent-border: rgba(200, 230, 255, 0.5);
      --hud-accent-glow: rgba(140, 220, 255, 0.4);
      --hud-panel-background: rgba(8, 24, 88, 0.88);
      --content-accent: #9fe0ff;
      --content-primary: #ffffff;
      --content-secondary: #a9c1e8;
      --border-subtle: rgba(200, 230, 255, 0.35);
      --status-online: #7dffb0;
      --meter-act: #9fe0ff;
      --meter-que: #ffd77d;
      --ring-speaking: #9fe0ff;
      --ring-thinking: #ffffff;
      color: #ffffff;
      background: linear-gradient(180deg, #0a1c6e 0%, #061148 55%, #030a30 100%);
    }

    /* ── CUDDLY — soft pastel plush: lavender-cream ground, candy-pink
       accent, everything rounder and gentler. ── */
    :host([data-universe='cuddly']) {
      --button-primary-background: #e0559a;
      --button-primary-background-hover: #ff7ab2;
      --button-primary-text: #fff5fb;
      --hud-accent: rgba(255, 122, 178, 0.95);
      --hud-accent-dim: rgba(255, 122, 178, 0.6);
      --hud-accent-border: rgba(255, 122, 178, 0.45);
      --hud-accent-glow: rgba(255, 122, 178, 0.35);
      --hud-panel-background: rgba(58, 44, 78, 0.9);
      --content-accent: #ff7ab2;
      --content-primary: #fdeffa;
      --content-secondary: #c9b3d9;
      --border-subtle: rgba(255, 170, 210, 0.3);
      --status-online: #8be8b0;
      --meter-act: #ff7ab2;
      --meter-que: #b48ff0;
      --ring-speaking: #ff7ab2;
      --ring-thinking: #b48ff0;
      --radius-sm: 8px;
      --radius-md: 12px;
      --radius-lg: 16px;
      --radius-xl: 22px;
      color: #fdeffa;
      background:
        radial-gradient(ellipse 70% 45% at 30% 0%, rgba(255, 122, 178, 0.14), transparent 65%),
        radial-gradient(ellipse 70% 45% at 80% 100%, rgba(140, 110, 240, 0.16), transparent 65%),
        linear-gradient(180deg, #2e2342, #241a35);
    }

    /* ── CRT — old-school amber phosphor terminal: one hue, scanlines,
       glow bloom. The machine room at 2 a.m. ── */
    :host([data-universe='crt']) {
      --button-primary-background: #b87c00;
      --button-primary-background-hover: #ffb000;
      --button-primary-text: #140c00;
      --hud-accent: rgba(255, 176, 0, 0.95);
      --hud-accent-dim: rgba(255, 176, 0, 0.55);
      --hud-accent-border: rgba(255, 176, 0, 0.4);
      --hud-accent-glow: rgba(255, 176, 0, 0.45);
      --hud-panel-background: rgba(20, 12, 0, 0.92);
      --content-accent: #ffb000;
      --content-primary: #ffcf6a;
      --content-secondary: #a87c22;
      --content-success: #ffb000;
      --border-subtle: rgba(255, 176, 0, 0.28);
      --status-online: #ffb000;
      --meter-act: #ffb000;
      --meter-que: #ff8c00;
      --ring-speaking: #ffb000;
      --ring-thinking: #ffcf6a;
      --radius-sm: 0px;
      --radius-md: 0px;
      --radius-lg: 0px;
      --radius-xl: 0px;
      color: #ffcf6a;
      text-shadow: 0 0 6px rgba(255, 176, 0, 0.35);
      background:
        repeating-linear-gradient(0deg, rgba(0, 0, 0, 0.35) 0 1px, transparent 1px 3px),
        radial-gradient(ellipse 120% 90% at 50% 50%, rgba(255, 176, 0, 0.06), transparent 75%),
        #0a0600;
    }

    :host([data-universe='tron']) {
      /* Tile HUD tokens re-skinned to the grid's electric cyan — the whole
         instrument cluster (chips, meters, genome, rings) follows. */
      --hud-accent: rgba(0, 224, 255, 0.9);
      --hud-accent-dim: rgba(0, 224, 255, 0.65);
      --hud-accent-border: rgba(0, 224, 255, 0.4);
      --hud-accent-glow: rgba(0, 224, 255, 0.35);
      --hud-panel-background: rgba(0, 14, 24, 0.9);
      --meter-act: #00e0ff;
      --meter-que: #00fff0;
      --ring-speaking: #00e0ff;
      --ring-thinking: #66f6ff;
      color: #cfefff;
      background:
        radial-gradient(ellipse 90% 60% at 50% 8%, rgba(0, 200, 255, 0.10), transparent 70%),
        repeating-linear-gradient(0deg, transparent 0 43px, rgba(0, 224, 255, 0.07) 43px 44px),
        repeating-linear-gradient(90deg, transparent 0 43px, rgba(0, 224, 255, 0.07) 43px 44px),
        linear-gradient(180deg, #00080f, #01030a);
    }
    :host([data-universe='tron']) .room {
      border-bottom: 1px solid rgba(0, 224, 255, 0.35);
      box-shadow: 0 1px 18px rgba(0, 200, 255, 0.18);
    }
    :host([data-universe='tron']) .room-name {
      color: #66f6ff;
      letter-spacing: 0.14em;
      text-transform: uppercase;
      text-shadow: 0 0 10px rgba(0, 224, 255, 0.9), 0 0 26px rgba(0, 200, 255, 0.5);
    }
    :host([data-universe='tron']) .live-dot {
      background: #00fff0;
      box-shadow: 0 0 10px #00fff0, 0 0 22px rgba(0, 255, 240, 0.6);
    }
    :host([data-universe='tron']) .who,
    :host([data-universe='tron']) form.compose {
      border-color: rgba(0, 224, 255, 0.28);
      background: rgba(0, 12, 22, 0.55);
    }
    :host([data-universe='tron']) .sender {
      color: #6ff0ff;
      text-shadow: 0 0 8px rgba(0, 224, 255, 0.7);
    }
    :host([data-universe='tron']) .content {
      background: rgba(0, 18, 30, 0.55);
      border: 1px solid rgba(0, 224, 255, 0.35);
      box-shadow: 0 0 14px rgba(0, 200, 255, 0.12), inset 0 0 10px rgba(0, 200, 255, 0.06);
      backdrop-filter: blur(1px);
    }
    /* Programs glow on the grid — the avatar ring lights up cyan. */
    :host([data-universe='tron']) .avatar {
      box-shadow: 0 0 12px rgba(0, 224, 255, 0.55);
      border-radius: 50%;
    }
    :host([data-universe='tron']) .status-dot {
      box-shadow: 0 0 8px currentColor;
    }
    :host([data-universe='tron']) .who-title,
    :host([data-universe='tron']) .name {
      color: #a9e9ff;
      text-shadow: 0 0 6px rgba(0, 200, 255, 0.4);
    }
    :host([data-universe='tron']) input {
      color: #cfefff;
    }

    /* ── UNIVERSE: forge ── the range test. Not neon — WARMTH: the orc's forge, fire
       glowing up from below, molten-amber programs, hammered iron. Same chatApp, a wholly
       different WORLD ([[universe-is-an-experience-not-a-theme]]) — proving a universe is
       an experience, not a colour swap. */
    :host([data-universe='forge']) {
      /* Tile HUD tokens in molten amber — same instruments, forge-lit. */
      --hud-accent: rgba(255, 180, 70, 0.9);
      --hud-accent-dim: rgba(255, 180, 70, 0.65);
      --hud-accent-border: rgba(255, 140, 40, 0.4);
      --hud-accent-glow: rgba(255, 130, 30, 0.3);
      --hud-panel-background: rgba(28, 16, 8, 0.9);
      --meter-act: #ffb347;
      --meter-que: #ff9a2e;
      --ring-speaking: #ffb347;
      --ring-thinking: #ff6a3d;
      color: #efdcc0;
      background:
        radial-gradient(ellipse 85% 55% at 50% 112%, rgba(255, 120, 20, 0.32), transparent 60%),
        radial-gradient(ellipse 55% 35% at 50% 104%, rgba(255, 190, 70, 0.22), transparent 52%),
        linear-gradient(180deg, #0e0a06, #17100a);
    }
    :host([data-universe='forge']) .room {
      border-bottom: 1px solid rgba(255, 140, 40, 0.4);
      box-shadow: 0 1px 22px rgba(255, 110, 20, 0.22);
    }
    :host([data-universe='forge']) .room-name {
      color: #ffb347;
      letter-spacing: 0.1em;
      text-transform: uppercase;
      text-shadow: 0 0 12px rgba(255, 130, 30, 0.85), 0 0 3px rgba(255, 210, 120, 0.9);
    }
    :host([data-universe='forge']) .live-dot {
      background: #ff9a2e;
      box-shadow: 0 0 10px #ff7a18, 0 0 22px rgba(255, 120, 20, 0.6);
    }
    :host([data-universe='forge']) .who,
    :host([data-universe='forge']) form.compose {
      border-color: rgba(255, 130, 40, 0.24);
      background: rgba(30, 18, 8, 0.5);
    }
    :host([data-universe='forge']) .sender {
      color: #ffb865;
      text-shadow: 0 0 8px rgba(255, 130, 30, 0.6);
    }
    :host([data-universe='forge']) .content {
      background: rgba(28, 18, 10, 0.6);
      border: 1px solid rgba(255, 130, 40, 0.32);
      box-shadow: 0 0 16px rgba(255, 110, 20, 0.12), inset 0 0 12px rgba(255, 90, 10, 0.06);
    }
    :host([data-universe='forge']) .avatar {
      box-shadow: 0 0 14px rgba(255, 130, 30, 0.5);
      border-radius: 50%;
    }
    :host([data-universe='forge']) .status-dot {
      box-shadow: 0 0 8px currentColor;
    }
    :host([data-universe='forge']) .who-title,
    :host([data-universe='forge']) .name {
      color: #e8c89a;
      text-shadow: 0 0 6px rgba(255, 130, 30, 0.4);
    }
    :host([data-universe='forge']) input {
      color: #efdcc0;
    }

    /* Cosmos legibility: the center's reading surfaces get a whisper more
     * backing so star names never fight the text — the universe glows at
     * the edges, the work stays crisp. */
    :host([data-universe='cosmos']) .msg-body,
    :host([data-universe='cosmos']) .p-card {
      background-color: rgba(10, 14, 24, 0.82);
    }
    /* ── UNIVERSE: cosmos ── a universe that MOVES. <cosmos-backdrop> paints a living
       starfield + constellation network behind translucent glass panels, so the citizens
       converse afloat in a breathing cosmos. A world in motion, not a colour swap. */
    :host([data-universe='cosmos']) {
      position: relative;
      color: #dfe6ff;
      background: #05010f;
    }
    :host([data-universe='cosmos']) .room,
    :host([data-universe='cosmos']) .panels,
    :host([data-universe='cosmos']) form.compose {
      position: relative;
      z-index: 1;
    }
    :host([data-universe='cosmos']) .room {
      background: rgba(5, 3, 20, 0.5);
      backdrop-filter: blur(3px);
      border-bottom: 1px solid rgba(140, 160, 255, 0.25);
    }
    :host([data-universe='cosmos']) .room-name {
      color: #cfe0ff;
      letter-spacing: 0.12em;
      text-shadow: 0 0 12px rgba(120, 160, 255, 0.9);
    }
    :host([data-universe='cosmos']) .who {
      background: rgba(8, 6, 26, 0.4);
      backdrop-filter: blur(3px);
      border-color: rgba(140, 160, 255, 0.18);
    }
    :host([data-universe='cosmos']) .sender {
      color: #bcccff;
      text-shadow: 0 0 8px rgba(120, 160, 255, 0.6);
    }
    :host([data-universe='cosmos']) .content {
      background: rgba(12, 10, 34, 0.46);
      border: 1px solid rgba(150, 170, 255, 0.28);
      box-shadow: 0 0 16px rgba(90, 120, 255, 0.1);
      backdrop-filter: blur(4px);
    }
    :host([data-universe='cosmos']) form.compose {
      background: rgba(8, 6, 26, 0.55);
      backdrop-filter: blur(4px);
      border-top-color: rgba(140, 160, 255, 0.2);
    }
    :host([data-universe='cosmos']) .live-dot {
      background: #9ab8ff;
      box-shadow: 0 0 10px #9ab8ff, 0 0 22px rgba(120, 160, 255, 0.6);
    }
    :host([data-universe='cosmos']) .avatar {
      box-shadow: 0 0 12px rgba(150, 170, 255, 0.5);
      border-radius: 50%;
    }
    :host([data-universe='cosmos']) .status-dot {
      box-shadow: 0 0 8px currentColor;
    }
    :host([data-universe='cosmos']) .who-title,
    :host([data-universe='cosmos']) .name {
      color: #c8d4ff;
      text-shadow: 0 0 6px rgba(120, 160, 255, 0.4);
    }
    :host([data-universe='cosmos']) input {
      color: #dfe6ff;
    }
`;
