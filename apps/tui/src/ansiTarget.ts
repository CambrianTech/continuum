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

import { createContentRegistry, type RenderTarget, type WorkspaceView, type ListingView, type ListingCell, type ContentView, type ContextPanelView, type PanelWidget, type GaugeView, type ContinuonView, type ServingPanelView, type SystemPanelView } from '@continuum/patterns';
import type { ChatContentBody, MessageRowVM, MemberKind } from '@continuum/chat-view';

/** Unicode block ramp for terminal sparklines — the gauge's ANSI face. */
const BLOCKS = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'] as const;

/** A 0..=100 series → a compact block sparkline (last `width` samples). */
function blockSpark(points: readonly number[], width = 24): string {
  const tail = points.slice(-width);
  return tail
    .map((p) => BLOCKS[Math.min(7, Math.max(0, Math.round((p / 100) * 7)))])
    .join('');
}

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
    // Control-parity extras ride the same neutral cell the web draws: unread
    // count as "(N new)", subtitle (a room's purpose) dimmed inline.
    const count = c.count ? ' ' + paint('cyan', `(${c.count} new)`) : '';
    const sub = c.subtitle ? ' ' + paint('gray', c.subtitle) : '';
    return `  ${dot} ${kindGlyph(kind)} ${name}${runtimeTag(runtime)}${sub}${count}`;
  };

  const messageLine = (msg: MessageRowVM): string => {
    const time = paint('gray', msg.time);
    const sender = paint('bold', msg.senderName);
    // Digest tier ([[perception-resolution-contract]]): the terminal renders the
    // SAME classification the web collapses on — head + mechanical tail line —
    // so no message floods a terminal frame either. Full body stays a handle
    // away (the projection carries `content` verbatim).
    if (msg.digest && !msg.expanded) {
      const head = msg.digest.head.split('\n').map((l) => `    ${l}`).join('\n');
      const hist = msg.digest.histogram ? ` · ${msg.digest.histogram}` : '';
      const tail = paint('gray', `    ${msg.digest.tailSummary}${hist}`);
      return `  ${time} ${kindGlyph(msg.kind)} ${sender}${runtimeTag(msg.runtime)}:\n${head}\n${tail}`;
    }
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
    /** One left-rail widget, by kind — full parity, the terminal's own idiom:
     *  block-ramp sparklines for the gauge, a wordmark line + ticker for the
     *  continuon, node rows for status. Unknown kinds show their frame (the
     *  anti-disappearance rule: a titled placeholder, never nothing). */
    widget(view: PanelWidget): string {
      if (view.kind === 'listing') return this.listing(view.body as ListingView);
      if (view.kind === 'gauge' || view.kind === 'system') {
        const body = view.body as SystemPanelView & GaugeView;
        const gauge: GaugeView | undefined =
          view.kind === 'gauge' ? (view.body as GaugeView) : (view.body as SystemPanelView).gauge;
        if (!gauge) return paint('dim', '  awaiting system feed…');
        const lines = gauge.series.map(
          (s) =>
            `  ${paint('cyan', s.label.padEnd(4))}${blockSpark(s.points)} ${paint('bold', s.current)}`,
        );
        const stats = (body as SystemPanelView).stats;
        if (stats) {
          lines.push(
            '  ' + stats.stats.map((st) => `${paint('bold', st.value)} ${paint('gray', st.label)}`).join('  '),
          );
        }
        return lines.join('\n');
      }
      if (view.kind === 'serving') {
        const s = view.body as ServingPanelView;
        const lines: string[] = [];
        if (s.header) {
          const h = s.header;
          lines.push(
            h.degradedReason
              ? `  ${paint('bold', '⚠')} ${paint('gray', h.degradedReason)}`
              : h.model
                ? `  ${paint('bold', h.model)} ${paint('gray', `${h.ready ? 'ready' : 'warming'} · ${h.lanes}×${h.contextWindow}`)}`
                : paint('dim', '  no model serving'),
          );
        }
        if (s.gauge) {
          for (const series of s.gauge.series) {
            lines.push(
              `  ${paint('cyan', series.label.padEnd(6))}${blockSpark(series.points)} ${paint('bold', series.current)}`,
            );
          }
        }
        for (const e of [...s.events].slice(-3)) {
          lines.push(`  ${paint('gray', `t${e.atToken}`)} ${paint('dim', e.detail)}`);
        }
        return lines.length > 0 ? lines.join('\n') : paint('dim', '  awaiting serving feed…');
      }
      if (view.kind === 'continuon') {
        const c = view.body as ContinuonView;
        const head = `  ${paint('green', '●')} ${paint('bold', paint('cyan', c.wordmark))}${c.version ? ' ' + paint('gray', c.version) : ''}`;
        const ticker = c.ticker.map((t) => `    ${paint('gray', t)}`);
        return [head, ...ticker].join('\n');
      }
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
