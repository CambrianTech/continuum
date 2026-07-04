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

import { chatWorkspace, type ChatViewModel } from '@continuum/chat-view';
import { createAnsiTarget } from './ansiTarget';

/** Render the who/what/where surface from one view model to a frame body.
 *
 * Now a thin delegation through positron's framework path: project the VM onto the
 * neutral `WorkspaceView` (`chatWorkspace`) and let the terminal `RenderTarget` paint
 * it. Output is byte-identical to the former inline renderer (verified by the tui specs
 * + `npm run frame`) — the difference is architectural: the SAME projection the web
 * `webTarget` paints as Lit and a persona reads over RAG. One `chatApp`, per-surface paint. */
export function renderChat(vm: ChatViewModel, useColor = true): string {
  return createAnsiTarget(useColor).workspace(chatWorkspace(vm));
}
