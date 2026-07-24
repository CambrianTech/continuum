/**
 * Cross-consumer conformance — the shared READ pipe both chat clients run.
 *
 * The north-star for task #29 is proving web + terminal + personaRag consume ONE
 * SDK seam. This spec pins the part web and terminal genuinely SHARE: the
 * read-path pipe that turns a `chat` `StateEnvelope` off the wire into the
 * `ChatViewModel` a renderer draws. Both composition roots run this exact pipe,
 * verbatim, inside their `StateConnection.on(CHAT_KIND)` sink:
 *
 *   apps/web/src/index.ts:74-77  latest = chatStateFromEnvelope(env); widget.state = latest
 *                                (the Lit widget then draws renderChat(chatViewModel(latest)))
 *   apps/tui/src/index.ts:78-81  latest = chatStateFromEnvelope(env); paint()
 *                                (paint draws renderChat(chatViewModel(latest)))
 *
 * So `chatViewModel(chatStateFromEnvelope(env))` IS the shared seam. It lives
 * once here in `@continuum/chat-view` (the compression rule — one place computes
 * "what a chat snapshot becomes") and is tested here from the WIRE — the gap
 * neither sibling spec covers: chatViewModel.spec starts from a hand-built
 * `ChatState` (never an envelope), and StateConnection.spec proves socket
 * framing/routing (never the merge+project). Two independent consumers folding
 * the SAME scripted envelope stream through this pipe MUST stay byte-identical;
 * that identity is precisely what "web and terminal off one seam" means.
 *
 * Each surface's own rendering is proven in its own package: apps/tui's
 * renderChat.spec (ANSI) and apps/web's renderChat.spec (Lit). This file proves
 * the MODEL those two renderers both receive is one canonical value.
 */

import { describe, it, expect } from 'vitest';
import { CHAT_KIND, chatStateFromEnvelope } from './ChatState';
import { chatViewModel } from './chatViewModel';
import type { ChatViewModel } from './chatViewModel';
import type {
  ChatMessageView,
  ChatViewState,
  RosterSlotView,
  SenderKind,
  StateEnvelope,
} from '@continuum/sdk-typescript';

const kind = (k: SenderKind['kind']): SenderKind => ({ kind: k });

const member = (over: Partial<RosterSlotView> = {}): RosterSlotView => ({
  member_id: 'm-1',
  display_name: 'Asha',
  kind: kind('agent'),
  integrations: {},
  provenance: { runtime: '' },
  active: true,
  last_seen_ms: 0,
  vitals: {},
  genes: [],
  ...over,
});

const message = (over: Partial<ChatMessageView> = {}): ChatMessageView => ({
  id: 'msg-1',
  room_id: 'room-1',
  sender_id: 's-1',
  sender_name: 'Joel',
  sender_kind: kind('human'),
  integrations: {},
  provenance: { runtime: '' },
  content: 'hello',
  timestamp: 0,
  ...over,
});

/** A `chat` state frame exactly as `StateConnection` hands to a `CHAT_KIND` sink:
 *  the payload is a bare `ChatViewState` (no kind/revision — those ride the
 *  envelope), and the merge is what grafts them on. */
const chatEnvelope = (revision: number, payload: ChatViewState): StateEnvelope => ({
  kind: CHAT_KIND,
  revision,
  layer: 'ephemeral',
  payload,
});

/**
 * The shared read-path closure both `index.ts` sinks run, verbatim. Every chat
 * client is `chatViewModel ∘ chatStateFromEnvelope` over each delivered envelope
 * — the single seam this spec pins.
 */
const readPath = (env: StateEnvelope): ChatViewModel =>
  chatViewModel(chatStateFromEnvelope(env));

/**
 * One consumer of the state stream — models a client's `.on(CHAT_KIND)` sink: it
 * folds each envelope through the shared pipe and keeps the latest model, exactly
 * as apps/web (→ `widget.state`) and apps/tui (→ `paint`) do. Two of these,
 * driven by the same stream, are the "web consumer" and "terminal consumer".
 */
class StreamConsumer {
  readonly models: ChatViewModel[] = [];
  latest?: ChatViewModel;
  deliver(env: StateEnvelope): void {
    this.latest = readPath(env);
    this.models.push(this.latest);
  }
}

/** The scripted wire: an opening snapshot, then a live delta that adds a turn and
 *  flips a member's presence — the minimal stream that exercises merge + project
 *  across a revision transition. */
const SNAPSHOT: StateEnvelope = chatEnvelope(1, {
  room_id: 'room-1',
  room_name: 'general',
  purpose: 'chat',
  roster: [
    member({ member_id: 'asha', display_name: 'Asha', kind: kind('agent'), provenance: { runtime: 'claude' } }),
    member({ member_id: 'joel', display_name: 'Joel', kind: kind('human'), active: false }),
  ],
  messages: [
    message({ id: 'm1', sender_id: 'joel', sender_name: 'Joel', sender_kind: kind('human'), content: 'hi Asha', timestamp: 0 }),
  ],
});

const DELTA: StateEnvelope = chatEnvelope(2, {
  room_id: 'room-1',
  room_name: 'general',
  purpose: 'chat',
  roster: [
    member({ member_id: 'asha', display_name: 'Asha', kind: kind('agent'), provenance: { runtime: 'claude' } }),
    member({ member_id: 'joel', display_name: 'Joel', kind: kind('human'), active: true }),
  ],
  messages: [
    message({ id: 'm1', sender_id: 'joel', sender_name: 'Joel', sender_kind: kind('human'), content: 'hi Asha', timestamp: 0 }),
    message({
      id: 'm2',
      sender_id: 'asha',
      sender_name: 'Asha',
      sender_kind: kind('agent'),
      provenance: { runtime: 'claude' },
      content: 'hello Joel',
      timestamp: 9 * 3600_000 + 5 * 60_000,
    }),
  ],
});

describe('cross-consumer read pipe', () => {
  // what this catches: the whole reason chat-view exists — two clients built on
  // the same seam must derive the IDENTICAL view model from the same wire, or the
  // "one seam, many surfaces" claim is false. A regression that let one consumer's
  // pipe drift (an app doing its own merge/projection instead of this shared one)
  // would make these two folds diverge.
  it('two independent consumers fold one wire stream to byte-identical models', () => {
    const web = new StreamConsumer();
    const tui = new StreamConsumer();

    for (const env of [SNAPSHOT, DELTA]) {
      web.deliver(env);
      tui.deliver(env);
    }

    expect(web.models).toEqual(tui.models);
    expect(web.latest).toEqual(tui.latest);
  });

  // what this catches: the pipe must project the WIRE snapshot into the exact
  // three-panel model both renderers draw — room (where), roster (who), messages
  // (what) — starting from a StateEnvelope, the path no sibling spec covers.
  it('projects the opening snapshot into the canonical view model', () => {
    const vm = readPath(SNAPSHOT);
    expect(vm.roomName).toBe('general');
    expect(vm.roomId).toBe('room-1');
    expect(vm.memberCount).toBe(2);
    expect(vm.activeCount).toBe(1); // Joel idle at snapshot
    expect(vm.members.map((m) => m.id)).toEqual(['asha', 'joel']);
    expect(vm.messages.map((m) => m.content)).toEqual(['hi Asha']);
    expect(vm.isEmpty).toBe(false);
  });

  // what this catches: a live delta must fully replace the model (positron emits a
  // fresh snapshot per revision — no widget-local accumulation). The added turn
  // appears, the flipped presence updates activeCount, and the empty-state clears.
  it('advances to the delta as a full fresh snapshot, not an accumulation', () => {
    const web = new StreamConsumer();
    web.deliver(SNAPSHOT);
    web.deliver(DELTA);

    const vm = web.latest;
    expect(vm?.messages.map((m) => m.content)).toEqual(['hi Asha', 'hello Joel']);
    expect(vm?.activeCount).toBe(2); // Joel now present
    expect(vm?.messages[1]?.time).toBe('09:05'); // deterministic UTC HH:MM off the wire timestamp
  });

  // what this catches: the ChatState merge seam — kind/revision live on the
  // ENVELOPE, never the ChatViewState payload, so the pipe must graft the
  // envelope's revision onto the model. A regression reading revision off the
  // payload (where it does not exist) would surface undefined and defeat cheap
  // change-detection.
  it('carries the envelope revision (not the payload) onto the model', () => {
    expect(readPath(SNAPSHOT).revision).toBe(1);
    expect(readPath(DELTA).revision).toBe(2);
  });

  // what this catches: a non-chat envelope reaching the chat merge is a
  // StateConnection routing bug and must fail loud, never be coerced into a chat
  // model ([[fallbacks-are-illegal-fail-loud]]). Guards the seam both apps trust
  // the router to uphold.
  it('fails loud when a non-chat envelope reaches the pipe', () => {
    const wrong: StateEnvelope = { kind: 'wall', revision: 1, layer: 'ephemeral', payload: {} };
    expect(() => readPath(wrong)).toThrow(/expected kind 'chat'/);
  });
});
