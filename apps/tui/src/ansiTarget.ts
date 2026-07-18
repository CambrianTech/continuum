/**
 * `createAnsiTarget` — positron's terminal `RenderTarget`. The ANSI twin of apps/web's
 * `webTarget`: it draws the SAME neutral `WorkspaceView` (who/what/where), but to a
 * plain multi-line string instead of Lit markup. One `chatApp`, mounted here, renders
 * the terminal frame; mounted on `webTarget`, the browser three-panel. Define once,
 * paint per surface.
 *
 * The roster's ANSI glyph is derived from the member KIND (carried on the neutral cell
 * as `badges[0]`), so the terminal picks its own ASCII marker (`* > ~`) while the web
 * cell keeps its emoji — the target owns the glyph, the cell owns the data. Pure: no
 * `process`/`stdout`; returns a frame body. `useColor` is closed over so the target
 * satisfies `RenderTarget<string>` (no per-call flag).
 */

import { createContentRegistry, type RenderTarget, type WorkspaceView, type ListingView, type ListingCell, type ContentView, type ContextPanelView, type PanelWidget } from '@continuum/patterns';
import type { ChatContentBody, MessageRowVM, MemberKind } from '@continuum/chat-view';

const SGR = {
  reset: '\x1b[0m', bold: '\x1b[1m', dim: '\x1b[2m',
  green: '\x1b[32m', cyan: '\x1b[36m', yellow: '\x1b[33m', gray: '\x1b[90m',
} as const;
type SgrKey = keyof typeof SGR;

/** ASCII glyph per author kind — deterministic column widths across terminals. */
function kindGlyph(kind: string): string {
  switch (kind as MemberKind) {
    case 'human': return '>';
    case 'agent': return '*';
    case 'system': return '~';
    default: return '*';
  }
}

/** Build a terminal target for one colour setting. */
export function createAnsiTarget(useColor = true): RenderTarget<string> {
  const paint = (code: SgrKey, text: string): string =>
    useColor ? `${SGR[code]}${text}${SGR.reset}` : text;
  const runtimeTag = (runtime: string): string => (runtime ? ' ' + paint('gray', `[${runtime}]`) : '');

  /** One roster line from the NEUTRAL cell — WHO is here, with a live presence marker. */
  const rosterLine = (c: ListingCell): string => {
    const active = c.status === 'active';
    const dot = active ? paint('green', '●') : paint('gray', '○');
    const name = active ? c.title : paint('dim', c.title);
    const kind = c.badges?.[0] ?? 'agent';
    const runtime = c.badges?.[1] ?? '';
    return `  ${dot} ${kindGlyph(kind)} ${name}${runtimeTag(runtime)}`;
  };

  const messageLine = (msg: MessageRowVM): string => {
    const time = paint('gray', msg.time);
    const sender = paint('bold', msg.senderName);
    return `  ${time} ${kindGlyph(msg.kind)} ${sender}${runtimeTag(msg.runtime)}: ${msg.content}`;
  };

  const content = createContentRegistry<string>();
  content.register<ChatContentBody>('chat', (body) =>
    body.isEmpty
      ? paint('dim', '  No messages yet — say hello.')
      : body.messages.map(messageLine).join('\n'),
  );

  return {
    listing(view: ListingView): string {
      if (view.cells.length === 0) return paint('dim', '  (no one here yet)');
      return view.cells.map(rosterLine).join('\n');
    },
    content(view: ContentView): string {
      return content.render(view);
    },
    contextPanel(_view: ContextPanelView): string {
      return '';
    },
    /** One left-rail widget. A `'listing'` draws its rows; a terminal can't paint a
     *  sparkline, so a richer kind (metrics/status) degrades to its title line — the
     *  ANSI analogue of mobile dropping the dossier. */
    widget(view: PanelWidget): string {
      if (view.kind === 'listing') return this.listing(view.body as ListingView);
      return paint('dim', `  [${view.title}]`);
    },
    workspace(ws: WorkspaceView): string {
      const room = ws.nav.cells[0];
      // Count comes from the roster — the first `kind:'listing'` widget in the rail.
      const rosterWidget = ws.left.find((w) => w.kind === 'listing');
      const roster = rosterWidget ? (rosterWidget.body as ListingView).cells : [];
      const activeCount = roster.filter((c) => c.status === 'active').length;
      const lines: string[] = [];
      lines.push(
        `${paint('cyan', paint('bold', room?.title ?? ''))}  ${paint('dim', `${activeCount}/${roster.length} here`)} · ${paint('green', '●')} ${paint('dim', 'live')}`,
      );
      // The global widget stack: each widget titled, drawn by kind.
      for (const w of ws.left) {
        lines.push('');
        lines.push(paint('yellow', w.title.toUpperCase()));
        lines.push(this.widget(w));
      }
      lines.push('');
      lines.push(paint('yellow', 'WHAT'));
      lines.push(this.content(ws.content));
      return lines.join('\n');
    },
  };
}
