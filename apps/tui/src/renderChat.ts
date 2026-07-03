/**
 * `renderChat` — the pure ANSI renderer for the terminal chat surface.
 *
 * It is the terminal twin of apps/web's Lit `renderChat`: same input (an
 * already-projected `ChatViewModel` from the SHARED `@continuum/chat-view`), a
 * totally different output (a plain multi-line string with optional ANSI colour,
 * not a DOM template). That two renderers this different consume one view model
 * without either reaching back into the SDK is the outlier-B proof — the "how a
 * message row reads" logic lives once, upstream, in `chatViewModel`.
 *
 * It stays pure: no `process`, no `stdout`, no screen control. It returns the
 * BODY of a frame; the composition root (`index.ts`) owns cursor/clear when it
 * paints. That keeps this unit-testable on plain text (call with `useColor:false`
 * and assert on the string) with no TTY.
 *
 * The layout is Joel's who/what/where design projected onto a line-oriented
 * terminal — three labelled sections rather than CSS-grid panels:
 *   WHERE/WHICH — the header (room + live counts + id)
 *   WHO         — the roster (presence per member)
 *   WHAT        — the conversation
 */

import type { ChatViewModel, MemberKind, MessageRowVM, RosterMemberVM } from '@continuum/chat-view';

/** SGR codes, applied only when `useColor` is on so tests read clean text. */
const SGR = {
  reset: '\x1b[0m',
  bold: '\x1b[1m',
  dim: '\x1b[2m',
  green: '\x1b[32m',
  cyan: '\x1b[36m',
  yellow: '\x1b[33m',
  gray: '\x1b[90m',
} as const;

type SgrKey = keyof typeof SGR;

/** Wrap `text` in an SGR code, or return it bare when colour is disabled. */
function paint(useColor: boolean, code: SgrKey, text: string): string {
  return useColor ? `${SGR[code]}${text}${SGR.reset}` : text;
}

/** Short glyph per author kind — the neutral human/agent/system discriminant.
 *  ASCII so column widths and tests stay deterministic across terminals. */
function kindGlyph(kind: MemberKind): string {
  switch (kind) {
    case 'human':
      return '>';
    case 'agent':
      return '*';
    case 'system':
      return '~';
  }
}

/** The runtime origin as a bracket tag, only when the substrate resolved one. */
function runtimeTag(useColor: boolean, runtime: string): string {
  return runtime ? ' ' + paint(useColor, 'gray', `[${runtime}]`) : '';
}

/** One roster line — WHO is here, with a live presence marker. */
function rosterLine(useColor: boolean, m: RosterMemberVM): string {
  const dot = m.active ? paint(useColor, 'green', '●') : paint(useColor, 'gray', '○');
  const name = m.active ? m.name : paint(useColor, 'dim', m.name);
  const glyph = kindGlyph(m.kind);
  return `  ${dot} ${glyph} ${name}${runtimeTag(useColor, m.runtime)}`;
}

/** One conversation line — WHAT was said. */
function messageLine(useColor: boolean, msg: MessageRowVM): string {
  const time = paint(useColor, 'gray', msg.time);
  const glyph = kindGlyph(msg.kind);
  const sender = paint(useColor, 'bold', msg.senderName);
  return `  ${time} ${glyph} ${sender}${runtimeTag(useColor, msg.runtime)}: ${msg.content}`;
}

/** Render the who/what/where surface from one view model to a frame body. */
export function renderChat(vm: ChatViewModel, useColor = true): string {
  const lines: string[] = [];

  // WHERE/WHICH — header.
  const title = paint(useColor, 'cyan', paint(useColor, 'bold', vm.roomName));
  const counts = paint(useColor, 'dim', `${vm.activeCount}/${vm.memberCount} here`);
  const id = paint(useColor, 'gray', vm.roomId);
  lines.push(`${title}  ${counts} · ${id}`);
  lines.push('');

  // WHO — roster.
  lines.push(paint(useColor, 'yellow', 'WHO'));
  if (vm.members.length === 0) {
    lines.push(paint(useColor, 'dim', '  (no one here yet)'));
  } else {
    for (const m of vm.members) lines.push(rosterLine(useColor, m));
  }
  lines.push('');

  // WHAT — conversation.
  lines.push(paint(useColor, 'yellow', 'WHAT'));
  if (vm.isEmpty) {
    lines.push(paint(useColor, 'dim', '  No messages yet — say hello.'));
  } else {
    for (const msg of vm.messages) lines.push(messageLine(useColor, msg));
  }

  return lines.join('\n');
}
