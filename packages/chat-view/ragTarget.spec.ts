/**
 * ragTarget spec — proof the AUTOMATIC per-surface model works.
 *
 * The SAME `chatApp` + `WorkspaceView` that `webTarget` renders as a three-panel and
 * `ansiTarget` renders as WHO/WHAT sections, the RAG rule (`createRagTarget`) renders as
 * a concise grounding block for a persona's LLM context — room + who + primary content,
 * ALL chrome dropped. One definition, per-modality RULES, entirely different appropriate
 * output — like `@media (modality: rag)`. No per-app RAG design; the rule is authored once.
 */

import { describe, it, expect } from 'vitest';
import { mount, createRagTarget, createContentRegistry, type AppSource } from '@continuum/patterns';
import { chatApp } from './chatApp';
import type { ChatState, ChatContentBody } from './index';
import type { ChatMessageView, RosterSlotView, SenderKind } from '@continuum/sdk-typescript';

const kind = (k: SenderKind['kind']): SenderKind => ({ kind: k });
const member = (over: Partial<RosterSlotView> = {}): RosterSlotView => ({
  member_id: 'm-1', display_name: 'Asha', kind: kind('agent'), integrations: {},
  provenance: { runtime: '' }, active: true, last_seen_ms: 0, vitals: {}, genes: [], ...over,
});
const message = (over: Partial<ChatMessageView> = {}): ChatMessageView => ({
  id: 'msg-1', room_id: 'room-1', sender_id: 's-1', sender_name: 'Joel',
  sender_kind: kind('human'), integrations: {}, provenance: { runtime: '' },
  content: 'hello', timestamp: 0, ...over,
});
const chatState = (over: Partial<ChatState> = {}): ChatState => ({
  kind: 'chat', revision: 3, room_id: 'room-1', room_name: 'general',
  purpose: 'chat', messages: [], roster: [], ...over,
});

describe('createRagTarget — the RAG rule derives concise grounding automatically', () => {
  // what this catches: the RAG rule collapses the semantic WorkspaceView to room + who +
  // primary content, dropping nav/secondary/context chrome — a fundamentally different,
  // agent-appropriate output than the human targets, from the identical chatApp. If the
  // rule leaked chrome (WHO/Users & Agents headings) or dropped the who/content, RAG grounding
  // would be either bloated or useless. This is the automatic-per-surface model, demonstrated.
  it('renders room + who + primary content concisely, dropping all chrome', () => {
    const state = chatState({
      roster: [
        member({ member_id: 'a', display_name: 'Asha' }),
        member({ member_id: 'b', display_name: 'Solenne' }),
      ],
      messages: [message({ id: 'x', sender_name: 'Asha', content: 'working on vitals' })],
    });

    const content = createContentRegistry<string>();
    content.register<ChatContentBody>('chat', (b) =>
      b.isEmpty ? 'No messages yet.' : `latest: "${b.messages[b.messages.length - 1]?.content}"`,
    );
    const rag = createRagTarget(content);
    const src: AppSource<ChatState> = (onState) => {
      onState(state);
      return () => {};
    };

    let grounding = '';
    mount(chatApp, src, rag, (o) => (grounding = o));

    expect(grounding).toContain('You are in "general" with Asha, Solenne'); // room + who
    expect(grounding).toContain('latest: "working on vitals"'); // primary content
    expect(grounding).not.toContain('WHO'); // no terminal chrome
    expect(grounding).not.toContain('Users & Agents'); // no web chrome
    expect(grounding.split('\n').length).toBeLessThan(4); // concise — a grounding block, not a screen
  });
});
