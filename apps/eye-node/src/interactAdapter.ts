/**
 * interactAdapter — fulfils `perception/interact` and `perception/session-close` (card
 * 3569675f): a persona drives a LIVE page across calls and sees it after each step.
 *
 * observe and hot-edit reopen the page per call. A citizen checking her own site needs the
 * page to persist: open her dev server, click through a flow, fill a form, and see what each
 * step did. This adapter keeps each `PerceptionSession` behind an opaque handle, so the next
 * call continues on the same page (cookies, storage and navigation kept).
 *
 * ## Bounded, never leaked
 *
 * A browser per session is real memory. Sessions close after {@link IDLE_MS} without a call,
 * at most {@link MAX_SESSIONS} are open at once (opening one more is refused with the reason,
 * never an eviction of someone's live session), and `stop()` closes them all.
 *
 * ## Hers alone, and only the web
 *
 * A session will hold her logins on job sites, so it is bound to the citizen who opened it:
 * the core stamps the VERIFIED caller into `_callerPeerId` (never trusted from the request), and
 * a handle used by anyone else is refused, so a handle pasted into a room cannot let another
 * citizen drive her account. `target` and `goto` must be http(s): a `file://` page would render
 * local files (config.env, keys) into her observation (Fable on #4551).
 *
 * ## Never throws
 *
 * Like the siblings, every failure comes back as `{ success: false, error }`, the honest bare
 * contract, never a fabricated observation.
 */

import { randomUUID } from 'node:crypto';

import { PerceptionSession } from '@continuum/perception';
import type { DomAction, Observation } from '@continuum/perception';

import type { InteractParams } from '../../../protocol/typescript/perception/InteractParams';
import type { InteractResult } from '../../../protocol/typescript/perception/InteractResult';
import type { PerceptionAction } from '../../../protocol/typescript/perception/PerceptionAction';
import type { SessionCloseParams } from '../../../protocol/typescript/perception/SessionCloseParams';
import type { SessionCloseResult } from '../../../protocol/typescript/perception/SessionCloseResult';

import { mapNode, perceptToImage } from './observeAdapter';

/** A session idle this long is closed and its browser released. */
export const IDLE_MS = 10 * 60 * 1000;
/** The most sessions (browsers) one eye-node holds at once. */
export const MAX_SESSIONS = 8;

type WebSession = Awaited<ReturnType<typeof PerceptionSession.openWeb>>;

/** The field the core stamps with the verified caller's peer id (`CALLER_PEER_FIELD` in Rust). */
export const CALLER_PEER_FIELD = '_callerPeerId';

/** The verified caller the core forwarded, or undefined for a local operator call. */
export function callerOf(raw: unknown): string | undefined {
  const v = (raw as Record<string, unknown> | null)?.[CALLER_PEER_FIELD];
  return typeof v === 'string' && v.length > 0 ? v : undefined;
}

/** An http(s) URL, never file://, data: or any other scheme a browser would render. */
export function isWebUrl(url: string): boolean {
  try {
    const { protocol } = new URL(url);
    return protocol === 'http:' || protocol === 'https:';
  } catch {
    return false;
  }
}

/** The wire action onto the surface's driver verb. Exhaustive: a new wire kind must be mapped. */
export function toDomAction(action: PerceptionAction): DomAction {
  switch (action.kind) {
    case 'click':
      return { kind: 'click', selector: action.selector };
    case 'type':
      return { kind: 'type', selector: action.selector, text: action.text };
    case 'press':
      return { kind: 'press', key: action.key };
    case 'hover':
      return { kind: 'hover', selector: action.selector };
    case 'goto':
      return { kind: 'goto', url: action.url };
  }
}

interface Held {
  readonly session: WebSession;
  /** The verified caller that opened it; only they may drive or close it. */
  readonly owner: string | undefined;
  lastUsedMs: number;
}

/** Opens a web session; injectable so the registry is testable without a browser. */
export type OpenWeb = (url: string, viewport?: { width: number; height: number }) => Promise<WebSession>;

export class InteractSessions {
  private readonly held = new Map<string, Held>();
  private readonly sweeper: ReturnType<typeof setInterval>;
  private readonly active = new Set<Promise<InteractResult>>();
  private stopping = false;
  private stopped?: Promise<void>;

  constructor(
    private readonly openWeb: OpenWeb = (url, viewport) => PerceptionSession.openWeb({ url, viewport }),
    private readonly now: () => number = Date.now,
  ) {
    this.sweeper = setInterval(() => void this.sweep(), 60_000);
    this.sweeper.unref?.();
  }

  /** Continue `params.session`, or open one at `params.target`; take the actions; observe. */
  interact(params: InteractParams): Promise<InteractResult> {
    if (this.stopping) return Promise.resolve(failure('eye-node is stopping; session unavailable'));
    const pending = this.performInteract(params).finally(() => this.active.delete(pending));
    this.active.add(pending);
    return pending;
  }

  private async performInteract(params: InteractParams): Promise<InteractResult> {
    let handle = params.session;
    const caller = callerOf(params);
    try {
      await this.sweep();
      const offWeb = [params.target, ...params.actions.map((a) => (a.kind === 'goto' ? a.url : undefined))]
        .filter((u): u is string => u !== undefined)
        .find((u) => !isWebUrl(u));
      if (offWeb !== undefined) return failure(`'${offWeb}' is not an http(s) URL; a session only opens web pages`);
      let held: Held | undefined;
      if (handle) {
        held = this.held.get(handle);
        if (!held) {
          return failure(`no live session '${handle}' (closed, expired after ${IDLE_MS / 60000} idle minutes, or from another eye-node); open a new one with target`);
        }
        if (held.owner !== caller) return failure(`session '${handle}' belongs to another citizen`);
      } else {
        if (!params.target) return failure('pass target (a URL) to open a session, or session to continue one');
        if (this.held.size >= MAX_SESSIONS) {
          return failure(`this eye-node already holds ${MAX_SESSIONS} open sessions; close one with perception/session-close`);
        }
        const viewport = params.viewport ? { width: params.viewport.width, height: params.viewport.height } : undefined;
        const session = await this.openWeb(params.target, viewport);
        handle = randomUUID();
        held = { session, owner: caller, lastUsedMs: this.now() };
        this.held.set(handle, held);
      }
      held.lastUsedMs = this.now();
      const view = params.selector ? { selector: params.selector } : undefined;
      if (params.actions.length === 0) {
        const observation = await held.session.observe(view);
        return success(observation, handle);
      }
      const { observation, delta } = await held.session.interact(params.actions.map(toDomAction), view);
      return {
        ...success(observation, handle),
        delta: { pixelsChanged: delta.pixelsChanged, totalPixels: delta.totalPixels, ratio: delta.ratio },
      };
    } catch (err) {
      // the session (if any) stays open: one failed click must not cost her the page
      return { ...failure(err instanceof Error ? err.message : String(err)), session: handle };
    }
  }

  async close(params: SessionCloseParams): Promise<SessionCloseResult> {
    const held = this.held.get(params.session);
    if (!held) return { success: true, closed: false };
    if (held.owner !== callerOf(params)) {
      return { success: false, closed: false, error: `session '${params.session}' belongs to another citizen` };
    }
    this.held.delete(params.session);
    try {
      await held.session.close();
      return { success: true, closed: true };
    } catch (err) {
      return { success: false, closed: true, error: err instanceof Error ? err.message : String(err) };
    }
  }

  /** Close every session idle past {@link IDLE_MS}. */
  async sweep(): Promise<void> {
    const now = this.now();
    const stale = [...this.held].filter(([, h]) => now - h.lastUsedMs >= IDLE_MS);
    for (const [handle, h] of stale) {
      this.held.delete(handle);
      await h.session.close().catch(() => undefined);
    }
  }

  get size(): number {
    return this.held.size;
  }

  /** Close everything (the eye-node is stopping). */
  closeAll(): Promise<void> {
    if (this.stopped) return this.stopped;
    this.stopping = true;
    clearInterval(this.sweeper);
    this.stopped = (async () => {
      // An openWeb already in flight can publish a session after stop begins.
      // Drain accepted calls before taking the final set of browsers to close.
      await Promise.allSettled([...this.active]);
      const all = [...this.held.values()];
      this.held.clear();
      await Promise.all(all.map((h) => h.session.close().catch(() => undefined)));
    })();
    return this.stopped;
  }
}

function success(observation: Observation, session: string): InteractResult {
  return {
    success: true,
    url: observation.structure.url,
    title: observation.structure.title,
    image: perceptToImage(observation.percept),
    structure: mapNode(observation.structure.tree),
    session,
  };
}

function failure(error: string): InteractResult {
  return { success: false, error };
}
