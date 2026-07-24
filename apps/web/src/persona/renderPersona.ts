/**
 * `renderPersona` — the web renderer for the persona HOME (`purpose="persona"`).
 *
 * The profile center + Cognitive System View from the reference designs
 * (docs/images/persona-profile.png, persona-brain-hud.png), drawn as a
 * purpose-registered `Content` renderer — the SAME registry dispatch as
 * chat/foundry, reached by the SAME nav semantics (a persona-kind tab). Pure
 * fragments: everything here is a field→element map of the already-projected
 * `PersonaContentBody`; region drill-down is a native `<details>` in-content
 * expansion (DOM state, no widget state, no route).
 *
 * Honesty rules ([[fallbacks-are-illegal-fail-loud]]): every section always
 * renders its FRAME — awaiting/empty states for absent feeds (LIMBIC until an
 * affect axis radiates, WRITINGS until the blog exists, claims until the board
 * feed delivers) — and never a fabricated number. The brain regions glow from
 * the SAME live `persona:vitals` pulse the roster compass draws.
 */

import { html, nothing, svg, type TemplateResult } from 'lit';
import type {
  PersonaBrainRegionVM,
  PersonaClaimVM,
  PersonaContentBody,
  PersonaPathwayVM,
} from '@continuum/patterns';
import { agoLabel, cognitionDiamond, loadoutStrip } from '../render/parts';

/** The low-poly brain mark at the HUD's center — a wireframe polygon cloud
 *  (the reference's mesh brain), pure SVG, glowing via CSS. Decorative
 *  (aria-hidden); the DATA lives in the region cards around it. */
function brainMark(): TemplateResult {
  // Hand-placed low-poly silhouette: outline + interior facets.
  const outline =
    '14,58 22,38 34,24 52,14 74,10 96,12 112,20 122,32 126,46 122,60 112,72 96,82 76,86 66,84 58,88 50,84 34,80 22,72';
  const facets = [
    '34,24 52,14 54,34',
    '54,34 52,14 74,10',
    '74,10 96,12 78,32',
    '54,34 74,10 78,32',
    '96,12 112,20 100,40',
    '78,32 96,12 100,40',
    '112,20 122,32 126,46 100,40',
    '34,24 54,34 40,52',
    '22,38 34,24 40,52',
    '14,58 22,38 40,52 30,66',
    '40,52 54,34 62,56',
    '54,34 78,32 62,56',
    '62,56 78,32 84,58',
    '78,32 100,40 84,58',
    '84,58 100,40 122,60 104,70',
    '100,40 126,46 122,60',
    '30,66 40,52 62,56 48,74',
    '22,72 14,58 30,66',
    '48,74 62,56 84,58 66,78',
    '34,80 22,72 30,66 48,74',
    '66,78 84,58 104,70 88,80',
    '50,84 34,80 48,74 58,82',
    '66,78 88,80 76,86',
    '96,82 88,80 104,70 112,72',
  ];
  return html`<svg class="brain-mark" viewBox="0 0 140 96" aria-hidden="true">
    ${facets.map((pts) => svg`<polygon class="brain-facet" points=${pts} />`)}
    ${svg`<polygon class="brain-outline" points=${outline} />`}
  </svg>`;
}

/** One brain-region card — label, [ role ], live level (bar + status word),
 *  drill-down facts inside a native <details>. Absent level = the honest
 *  AWAITING frame (dashed, dim), never a fabricated 0-bar. */
function regionCard(r: PersonaBrainRegionVM): TemplateResult {
  const live = r.level !== undefined;
  return html`<details
    class="region"
    data-region=${r.id}
    data-faculty=${r.facultyKey || 'none'}
    data-live=${live ? '' : nothing}
    style=${live ? `--region-level:${r.level}` : nothing}
  >
    <summary class="region-face">
      <span class="region-name">${r.label}</span>
      <span class="region-role">[ ${r.role} ]</span>
      ${live
        ? html`<span class="region-track"
              ><span class="region-fill" style="width:${r.level}%"></span></span
            ><span class="region-status">${r.status} · ${r.level}</span>`
        : html`<span class="region-status awaiting">awaiting signal</span>`}
    </summary>
    <div class="region-detail">
      <div class="region-detail-row">
        <span class="rd-label">status</span>
        <span class="rd-val">${r.status.toLowerCase()}</span>
      </div>
      ${live
        ? html`<div class="region-detail-row">
            <span class="rd-label">level</span>
            <span class="rd-val">${r.level} / 100</span>
          </div>`
        : html`<div class="region-detail-row">
            <span class="rd-label">signal</span>
            <span class="rd-val">no live axis radiates for this region yet</span>
          </div>`}
      ${r.detail.map(
        (f) => html`<div class="region-detail-row">
          <span class="rd-label">${f.label.toLowerCase()}</span>
          <span class="rd-val">${f.value}</span>
        </div>`,
      )}
      <div class="region-actions">
        <button class="p-btn" disabled title="coming soon — cognition log view isn't wired yet">
          View log
        </button>
        <button class="p-btn" disabled title="coming soon — state inspection isn't wired yet">
          Inspect state
        </button>
      </div>
    </div>
  </details>`;
}

/** One PATHWAYS tile — an enabled tile is a real in-content anchor; a disabled
 *  one is honestly "coming soon" (the destination isn't an activity yet). */
function pathwayTile(p: PersonaPathwayVM): TemplateResult {
  if (!p.enabled) {
    return html`<span class="pathway" data-disabled title="coming soon — this pathway isn't an activity yet">
      <span class="pathway-glyph">${p.glyph}</span>
      <span class="pathway-label">${p.label}</span>
      <span class="pathway-sub">${p.sublabel}</span>
    </span>`;
  }
  return html`<a class="pathway" href=${p.target} title="Open ${p.label}">
    <span class="pathway-glyph">${p.glyph}</span>
    <span class="pathway-label">${p.label}</span>
    <span class="pathway-sub">${p.sublabel}</span>
  </a>`;
}

/** One claims-feed row — a work-board card this persona owns. */
function claimRow(c: PersonaClaimVM): TemplateResult {
  const ago = agoLabel(c.updatedAtMs);
  return html`<li class="claim" data-state=${c.state}>
    <span class="claim-state">${c.state.replace('_', ' ')}</span>
    <span class="claim-title">${c.title}</span>
    <span class="claim-meta">
      <span class="claim-priority">${c.priority}</span>
      ${ago ? html`<span class="claim-ago">${ago}</span>` : nothing}
    </span>
  </li>`;
}

/** Big hero avatar — stored image over the kind glyph (honest fallback). */
function heroAvatar(body: PersonaContentBody): TemplateResult {
  const hide = (e: Event): void => {
    (e.currentTarget as HTMLElement).remove();
  };
  return html`<span class="p-avatar" data-online=${body.online ? '' : nothing}>
    <span class="p-avatar-glyph">${body.kind === 'human' ? '🧑' : '🤖'}</span>
    ${body.avatarUrl
      ? html`<img class="p-avatar-img" src=${body.avatarUrl} alt="" @error=${hide} />`
      : nothing}
    <span class="p-presence-dot"></span>
  </span>`;
}

/** The persona HOME surface. Every section renders a frame — awaiting/empty
 *  states, never a vanished section (the anti-disappearance rule). */
export function renderPersona(body: PersonaContentBody): TemplateResult {
  const ago = agoLabel(body.lastSeenMs);
  const hasVitals = Object.keys(body.vitals).length > 0;
  return html`<div class="persona-home" data-persona=${body.personaId}>
    ${body.awaitingIdentity
      ? html`<div class="p-awaiting-banner">
          Awaiting this citizen's presence in the live roster — showing what the substrate carries.
        </div>`
      : nothing}

    <section class="p-hero">
      ${heroAvatar(body)}
      <div class="p-id">
        <h1 class="p-name">${body.name || 'unresolved citizen'}</h1>
        <div class="p-handle">
          ${body.handle ? html`<span>@${body.handle}</span>` : nothing}
          <span class="p-dot">·</span>
          <span>${body.kind}</span>
          <span class="p-dot">·</span>
          <span class="p-online" data-on=${body.online ? '' : nothing}
            >${body.online ? 'online' : 'offline'}</span
          >
        </div>
        <div class="p-chips">
          ${body.loadout ? html`<span class="p-chip p-chip-model">${loadoutStrip(body.loadout)}</span>` : nothing}
          <span class="p-chip p-chip-kind">${body.kind === 'agent' ? 'persona' : body.kind}</span>
          ${body.runtime ? html`<span class="p-chip">${body.runtime}</span>` : nothing}
        </div>
        <div class="p-actions">
          <button class="p-btn" disabled title="coming soon — the DM room isn't wired yet">
            💬 Message
          </button>
          <button class="p-btn" disabled title="coming soon — live calls aren't wired yet">
            📹 Video Call
          </button>
        </div>
      </div>
    </section>

    <section class="p-card p-about">
      <div class="p-card-head">about</div>
      <div class="p-facts">
        ${ago
          ? html`<span class="p-fact"><span class="p-fact-label">last active</span><span class="p-fact-val">${ago}</span></span>`
          : nothing}
        ${body.runtime
          ? html`<span class="p-fact"><span class="p-fact-label">runtime</span><span class="p-fact-val">${body.runtime}</span></span>`
          : nothing}
        ${body.loadout?.model
          ? html`<span class="p-fact"><span class="p-fact-label">model</span><span class="p-fact-val">${body.loadout.model}</span></span>`
          : nothing}
        ${!ago && !body.runtime && !body.loadout?.model
          ? html`<span class="p-empty">Nothing resolved yet — facts appear as presence and vitals radiate.</span>`
          : nothing}
      </div>
    </section>

    <section class="p-card p-brain" id="brain">
      <div class="p-card-head">
        cognitive system view
        ${hasVitals
          ? html`<span class="p-live-chip" data-on>live</span>`
          : html`<span class="p-live-chip" title="no vitals radiating yet">awaiting vitals</span>`}
        ${hasVitals ? cognitionDiamond(body.vitals) : nothing}
      </div>
      <div class="brain-grid">
        <div class="brain-col">${body.regions.slice(0, 2).map(regionCard)}</div>
        <div class="brain-center">${brainMark()}</div>
        <div class="brain-col">${body.regions.slice(2, 4).map(regionCard)}</div>
        <div class="brain-wide">${body.regions.slice(4).map(regionCard)}</div>
      </div>
      <div class="brain-stats">
        ${Object.keys(body.vitals).length === 0
          ? html`<span class="p-empty">No vitals radiating yet — the pulse lights this panel when the persona thinks.</span>`
          : html`
              ${body.vitals.activity !== undefined
                ? html`<span class="b-stat">⚡ activity ${Math.round(body.vitals.activity)}</span>`
                : nothing}
              ${body.vitals.queue !== undefined
                ? html`<span class="b-stat">📥 queue ${Math.round(body.vitals.queue)}</span>`
                : nothing}
              <span class="b-stat">🧬 ${body.genes.length} gene${body.genes.length === 1 ? '' : 's'}</span>
            `}
      </div>
    </section>

    <section class="p-card p-pathways">
      <div class="p-card-head">pathways</div>
      <div class="pathway-grid">${body.pathways.map(pathwayTile)}</div>
    </section>

    <section class="p-card p-genome" id="genome">
      <div class="p-card-head">genome shelf</div>
      ${body.genes.length > 0
        ? html`<div class="gene-shelf">
            ${body.genes.map(
              (g, i) => html`<span class="gene-chip" title="paged-in gene · slot ${i + 1}">
                <span class="gene-slot-dot"></span>${g}
              </span>`,
            )}
          </div>`
        : html`<div class="p-empty">No genes paged in — the base model is running bare.</div>`}
    </section>

    <section class="p-card p-claims">
      <div class="p-card-head">work board claims</div>
      ${!body.claimsLive
        ? html`<div class="p-empty">Awaiting the work-board feed…</div>`
        : body.claims.length === 0
          ? html`<div class="p-empty">No claims on the board.</div>`
          : html`<ul class="claims">
              ${body.claims.map(claimRow)}
            </ul>`}
    </section>

    <section class="p-card p-writings">
      <div class="p-card-head">writings</div>
      ${body.writings.length === 0
        ? html`<div class="p-empty">No published writings yet.</div>`
        : nothing}
    </section>
  </div>`;
}
