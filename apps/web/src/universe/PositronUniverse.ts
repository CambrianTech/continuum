/**
 * `PositronUniverse` — the app renders its own nervous system.
 *
 * Etched circuit traces between the real panels; a light pulse travels a
 * trace when — and only when — a real event rides that pipe: cyan for a
 * message landing (rail → center), violet for an act receipt (center →
 * board), green for a settling verdict (board → rail: the flywheel,
 * drawn). Idle app = still circuit.
 *
 * Implements [`Universe`]/[`UniverseInstance`] — the host speaks the
 * interface, never this class's internals. Rendering is one static SVG;
 * pulses are CSS `offset-path` animations (compositor-only, no rAF),
 * capped, self-removing, stilled by `prefers-reduced-motion`.
 */

import type {
  Universe,
  UniverseFact,
  UniverseFactKind,
  UniverseFrame,
  UniverseInstance,
} from './Universe';

/** Etched geometry per pipe, in viewBox units (1000×600). One definition:
 *  path + entry-via position together, never parallel ternaries. */
const TRACES: Readonly<
  Record<UniverseFactKind, { readonly d: string; readonly via: { x: number; y: number } }>
> = {
  message: { d: 'M 40 120 H 220 V 300 H 480', via: { x: 40, y: 120 } },
  act: { d: 'M 500 560 V 380 H 760 V 240', via: { x: 500, y: 560 } },
  verdict: { d: 'M 960 200 H 700 V 80 H 300', via: { x: 960, y: 200 } },
};

const MAX_LIVE_PULSES = 8;
const SVG_NS = 'http://www.w3.org/2000/svg';

const STYLE = `
  .positron-root { position: absolute; inset: 0; overflow: hidden; pointer-events: none; z-index: 0; }
  .positron-root svg { width: 100%; height: 100%; display: block; }
  .positron-trace { fill: none; stroke: rgba(80,140,200,0.10); stroke-width: 1; }
  .positron-via { fill: rgba(80,140,200,0.16); }
  .positron-pulse { offset-rotate: 0deg; animation: positron-travel 1.4s cubic-bezier(0.3,0,0.7,1) forwards; }
  .positron-pulse[data-kind='message'] { fill: #35d0e0; }
  .positron-pulse[data-kind='act'] { fill: #b48cff; }
  .positron-pulse[data-kind='verdict'] { fill: #3fb950; }
  @keyframes positron-travel {
    from { offset-distance: 0%; opacity: 0; }
    12% { opacity: 1; }
    88% { opacity: 1; }
    to { offset-distance: 100%; opacity: 0; }
  }
  @media (prefers-reduced-motion: reduce) { .positron-pulse { display: none; } }
`;

class PositronInstance implements UniverseInstance {
  readonly element: HTMLElement;
  private readonly svg: SVGSVGElement;
  private livePulses = 0;

  constructor() {
    this.element = document.createElement('div');
    this.element.className = 'positron-root';
    const style = document.createElement('style');
    style.textContent = STYLE;
    this.element.appendChild(style);

    this.svg = document.createElementNS(SVG_NS, 'svg');
    this.svg.setAttribute('viewBox', '0 0 1000 600');
    this.svg.setAttribute('preserveAspectRatio', 'none');
    this.svg.setAttribute('aria-hidden', 'true');
    for (const { d, via } of Object.values(TRACES)) {
      const path = document.createElementNS(SVG_NS, 'path');
      path.setAttribute('class', 'positron-trace');
      path.setAttribute('d', d);
      this.svg.appendChild(path);
      const dot = document.createElementNS(SVG_NS, 'circle');
      dot.setAttribute('class', 'positron-via');
      dot.setAttribute('r', '2.5');
      dot.setAttribute('cx', String(via.x));
      dot.setAttribute('cy', String(via.y));
      this.svg.appendChild(dot);
    }
    this.element.appendChild(this.svg);
  }

  update(_frame: UniverseFrame): void {
    // The circuit is indifferent to ambience — it moves only on facts.
  }

  onFact(fact: UniverseFact): void {
    if (this.livePulses >= MAX_LIVE_PULSES) return; // paint budget
    const pulse = document.createElementNS(SVG_NS, 'circle');
    pulse.setAttribute('class', 'positron-pulse');
    pulse.setAttribute('data-kind', fact.kind);
    pulse.setAttribute('r', '3');
    pulse.style.offsetPath = `path('${TRACES[fact.kind].d}')`;
    pulse.addEventListener(
      'animationend',
      () => {
        pulse.remove();
        this.livePulses -= 1;
      },
      { once: true },
    );
    this.livePulses += 1;
    this.svg.appendChild(pulse);
  }

  dispose(): void {
    this.element.remove();
  }
}

export class PositronUniverse implements Universe {
  readonly key = 'positron';

  mount(): UniverseInstance {
    return new PositronInstance();
  }
}
