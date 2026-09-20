/**
 * The CHAT PORTAL, defined ONCE as a positron app.
 *
 * `chatApp` is the first real `defineApp` consumer: it projects a chat snapshot
 * (`ChatState`) → the neutral who/what/where `WorkspaceView` by composing the two
 * pieces that already exist — `chatViewModel` (snapshot → render-ready VM) and
 * `chatWorkspace` (VM → the neutral shell). Nothing here knows about the DOM, Lit,
 * Flutter, ANSI, or the SDK.
 *
 * Mount it on ANY `RenderTarget` and the SAME three-panel portal renders everywhere:
 * ```ts
 * mount(chatApp, sdkSource, webTarget,     domSink);      // web   (Lit)
 * mount(chatApp, sdkSource, flutterTarget, flutterSink);  // mobile(Flutter)
 * mount(chatApp, sdkSource, ragTarget,     ragBuffer);    // agents(RAG)
 * ```
 * Define once, render on every modality.
 */

import { defineApp, type AppDefinition } from '@continuum/patterns';
import { chatViewModel } from './chatViewModel';
import { chatWorkspace } from './patternProjections';
import type { ChatState } from './ChatState';

/** The chat portal: `ChatState → WorkspaceView`, universe `continuum`. */
export const chatApp: AppDefinition<ChatState> = defineApp<ChatState>({
  universe: 'continuum',
  project: (state) => chatWorkspace(chatViewModel(state)),
});
