/**
 * `<sys-panel>` — the two-faced System rail section (the old sidebar's SYS|AI
 * header): one panel that toggles between the node's resource gauge (SYS) and
 * the live team-cognition stats (AI), with the honest time-window chip derived
 * from the gauge's own data (samples × cadence — never a hardcoded "1h").
 *
 * Which face shows is renderer state (a lens the reader flips), exactly like
 * `<rooms-panel>`'s facet — it belongs here, never in the projection. Light DOM
 * so the element inherits `<chat-widget>`'s shadow stylesheet.
 *
 * A missing gauge (the metrics feed hasn't delivered) disables the SYS chip and
 * shows the AI face — less data honestly shown, never an empty fabricated graph.
 */

import { LitElement, html, type TemplateResult } from 'lit';
import type { GaugeView, SystemPanelView } from '@continuum/patterns';
import { renderGaugeBody, renderMetricsRow } from './parts';

/** The gauge's honest window label, derived from its own data: longest series
 *  length × sample cadence. `undefined` when either is absent/zero — no chip
 *  drawn, never an invented span. Pure for unit pinning. */
export function gaugeWindowLabel(view: GaugeView | undefined): string | undefined {
  if (!view || view.series.length === 0) return undefined;
  const samples = Math.max(0, ...view.series.map((s) => s.points.length));
  const ms = (view.sampleIntervalMs ?? 0) * samples;
  if (ms <= 0) return undefined;
  const mins = ms / 60_000;
  if (mins < 1) return `${Math.round(ms / 1000)}s`;
  if (mins < 60) return `${Math.round(mins)}m`;
  return `${+(mins / 60).toFixed(1)}h`;
}

type Face = 'sys' | 'ai';

export class SysPanel extends LitElement {
  static override properties = {
    body: { attribute: false },
    heading: { attribute: false },
    _face: { state: true },
  };

  /** The projected two-face body ({ gauge?, stats }). */
  body?: SystemPanelView;

  /** Section heading (the PanelWidget title). */
  heading = 'System';

  private _face: Face = 'sys';

  protected override createRenderRoot(): HTMLElement {
    return this;
  }

  override render(): TemplateResult {
    const body = this.body;
    if (!body) return html``;
    const hasGauge = body.gauge !== undefined;
    // ANTI-DISAPPEARANCE rule (Joel: "stuff disappearing is alarming"): a
    // missing feed renders as VISIBLE absence — the SYS face stays selectable
    // and shows an awaiting-feed placeholder — never as a vanished face. The
    // frame is the promise; the data fills it when it arrives.
    const face: Face = this._face;
    const window = gaugeWindowLabel(body.gauge);
    const chip = (id: Face, label: string, enabled: boolean): TemplateResult => html`<button
      class="face-chip"
      role="tab"
      aria-selected=${face === id ? 'true' : 'false'}
      ?data-active=${face === id}
      ?disabled=${!enabled}
      title=${enabled ? label : 'no resource feed yet'}
      @click=${(): void => {
        this._face = id;
      }}
    >
      ${label}
    </button>`;
    return html`
      <section class="rail-widget" data-widget="system" data-id="system">
        <div class="who-head">
          <span class="who-title">${this.heading}</span>
          <span class="face-chips" role="tablist" aria-label="system panel face">
            ${chip('sys', 'SYS', hasGauge)} ${chip('ai', 'AI', true)}
          </span>
          ${window
            ? html`<span class="gauge-window" title="window span — samples × cadence, from the data"
                >${window}</span
              >`
            : html``}
        </div>
        ${face === 'sys'
          ? body.gauge
            ? renderGaugeBody(body.gauge)
            : html`<div class="gauge-awaiting" title="the system-metrics feed has not delivered yet">
                awaiting system feed…
              </div>`
          : renderMetricsRow(body.stats)}
      </section>
    `;
  }
}

customElements.define('sys-panel', SysPanel);

declare global {
  interface HTMLElementTagNameMap {
    'sys-panel': SysPanel;
  }
}
