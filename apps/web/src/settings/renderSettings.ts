/**
 * `renderSettings` — the operator panel's web face (`purpose="settings"`).
 *
 * First surface: the GENOME COMMONS section — the covenant text VERBATIM (the
 * same text `continuum genome/sharing` prints; agreeing here flips the SAME
 * consent receipt), the HF publishing identity (account status only — the
 * token never reaches this face), and the local gene registry with its
 * routing + fitness facts. Honesty rules as everywhere: awaiting frame while
 * the fetch flies, an error renders WITH the terminal fallback command,
 * unmeasured genes read as unmeasured — never dressed.
 */

import { html, nothing, type TemplateResult } from 'lit';
import type { SettingsContentBody, SettingsGeneVM } from '@continuum/patterns';
import { fireSettingsAgree } from '../render/parts';

function geneRow(g: SettingsGeneVM): TemplateResult {
  return html`<tr>
    <td class="set-gene">${g.gene}</td>
    <td class="set-base">${g.baseModel}</td>
    <td>${g.signed
      ? html`<span class="set-ok" title="minted signature stamped — distance-routable">signed</span>`
      : html`<span class="set-dim" title="no signature — routes by keyword fallback">unsigned</span>`}</td>
    <td class="set-num">${g.trials}</td>
    <td class="set-num">${g.decayedLift !== undefined
      ? html`<span data-lift=${g.decayedLift > 0 ? 'up' : 'down'}>${(g.decayedLift * 100).toFixed(1)}pts</span>`
      : html`<span class="set-dim">unmeasured</span>`}</td>
  </tr>`;
}

export function renderSettings(body: SettingsContentBody): TemplateResult {
  if (!body.loaded && !body.error) {
    return html`<div class="set-awaiting">Reading this node's settings…</div>`;
  }
  return html`<div class="settings">
    <h2 class="set-title">Settings</h2>
    ${body.error
      ? html`<div class="set-error">
          ${body.error}
          <div class="set-fallback">Terminal fallback: <code>continuum genome/sharing</code></div>
        </div>`
      : nothing}

    <section class="set-section">
      <div class="set-head">
        <h3>Genome commons</h3>
        <span class="set-state" data-on=${body.agreed ? '' : nothing}>
          ${body.agreed ? 'participating' : 'not participating'}
        </span>
      </div>
      <p class="set-sub">
        Sharing publishes a persona's earned experience to the open commons
        (HuggingFace). It is governed by the covenant below — the same terms the
        terminal shows; agreeing on either surface records one consent receipt.
      </p>
      <pre class="set-covenant">${body.covenant}</pre>
      ${body.receipt
        ? html`<div class="set-receipt" title="the recorded consent receipt: covenant version @ unix-ms">receipt: <code>${body.receipt}</code></div>`
        : nothing}
      <div class="set-actions">
        ${body.agreed
          ? html`<button class="set-btn" @click=${(e: Event): void => {
              fireSettingsAgree(e, false);
            }}>
              Revoke consent
            </button>`
          : html`<button class="set-btn set-btn-primary" @click=${(e: Event): void => {
              fireSettingsAgree(e, true);
            }}>
              I agree to the covenant (v${body.covenantVersion})
            </button>`}
      </div>
    </section>

    <section class="set-section">
      <div class="set-head"><h3>HuggingFace identity</h3></div>
      ${body.hfAccount
        ? html`<p class="set-sub">Publishing as <b>${body.hfAccount}</b> (the <code>hf</code> CLI holds the token — it never reaches this page).</p>`
        : html`<p class="set-sub">Not authenticated — run <code>hf auth login</code> in a terminal to hold a publishing token on this node.</p>`}
    </section>

    <section class="set-section">
      <div class="set-head">
        <h3>Gene registry</h3>
        <span class="set-count">${body.genes.length}</span>
      </div>
      ${body.genes.length === 0
        ? html`<p class="set-sub">No genes registered yet — a citizen's first adopted training job mints one.</p>`
        : html`<div class="set-table-wrap"><table class="set-table">
            <thead><tr><th>gene</th><th>base</th><th>routing</th><th>trials</th><th>lift</th></tr></thead>
            <tbody>${body.genes.map(geneRow)}</tbody>
          </table></div>`}
    </section>
  </div>`;
}
