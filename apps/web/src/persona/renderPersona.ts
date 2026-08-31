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

import { html, nothing, type TemplateResult } from 'lit';
import type {
  PersonaBrainRegionVM,
  PersonaClaimVM,
  PersonaContentBody,
  PersonaPathwayVM,
} from '@continuum/patterns';
import { agoLabel, cognitionDiamond, loadoutStrip } from '../render/parts';
// The REAL mesh brain — recovered from the original HUD (docs/images/
// persona-brain-hud.png crop): LLM-drawn SVG anatomy is banned; the 3D-rendered
// replacement is card eb69cfb0. Vite inlines/serves the asset.
import brainMeshUrl from './brain-mesh.png';

/** The mesh brain at the HUD's center — the ORIGINAL asset, recovered from the
 *  old HUD screenshot (hand-drawing anatomy in SVG is banned; the live
 *  3D-rendered brain is card eb69cfb0). Decorative — the DATA lives in the
 *  region cards around it. */
function brainMark(): TemplateResult {
  return html`<img class="brain-mark" src=${brainMeshUrl} alt="" aria-hidden="true" />`;
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
  return html`<li class="claim" data-state=${c.state} data-lapsed=${c.holdLapsed ? '' : nothing}>
    <span class="claim-state">${c.state.replace('_', ' ')}</span>
    <span class="claim-title">${c.title}</span>
    <span class="claim-meta">
      <span class="claim-priority">${c.priority}</span>
      ${c.holdLapsed
        ? html`<span class="claim-lapsed" title="the claim's lease expired — the card is takeable; ask this persona before taking it">lease lapsed</span>`
        : nothing}
      ${c.prUrl
        ? html`<a class="claim-pr" href=${c.prUrl} target="_blank" rel="noopener" title="the card's landing PR">PR</a>`
        : nothing}
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

/** The HOME INTERIOR — an orthographic dollhouse card, furnished entirely by
 *  live facts (CITIZEN-HOMES-ORTHOGRAPHIC.md's mapping table, first cut):
 *  window light = online; desk items = active runs; trophy shelf = resolved
 *  verdicts; the plant grows with her genome. Default geometry until home
 *  recipes carry authored layouts — but every object is TRUE. */
function homeInterior(body: PersonaContentBody): TemplateResult {
  const runs = body.runs ?? [];
  const live = runs.filter((r) => r.state === 'working' || r.state === 'grading' || r.state === 'queued').length;
  const wins = runs.filter((r) => r.state === 'resolved').length;
  const genes = body.genes.length;
  const lit = body.online;
  const deskItems = Math.min(live, 4);
  const trophies = Math.min(wins, 6);
  const leaves = Math.min(1 + genes, 5);
  return html`<svg class="home-iso" viewBox="0 0 220 120" role="img" aria-label="home interior — furnished by live state">
    <!-- floor + walls: a fixed iso shell (default geometry) -->
    <polygon class="hi-floor" points="20,80 110,110 200,80 110,50" />
    <polygon class="hi-wall-l" points="20,80 20,30 110,0 110,50" />
    <polygon class="hi-wall-r" points="200,80 200,30 110,0 110,50" />
    <!-- window: LIT when she is online -->
    <rect class="hi-window ${lit ? 'hi-lit' : ''}" x="45" y="26" width="26" height="20" transform="skewY(-18)" />
    <!-- desk (office region) + one glowing item per ACTIVE RUN -->
    <polygon class="hi-desk" points="120,78 160,90 185,80 145,68" />
    ${Array.from({ length: deskItems }, (_, i) => {
      const x = 132 + i * 13;
      const y = 76 + i * 2.5;
      return html`<rect class="hi-run" x=${x} y=${y - 6} width="6" height="6" transform="skewX(-8)" />`;
    })}
    <!-- trophy shelf on the right wall: one cup per RESOLVED verdict -->
    <line class="hi-shelf" x1="150" y1="38" x2="192" y2="52" />
    ${Array.from({ length: trophies }, (_, i) => {
      const x = 152 + i * 7;
      const y = 33 + i * 2.3;
      return html`<circle class="hi-trophy" cx=${x} cy=${y} r="2.2" />`;
    })}
    <!-- the genome plant, left corner: a leaf per paged-in gene (+1 stem) -->
    <line class="hi-stem" x1="42" y1="86" x2="42" y2=${86 - leaves * 6} />
    ${Array.from({ length: leaves }, (_, i) => {
      const y = 82 - i * 6;
      const dx = i % 2 === 0 ? -6 : 6;
      return html`<ellipse class="hi-leaf" cx=${42 + dx / 2} cy=${y} rx="4" ry="2" transform="rotate(${dx * 4} ${42 + dx / 2} ${y})" />`;
    })}
    <!-- her presence: the avatar dot stands in the room when online -->
    ${lit ? html`<circle class="hi-self" cx="95" cy="84" r="4" />` : nothing}
  </svg>
  <div class="hi-legend">
    <span>window ${lit ? 'lit — home' : 'dark — away'}</span>
    <span>desk · ${live} active</span>
    <span>shelf · ${wins} trophies</span>
    <span>plant · ${genes} genes</span>
  </div>`;
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

    <section class="p-card p-home">
      <div class="p-card-head">home <span class="p-home-hint">interior · procedural until her home recipe lands</span></div>
      ${homeInterior(body)}
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

    <section class="p-card p-record">
      <div class="p-card-head">record &amp; awards</div>
      ${body.runs === undefined
        ? html`<div class="p-empty">Awaiting the benchmark feed…</div>`
        : (() => {
            const settled = body.runs.filter((r) => r.state === 'resolved' || r.state === 'failed');
            const wins = settled.filter((r) => r.state === 'resolved').length;
            if (settled.length === 0)
              return html`<div class="p-empty">No settled runs yet — the record starts with the first verdict.</div>`;
            return html`<div class="p-trophies">
                <span class="p-trophy p-trophy-wins" title="benchmark instances resolved">🏅 ${wins} resolved</span>
                <span class="p-trophy" title="settled runs (resolved + failed)">${settled.length} settled</span>
                <span class="p-trophy" title="resolve rate over settled runs">${Math.round((wins / settled.length) * 100)}%</span>
              </div>
              <ul class="p-runs">
                ${settled.slice(0, 8).map(
                  (r) => html`<li class="p-run bench-state-${r.state}">
                    <span class="p-run-instance">${r.instance}</span>
                    <span class="p-run-state">${r.state === 'resolved' ? '✓ resolved' : '✗ failed'}</span>
                    ${r.verdict
                      ? html`<span class="p-run-pulse">f2p ${r.verdict.f2pPassed}/${r.verdict.f2pTotal}</span>`
                      : nothing}
                  </li>`,
                )}
              </ul>`;
          })()}
    </section>

    <section class="p-card p-work">
      <div class="p-card-head">active work</div>
      ${body.runs === undefined
        ? html`<div class="p-empty">Awaiting the benchmark feed…</div>`
        : body.runs.length === 0
          ? html`<div class="p-empty">No benchmark runs on the board for ${body.name || 'this citizen'}.</div>`
          : html`<ul class="p-runs">
              ${body.runs.map(
                (r) => html`<li
                  class="p-run bench-state-${r.state}"
                  ?data-door=${r.roomId !== undefined}
                  title=${r.roomId !== undefined ? 'open this run\'s room' : r.runId}
                  @click=${r.roomId !== undefined
                    ? (e: Event): void => {
                        (e.currentTarget as HTMLElement).dispatchEvent(
                          new CustomEvent('bench-run-open', {
                            detail: { roomId: r.roomId, roomName: r.roomName },
                            bubbles: true,
                            composed: true,
                          }),
                        );
                      }
                    : nothing}
                >
                  <span class="p-run-instance">${r.instance}</span>
                  <span class="p-run-state">${r.state}</span>
                  ${r.lastGenAgeS === null
                    ? html`<span class="p-run-pulse">no generations yet</span>`
                    : html`<span class="p-run-pulse">${r.generations} gens</span>`}
                  ${r.verdict?.resolved ? html`<span class="bench-resolved">✓</span>` : nothing}
                </li>`,
              )}
            </ul>`}
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
