/**
 * The web `Content` registry — the MIME-table that dispatches the center panel by
 * the focused room's `purpose` (ACTIVITY-ROOM-PATTERNS.md). This is the seam that
 * makes `activity=room=content=tab` real in the browser: a `chat` room renders the
 * conversation, a `foundry` room its model list, each registered here. The shell
 * calls `render(workspace.content)`; the registry routes on `content.purpose` and
 * **fails loud** on an unregistered purpose — an unknown activity is a wiring bug,
 * never a blank ([[fallbacks-are-illegal-fail-loud]]).
 */

import { html, type TemplateResult } from 'lit';
import {
  ARENA_PURPOSE,
  BENCH_PURPOSE,
  CANVAS_PURPOSE,
  createContentRegistry,
  LIVE_PURPOSE,
  GRID_PURPOSE,
  PERSONA_PURPOSE,
  SERVING_PURPOSE,
  SETTINGS_PURPOSE,
  type ArenaContentBody,
  type BenchContentBody,
  type CanvasContentBody,
  type ContentRegistry,
  type GridContentBody,
  type LiveContentBody,
  type PersonaContentBody,
  type ServingContentBody,
  type SettingsContentBody,
} from '@continuum/patterns';
import type { ChatContentBody } from '@continuum/chat-view';
import { modelCell, type ForgeContentBody } from '@continuum/foundry-view';
import { actGroupRow, listingCell, messageRow } from '../render/parts';
import { ACADEMY_PURPOSE, type AcademyContentBody } from '@continuum/chat-view';
import type { BenchRunVM } from '@continuum/patterns';
import { renderPersona } from '../persona/renderPersona';
import { renderLive } from '../live/renderLive';
import { renderBench } from '../bench/renderBench';
import { renderArena } from '../arena/renderArena';
import { renderServing } from '../serving/renderServing';
import { renderGrid } from '../grid/renderGrid';
import { renderSettings } from '../settings/renderSettings';
import { renderCanvas } from '../canvas/renderCanvas';

/** The ACADEMY LANDING — the campus page a human lands on: a hero strip
 *  (who's here, what's running — the working wave carries the pulse), the
 *  LIVE benchmark board center-stage, and the room's own conversation as a
 *  disclosure below. Chat is a layer of the academy, never its face. */
function academyContent(body: AcademyContentBody): TemplateResult {
  const working = body.bench.runs.filter(
    (r: BenchRunVM) => r.state === 'working' || r.state === 'grading' || r.state === 'queued',
  ).length;
  return html`<div class="academy-landing">
    <div class="academy-hero">
      <div class="academy-title">Academy</div>
      <div class="academy-strip">
        <span class="academy-stat">${body.activeCount} <i>active</i></span>
        <span class="academy-stat">${body.memberCount} <i>citizens</i></span>
        ${working > 0
          ? html`<span class="academy-stat wave-active">${working} <i>working now</i></span>`
          : html`<span class="academy-stat">quiet — no runs in flight</span>`}
      </div>
    </div>
    ${renderBench(body.bench)}
    <details class="academy-chat">
      <summary>Room chat${body.chat.isEmpty ? ' — quiet' : ''}</summary>
      ${chatContent(body.chat)}
    </details>
  </div>`;
}

/** The chat activity's center: the conversation (or an honest empty state). */
function chatContent(body: ChatContentBody): TemplateResult {
  // The FULL transcript (#243): speech rows interleaved with collapsed tool-act
  // receipts, in timestamp order — the room shows the WORK, not just the words.
  // An older wire without acts folds to a messages-only transcript upstream.
  return body.isEmpty
    ? html`<div class="empty">No messages yet — say hello.</div>`
    : html`<ul class="messages">
        ${body.transcript.map((row) =>
          row.row === 'acts' ? actGroupRow(row) : messageRow(row),
        )}
      </ul>`;
}

/** The foundry activity's center: the model catalogue, drawn through the SAME
 *  generic `listingCell` the roster/rooms use — each model projected by foundry-view's
 *  `modelCell`. This is the outlier that proves the shell dispatches by purpose:
 *  chat → conversation, foundry → models, one registry, no shell change. */
function foundryContent(body: ForgeContentBody): TemplateResult {
  return body.models.length === 0
    ? html`<div class="empty">No models in the catalogue yet.</div>`
    : html`<ul class="cells">
        ${body.models.map((m) => listingCell(modelCell(m)))}
      </ul>`;
}

/** The app's Content registry. A new activity registers its renderer here keyed by
 *  purpose; the shell doesn't change. */
export const webContentRegistry: ContentRegistry<TemplateResult> =
  createContentRegistry<TemplateResult>();

webContentRegistry.register<ChatContentBody>('chat', (body) => chatContent(body));
webContentRegistry.register<ForgeContentBody>('foundry', (body) => foundryContent(body));
// The persona HOME — the profile + brain HUD center, dispatched when the
// focused tab is persona-kind (the projection publishes purpose "persona").
webContentRegistry.register<PersonaContentBody>(PERSONA_PURPOSE, (body) => renderPersona(body));
// The LIVE call face — the room's avatar-grid call surface, dispatched when the
// room recipe's purpose is "live", a live tab is focused, or the Go-live face
// is open (the projection publishes purpose "live").
webContentRegistry.register<LiveContentBody>(LIVE_PURPOSE, (body) => renderLive(body));
// The benchmark ARENA — ranked leaderboards + live-run strip from real eval
// ledger rows, dispatched when the room recipe's purpose is "arena".
webContentRegistry.register<ArenaContentBody>(ARENA_PURPOSE, (body) => renderArena(body));
// The SERVING console — per-node control-loop panels center-stage, dispatched
// when the room recipe's purpose is "serving" (the machine room, full view).
webContentRegistry.register<ServingContentBody>(SERVING_PURPOSE, (body) => renderServing(body));
// The Academy's live BENCHMARK BOARD — one progress row per run, operator and
// citizen-claimed alike, dispatched when the room recipe's purpose is "bench".
webContentRegistry.register<BenchContentBody>(BENCH_PURPOSE, (body) => renderBench(body));
webContentRegistry.register<AcademyContentBody>(ACADEMY_PURPOSE, (body) => academyContent(body));
// The GRID view — every node's panel (resources + serving), the NODES
// strip's full activity, dispatched when the room's purpose is "grid".
webContentRegistry.register<GridContentBody>(GRID_PURPOSE, (body) => renderGrid(body));
// The SETTINGS operator panel — covenant consent, HF identity, gene registry,
// dispatched when the header's Settings affordance opens the face.
webContentRegistry.register<SettingsContentBody>(SETTINGS_PURPOSE, (body) => renderSettings(body));
// The design-bench CANVAS region — the persona's rendered page live on stage
// (sandboxed iframe / last screenshot + craft scorecard), dispatched when the
// run room's purpose is "canvas" (DESIGN-BENCH-VISUAL-CRAFT.md §5).
webContentRegistry.register<CanvasContentBody>(CANVAS_PURPOSE, (body) => renderCanvas(body));
