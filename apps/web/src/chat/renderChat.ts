/**
 * `renderChat` — the pure Lit template for the three-panel who/what/where surface.
 *
 * Takes an already-projected `ChatViewModel` and returns markup. All "how it reads"
 * logic lives upstream in the view model + the pattern projections; this file only
 * lays out the panels and **dispatches the center by the room's `purpose`** through
 * the web Content registry — so the same shell renders chat today and foundry when
 * its renderer registers (ACTIVITY-ROOM-PATTERNS.md). The member cards + message
 * rows are shared fragments (`../render/parts`).
 *
 *   ┌─────────────────────────────────────────────┐
 *   │ header — WHERE/WHICH (room + counts)         │
 *   ├───────────────┬─────────────────────────────┤
 *   │ roster — WHO  │ Content — WHAT              │  ← dispatched by purpose
 *   │ (Listing)     │ (chat → conversation)       │
 *   └───────────────┴─────────────────────────────┘
 * (the compose bar under WHAT is owned by `<chat-widget>`, which needs the input
 * state + send handler — this function renders only the read surface.)
 */

import { type TemplateResult } from 'lit';
import { chatWorkspace, type ChatViewModel } from '@continuum/chat-view';
import type { NavViewState } from '@continuum/sdk-typescript';
import { webTarget } from '../render/litTarget';

/** The read surface: header + roster `Listing` + purpose-dispatched Content.
 *
 * Now a thin delegation through positron's framework path: project the VM onto the
 * neutral `WorkspaceView` (`chatWorkspace`) and let the web `RenderTarget` paint it.
 * The markup is byte-identical to the former inline template (screenshot-verified) —
 * the difference is architectural: the same projection a persona reads over RAG and a
 * mobile Flutter target will paint. `apps/web` flows through `mount(chatApp, …, webTarget)`.
 *
 * `nav` (the citizen's `kind="nav"` view) upgrades the rooms rail to the live room
 * set with unread badges when present. */
export function renderChat(vm: ChatViewModel, nav?: NavViewState): TemplateResult {
  return webTarget.workspace(chatWorkspace(vm, nav));
}
