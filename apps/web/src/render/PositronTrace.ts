/**
 * `<positron-trace>` — the POSITRON universe: the app renders its own
 * nervous system. Thin circuit traces are etched between the real panels
 * (rail → center → rail), and when a REAL event flows — a message lands, an
 * act radiates, a verdict settles — a light pulse travels the trace that
 * pipe represents. The glass-box made ambient: every mote of motion IS a
 * fact ([[universe-is-an-experience-not-a-theme]], and the 2026-08-31
 * anti-gimmick rule: spectacle tied to truth, on the paint budget).
 *
 * PAINT BUDGET (the cosmos post-mortem, same night): NO rAF loop. The
 * etched traces are one static SVG. Pulses are tiny circles animated with
 * CSS `offset-path` — compositor-only motion — created on demand, capped,
 * and removed on animationend. An idle app is a still circuit; a busy app
 * glitters exactly as much as it works. `prefers-reduced-motion` stills it.
 */

import { LitElement, css, html, nothing, svg, type TemplateResult } from 'lit';

/** Which pipe a pulse rides — each has its own etched path + hue. */
export type TraceKind = 'message' | 'act' | 'verdict';

const MAX_LIVE_PULSES = 8;

export class PositronTrace extends LitElement {
  static override styles = css`
    :host {
      position: absolute;
      inset: 0;
      z-index: 0;
      overflow: hidden;
      pointer-events: none;
    }
    svg {
      width: 100%;
      height: 100%;
      display: block;
    }
    .trace {
      fill: none;
      stroke: rgba(80, 140, 200, 0.1);
      stroke-width: 1;
    }
    .via {
      fill: rgba(80, 140, 200, 0.16);
    }
    .pulse {
      offset-rotate: 0deg;
      animation: travel 1.4s cubic-bezier(0.3, 0, 0.7, 1) forwards;
    }
    .pulse[data-kind='message'] {
      fill: #35d0e0;
      filter: drop-shadow(0 0 3px rgba(53, 208, 224, 0.8));
    }
    .pulse[data-kind='act'] {
      fill: #b48cff;
      filter: drop-shadow(0 0 3px rgba(180, 140, 255, 0.8));
    }
    .pulse[data-kind='verdict'] {
      fill: #3fb950;
      filter: drop-shadow(0 0 4px rgba(63, 185, 80, 0.9));
    }
    @keyframes travel {
      from {
        offset-distance: 0%;
        opacity: 0;
      }
      12% {
        opacity: 1;
      }
      88% {
        opacity: 1;
      }
      to {
        offset-distance: 100%;
        opacity: 0;
      }
    }
    @media (prefers-reduced-motion: reduce) {
      .pulse {
        display: none;
      }
    }
  `;

  private _pulses: { id: number; kind: TraceKind; path: string }[] = [];
  private _nextId = 0;

  /** Fire one pulse along `kind`'s trace. Called by the host on REAL events
   *  only — a pulse with no fact behind it is forbidden. */
  pulse(kind: TraceKind): void {
    if (this._pulses.length >= MAX_LIVE_PULSES) return; // budget, silently
    const path = this._pathFor(kind);
    this._pulses = [...this._pulses, { id: this._nextId++, kind, path }];
    this.requestUpdate();
  }

  /** The etched geometry, in viewBox units (1000×600, scales with the box).
   *  Three pipes, three roads: messages run rail→center, acts run
   *  center→board, verdicts run board→rail (the flywheel, drawn). */
  private _pathFor(kind: TraceKind): string {
    switch (kind) {
      case 'message':
        return 'M 40 120 H 220 V 300 H 480';
      case 'act':
        return 'M 500 560 V 380 H 760 V 240';
      case 'verdict':
        return 'M 960 200 H 700 V 80 H 300';
    }
  }

  private _onPulseEnd(id: number): void {
    this._pulses = this._pulses.filter((p) => p.id !== id);
    this.requestUpdate();
  }

  override render(): TemplateResult {
    const kinds: TraceKind[] = ['message', 'act', 'verdict'];
    return html`<svg viewBox="0 0 1000 600" preserveAspectRatio="none" aria-hidden="true">
      ${kinds.map(
        (k) => svg`<path class="trace" d=${this._pathFor(k)} />
          <circle class="via" r="2.5" cx=${k === 'message' ? 40 : k === 'act' ? 500 : 960} cy=${k === 'message' ? 120 : k === 'act' ? 560 : 200} />`,
      )}
      ${this._pulses.map(
        (p) => svg`<circle
          class="pulse"
          data-kind=${p.kind}
          r="3"
          style="offset-path: path('${p.path}')"
          @animationend=${(): void => this._onPulseEnd(p.id)}
        />`,
      )}
      ${this._pulses.length === 0 ? nothing : nothing}
    </svg>`;
  }
}

customElements.define('positron-trace', PositronTrace);

declare global {
  interface HTMLElementTagNameMap {
    'positron-trace': PositronTrace;
  }
}
