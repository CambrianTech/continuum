/**
 * `renderLive` — the web renderer for a room's LIVE call face (`purpose="live"`).
 *
 * The Teams/Discord-style avatar grid from the reference design
 * (docs/images/live-session-avatars.png), drawn as a purpose-registered
 * `Content` renderer — the SAME registry dispatch as chat/foundry/persona,
 * reached by the SAME nav semantics. Pure fragments: everything here is a
 * field→element map of the already-projected `LiveContentBody`.
 *
 * Honesty rules ([[fallbacks-are-illegal-fail-loud]]):
 *   - a tile is the citizen's REAL stored avatar (or the kind glyph) — real
 *     video tracks are the documented follow-up when the browser media plane
 *     lands (`mediaPlaneLive: false` renders the honest "avatar presence" tag);
 *   - the speaking border is driven by the LIVE StreamDelta token rail (the
 *     same signal as the roster's speaking ring), never an animation on a
 *     timer;
 *   - the caption strip IS the active speaker's streaming turn — a real live
 *     transcript line;
 *   - controls: only real actions are enabled (hang-up → back to chat via the
 *     composed face toggle; CC → toggles the caption strip). Mic / camera /
 *     screenshare render honestly disabled — no fake toggles.
 */

import { html, nothing, type TemplateResult } from 'lit';
import type { LiveContentBody, LiveParticipantVM } from '@continuum/patterns';
import { fireLiveCameraToggle, fireLiveCaptionsToggle, fireLiveFaceToggle, fireLiveMicToggle } from '../render/parts';

/** Kind glyph for a tile with no stored avatar — the honest fallback face. */
function tileGlyph(kind: string): string {
  if (kind === 'human') return '🧑';
  if (kind === 'system') return '⚙️';
  return '🤖';
}

/** One participant tile — avatar face, presence dot (top-left), name tag
 *  (bottom-left), speaking border when the live rail flows. */
function participantTile(p: LiveParticipantVM): TemplateResult {
  const hide = (e: Event): void => {
    (e.currentTarget as HTMLElement).remove();
  };
  return html`<div
    class="live-tile"
    data-kind=${p.kind}
    data-speaking=${p.speaking ? '' : nothing}
    data-active=${p.active ? '' : nothing}
    title=${p.speaking ? `${p.name} — speaking` : p.name}
  >
    <span class="lt-glyph">${tileGlyph(p.kind)}</span>
    ${p.avatarUrl ? html`<img class="lt-img" src=${p.avatarUrl} alt="" @error=${hide} />` : nothing}
    ${p.hasVideo ? html`<canvas class="lt-video" data-sender=${p.id}></canvas>` : nothing}
    <span class="lt-status" data-on=${p.active ? '' : nothing} title=${p.active ? 'online' : 'offline'}></span>
    <span class="lt-name">
      ${p.name}${p.speaking ? html`<span class="lt-wave" aria-label="speaking">🔊</span>` : nothing}
    </span>
  </div>`;
}

/** One control button. `on` draws the active (latched) state of a real toggle;
 *  a disabled control carries its honest coming-soon reason in the tooltip. */
function controlBtn(opts: {
  readonly glyph: string;
  readonly label: string;
  readonly enabled: boolean;
  readonly title: string;
  readonly on?: boolean;
  readonly danger?: boolean;
  readonly badge?: number;
  readonly onClick?: (e: Event) => void;
}): TemplateResult {
  return html`<button
    class="live-btn"
    data-on=${opts.on ? '' : nothing}
    data-danger=${opts.danger ? '' : nothing}
    ?disabled=${!opts.enabled}
    title=${opts.title}
    aria-label=${opts.label}
    @click=${opts.enabled && opts.onClick ? opts.onClick : nothing}
  >
    <span class="live-btn-glyph">${opts.glyph}</span>
    ${opts.badge ? html`<span class="live-btn-badge">${opts.badge}</span>` : nothing}
  </button>`;
}

/** The live call face. Anti-disappearance: an empty room renders its honest
 *  empty state inside the same frame; the controls bar always renders. */
/** The call composition — TikTok's own two canonical layouts
 *  (docs/images/reference/tiktok-live/panel-vs-grid-canonical.jpeg):
 *  PANEL when someone is SPEAKING (the active speaker takes the full bleed,
 *  everyone else shrinks to a right rail — driven by the REAL StreamDelta
 *  rail, so focus follows actual tokens, never a timer), GRID otherwise
 *  (equal tiles). `data-composition` carries the state for CSS + tests. */
function renderComposition(body: LiveContentBody): TemplateResult {
  const focused = body.participants.find((p) => p.speaking);
  if (focused) {
    const rail = body.participants.filter((p) => p.id !== focused.id);
    return html`<div class="live-panel" data-composition="panel">
      <div class="live-stage">${participantTile(focused)}</div>
      ${rail.length > 0
        ? html`<div class="live-rail">${rail.map(participantTile)}</div>`
        : nothing}
    </div>`;
  }
  return html`<div class="live-grid" data-composition="grid" data-count=${body.participants.length}>
    ${body.participants.map(participantTile)}
  </div>`;
}

export function renderLive(body: LiveContentBody): TemplateResult {
  const hangup = (e: Event): void => {
    fireLiveFaceToggle(e, false);
  };
  const captions = (e: Event): void => {
    fireLiveCaptionsToggle(e);
  };
  const cc = body.controls;
  return html`<div class="live-room" data-room=${body.roomId}>
    <div class="live-head">
      <span class="live-title"><span class="live-title-dot"></span>${body.roomName} — live</span>
      ${body.mediaPlaneLive
        ? nothing
        : html`<span
            class="live-plane-chip"
            title="tiles are stored avatars + live presence/speaking — real video tracks land with the browser media plane"
            >avatar presence</span
          >`}
    </div>
    ${body.participants.length === 0
      ? html`<div class="live-empty">
          No one is in this room yet — tiles light up as citizens arrive.
        </div>`
      : renderComposition(body)}
    ${body.caption
      ? html`<div class="live-caption" aria-live="polite">
          <span class="live-caption-name">${body.caption.speakerName}:</span>
          <span class="live-caption-text">${body.caption.text}<span class="live-caret">▋</span></span>
        </div>`
      : nothing}
    <div class="live-controls" role="toolbar" aria-label="call controls">
      ${controlBtn({
        glyph: '🎤',
        label: 'microphone',
        enabled: cc.micAvailable,
        on: cc.micOn,
        title: cc.micAvailable
          ? cc.micOn
            ? 'mic live — click to mute'
            : 'click to speak'
          : 'connect the media plane to speak (core call server not reachable)',
        onClick: cc.micAvailable ? (e: Event) => fireLiveMicToggle(e) : undefined,
      })}
      ${controlBtn({
        glyph: '🎥',
        label: 'camera',
        enabled: cc.cameraAvailable,
        on: cc.cameraOn,
        title: cc.cameraAvailable
          ? cc.cameraOn
            ? 'camera live — click to stop'
            : 'start your camera'
          : 'camera enables when the call connects',
        onClick: cc.cameraAvailable ? (e: Event) => fireLiveCameraToggle(e) : undefined,
      })}
      ${controlBtn({
        glyph: '🖥️',
        label: 'screenshare',
        enabled: cc.screenshareAvailable,
        title: 'coming soon — screenshare lands with the browser media plane',
      })}
      ${controlBtn({
        glyph: 'CC',
        label: 'captions',
        enabled: cc.captionsAvailable,
        on: cc.captionsOn,
        title: cc.captionsOn ? 'captions on — live transcript line' : 'captions off',
        onClick: captions,
      })}
      ${controlBtn({
        glyph: '📄',
        label: 'transcript',
        enabled: false,
        badge: cc.transcriptCount,
        title: `coming soon — transcript panel (${cc.transcriptCount} messages in this room's transcript)`,
      })}
      ${controlBtn({
        glyph: '📞',
        label: 'leave call',
        enabled: cc.hangupAvailable,
        danger: true,
        title: 'leave — back to the room conversation',
        onClick: hangup,
      })}
    </div>
  </div>`;
}
