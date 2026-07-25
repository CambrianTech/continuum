/**
 * Persona home → pattern-primitive projections.
 *
 * The persona's profile/brain is the persona's HOME activity — an activity like
 * any room, reached by the SAME nav semantics (`nav/select` with a persona-kind
 * tab) and dispatched by purpose (`PERSONA_PURPOSE`) through the one Content
 * registry (POSITRON-PURE-ROOMS-BRIEF.md "persona home"; tabs==rooms==
 * activities — never a parallel route). This file is the PURE projection from
 * live state the surface already holds — the roster snapshot (presence, vitals
 * radiator, genes, loadout), the citizen's nav view, and the work board — onto
 * the neutral `PersonaContentBody` every target draws.
 *
 * Every number is REAL or explicitly absent ([[fallbacks-are-illegal-fail-loud]]):
 * the brain HUD regions light from the SAME `persona:vitals` faculty pulse the
 * roster tile's cognition compass draws — one signal, two zoom levels. A region
 * whose signal doesn't radiate yet (LIMBIC — no affect axis) carries
 * `level: undefined` and renders an honest awaiting state, never a fabricated 0.
 */

import type {
  ListingView,
  PersonaBrainRegionVM,
  PersonaClaimVM,
  PersonaContentBody,
  PersonaPathwayVM,
  PersonaRegionFact,
} from '@continuum/patterns';
import type { KanbanViewState, NavViewState } from '@continuum/sdk-typescript';
import type { ChatViewModel, RosterMemberVM } from './chatViewModel';

/** The persona-kind tab currently focused in the citizen's nav view, if any —
 *  the ONE fact the content dispatch keys off (`current_tab` + its tab's
 *  `kind === 'persona'`). Pure and shared so the workspace projection and its
 *  tests agree on "is a persona home on screen". */
export function focusedPersonaTab(
  nav: NavViewState | undefined,
): { id: string; title: string } | undefined {
  if (!nav?.current_tab) return undefined;
  const tab = nav.open_tabs.find((t) => t.id === nav.current_tab && t.kind === 'persona');
  return tab ? { id: tab.id, title: tab.title } : undefined;
}

/** Level → the region card's status word. The projection owns the wording
 *  (a target only paints): absent signal reads AWAITING — explicitly not a 0. */
function regionStatus(level: number | undefined): string {
  if (level === undefined) return 'AWAITING';
  if (level >= 66) return 'ACTIVE';
  if (level >= 33) return 'ENGAGED';
  if (level > 0) return 'QUIET';
  return 'IDLE';
}

/** Build one region card from the live vitals map. `facultyKey: ''` = no live
 *  axis radiates for this region yet (LIMBIC today) — level undefined, honest. */
function region(
  id: string,
  label: string,
  role: string,
  facultyKey: string,
  vitals: Readonly<Record<string, number>>,
  detail: readonly PersonaRegionFact[],
): PersonaBrainRegionVM {
  const raw = facultyKey ? vitals[facultyKey] : undefined;
  const level = raw === undefined ? undefined : Math.round(Math.max(0, Math.min(100, raw)));
  return {
    id,
    label,
    role,
    facultyKey,
    ...(level !== undefined ? { level } : {}),
    status: regionStatus(level),
    detail,
  };
}

/** A detail fact only when the vitals key is present — absent keys draw
 *  nothing, never a fabricated readout. */
function vitalFact(
  vitals: Readonly<Record<string, number>>,
  key: string,
  label: string,
): PersonaRegionFact[] {
  const v = vitals[key];
  return v === undefined ? [] : [{ label, value: String(Math.round(v)) }];
}

/** The brain HUD's five region cards — the reference wireframe's map
 *  (PREFRONTAL/LIMBIC/HIPPOCAMPUS/MOTOR CORTEX/CNS), each lit by its REAL
 *  faculty axis from the live pulse:
 *    PREFRONTAL ← reason (detail: focus) · HIPPOCAMPUS ← recall ·
 *    MOTOR CORTEX ← act (detail: queue) · CNS ← activity (detail: speed/size) ·
 *    LIMBIC ← (no affect axis radiates yet — honest awaiting).
 *  A memory COUNT (the reference's "1328 · 664 KB") needs an engram count feed
 *  the state stream doesn't carry yet — omitted, never invented. */
export function brainRegions(
  vitals: Readonly<Record<string, number>>,
): readonly PersonaBrainRegionVM[] {
  return [
    region('prefrontal', 'Prefrontal', 'executive', 'reason', vitals, [
      ...vitalFact(vitals, 'focus', 'Focus'),
    ]),
    region('limbic', 'Limbic', 'emotion', '', vitals, []),
    region('hippocampus', 'Hippocampus', 'memory', 'recall', vitals, []),
    region('motor', 'Motor Cortex', 'actions', 'act', vitals, [
      ...vitalFact(vitals, 'queue', 'Queue'),
    ]),
    region('cns', 'CNS', 'integration', 'activity', vitals, [
      ...vitalFact(vitals, 'speed', 'Speed'),
      ...vitalFact(vitals, 'size', 'Params'),
    ]),
  ];
}

/** The PATHWAYS grid — each tile a nav intent. In-content anchors (#brain,
 *  #genome — sections this surface carries) are enabled; destinations that
 *  aren't activities yet (DM room, logs, memory garden, stats) render honestly
 *  disabled until their rooms exist. */
export function personaPathways(): readonly PersonaPathwayVM[] {
  return [
    { id: 'brain', label: 'Brain', sublabel: 'cognitive view', glyph: '🧠', target: '#brain', enabled: true },
    { id: 'genome', label: 'Genome', sublabel: 'adapters & layers', glyph: '🧬', target: '#genome', enabled: true },
    { id: 'dm', label: 'DM', sublabel: 'message directly', glyph: '💬', target: '', enabled: false },
    { id: 'logs', label: 'Logs', sublabel: 'cognition logs', glyph: '📋', target: '', enabled: false },
    { id: 'memory', label: 'Memory', sublabel: 'long-term store', glyph: '💾', target: '', enabled: false },
    { id: 'stats', label: 'Stats', sublabel: 'activity metrics', glyph: '📊', target: '', enabled: false },
  ];
}

/** The persona's work-board claims — cards it is assignee of, newest event
 *  first. Real board rows only; the caller marks liveness separately. */
export function personaClaims(
  board: KanbanViewState,
  personaId: string,
): readonly PersonaClaimVM[] {
  return board.cards
    .filter((c) => c.assignee_id === personaId)
    .map((c) => ({
      id: c.card_id,
      title: c.title,
      state: c.state,
      priority: c.priority.toUpperCase(),
      updatedAtMs: c.updated_at,
    }))
    .sort((a, b) => b.updatedAtMs - a.updatedAtMs);
}

/** Project the persona HOME body from state the surface already holds. The
 *  persona may be absent from the current room's roster (it lives elsewhere) —
 *  then `awaitingIdentity` keeps every section framed with awaiting states
 *  (anti-disappearance), using the nav tab's title as the display name. */
export function personaContentBody(
  vm: ChatViewModel,
  persona: { id: string; title: string },
  board?: KanbanViewState,
): PersonaContentBody {
  const member: RosterMemberVM | undefined = vm.members.find((m) => m.id === persona.id);
  const name = member?.name ?? persona.title;
  const vitals = member?.vitals ?? {};
  return {
    personaId: persona.id,
    name,
    handle: name ? name.toLowerCase().replace(/\s+/g, '-') : '',
    kind: member?.kind ?? 'agent',
    online: member?.active ?? false,
    runtime: member?.runtime ?? '',
    ...(member?.avatarUrl ? { avatarUrl: member.avatarUrl } : {}),
    ...(member?.loadout ? { loadout: member.loadout } : {}),
    lastSeenMs: member?.lastSeenMs ?? 0,
    vitals,
    genes: member?.genes ?? [],
    regions: brainRegions(vitals),
    pathways: personaPathways(),
    claims: board ? personaClaims(board, persona.id) : [],
    claimsLive: board !== undefined,
    // Honest-empty until the blog/wonder-work feed exists as real data.
    writings: [],
    awaitingIdentity: member === undefined,
  };
}

/** Raw epoch ms → a relative stamp ("55m ago" / "2h ago" / "3d ago" / "now").
 *  `undefined` when the fact is absent/unusable — no line drawn, never a
 *  fabricated recency. Deterministic on its `nowMs` input (testable). */
export function agoText(lastActiveMs: number, nowMs: number): string | undefined {
  if (!lastActiveMs || lastActiveMs <= 0 || lastActiveMs > nowMs + 60_000) return undefined;
  const mins = Math.floor((nowMs - lastActiveMs) / 60_000);
  if (mins < 1) return 'now';
  if (mins < 60) return `${mins}m ago`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}h ago`;
  return `${Math.floor(hours / 24)}d ago`;
}

/** The persona home's right context panel — the persona FACTS listing (model,
 *  genes, presence, last active, claims), every line derived from state the
 *  surface already holds. Absent facts are absent lines, never placeholders. */
export function personaFactsListing(body: PersonaContentBody, nowMs = Date.now()): ListingView {
  const cells: ListingView['cells'][number][] = [];
  if (body.loadout?.model) {
    cells.push({ id: 'model', title: body.loadout.model, subtitle: 'model' });
  }
  cells.push({
    id: 'presence',
    title: body.online ? 'online' : 'offline',
    subtitle: 'presence',
    status: body.online ? 'active' : 'idle',
  });
  if (body.runtime) {
    cells.push({ id: 'runtime', title: body.runtime, subtitle: 'runtime' });
  }
  cells.push({
    id: 'genes',
    title: body.genes.length > 0 ? body.genes.join(', ') : 'none loaded',
    subtitle: `genome · ${body.genes.length} gene${body.genes.length === 1 ? '' : 's'}`,
  });
  const ago = agoText(body.lastSeenMs, nowMs);
  if (ago !== undefined) {
    cells.push({ id: 'last-active', title: ago, subtitle: 'last active', lastActiveMs: body.lastSeenMs });
  }
  if (body.claimsLive) {
    cells.push({
      id: 'claims',
      title: `${body.claims.length} claim${body.claims.length === 1 ? '' : 's'}`,
      subtitle: 'work board',
      ...(body.claims.length > 0 ? { count: body.claims.length } : {}),
    });
  }
  return { id: 'persona-facts', title: body.name || 'Persona', cells };
}
