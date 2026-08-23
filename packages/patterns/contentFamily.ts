/**
 * `contentFamilyOf` — recipe purpose → render FAMILY, in ONE place.
 *
 * Recipes bind rooms to HIERARCHICAL purposes (`benchmark/hard-rs`,
 * `video-chat`), while the content registry keys renderers by FAMILY
 * (`bench`, `live`). Nothing resolved between them, so 3 of the 4 shipped
 * recipes painted `Interface error rendering this room` — a dispatched
 * benchmark round's room was unrenderable in the browser (measured
 * 2026-08-22; #431's scoreboard region was the whole point).
 *
 * Resolution is deliberately narrow and data-first:
 *  - a purpose that IS a family passes through unchanged;
 *  - a namespaced purpose resolves by its FIRST SEGMENT through the alias
 *    table (`benchmark/<anything>` → bench — rooms are URI path trees, the
 *    variant after the slash is the recipe's business, not the renderer's);
 *  - anything unknown passes through UNCHANGED so the registry still fails
 *    loud ([[fallbacks-are-illegal-fail-loud]]) — this function widens what
 *    renders, never silences what can't.
 *
 * The long-term home for this mapping is the recipe manifest itself (the
 * Experience pipe carries regions/layout today with no client reader); when
 * the client reads manifests, the alias table collapses into recipe data.
 */

import { ARENA_PURPOSE } from './arenaContent';
import { BENCH_PURPOSE } from './benchContent';
import { GRID_PURPOSE } from './gridContent';
import { LIVE_PURPOSE } from './liveContent';
import { PERSONA_PURPOSE } from './personaContent';
import { SERVING_PURPOSE } from './servingContent';
import { SETTINGS_PURPOSE } from './settingsContent';

/** Purposes that ARE registry families — identity mappings. */
const FAMILIES: ReadonlySet<string> = new Set([
  'chat',
  'foundry',
  ARENA_PURPOSE,
  BENCH_PURPOSE,
  GRID_PURPOSE,
  LIVE_PURPOSE,
  PERSONA_PURPOSE,
  SERVING_PURPOSE,
  SETTINGS_PURPOSE,
]);

/** Recipe purpose (or its first path segment) → family. Every entry names the
 *  shipped recipe that motivates it. */
const ALIASES: ReadonlyMap<string, string> = new Map([
  ['benchmark', BENCH_PURPOSE], // recipes/benchmark.json → purpose "benchmark/hard-rs"
  ['video-chat', LIVE_PURPOSE], // recipes/video-chat.json
  ['profile', PERSONA_PURPOSE], // recipes/profile.json
]);

/** Resolve a room purpose to its render family; unknown purposes return
 *  unchanged (the registry stays the fail-loud authority). */
export function contentFamilyOf(purpose: string): string {
  if (FAMILIES.has(purpose)) return purpose;
  const exact = ALIASES.get(purpose);
  if (exact) return exact;
  const head = purpose.split('/', 1)[0] ?? purpose;
  return ALIASES.get(head) ?? purpose;
}
