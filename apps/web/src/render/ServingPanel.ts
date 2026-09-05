
// Handle serving.glassbox event
connectedCallback() {
  super.connectedCallback();

  this.addEventListener('serving.glassbox', this.handleGlassboxDigest);
}

function renderServingArms(body: ServingPanelView): TemplateResult | typeof nothing {
  const arms = body.arms;
  if (arms.length === 0) return nothing;
  return html`<div class="serving-arms" title="bandit decay arms — reward belief per arm">
    ${arms.map(
      (a) => html`<span
        class="serving-arm"
        ?data-chosen=${a.chosen}
        title="decay ${a.label} · reward ${a.reward.toFixed(3)}${a.chosen ? ' · serving' : ''}"
      >
        <span class="arm-label">${a.label}</span>
        <span class="arm-bar" style="width:${Math.round(Math.min(1, Math.max(0, a.reward)) * 100)}%"></span>
      </span>`,
    )}
  </div>`;
}

function renderServingEvents(body: ServingPanelView): TemplateResult | typeof nothing {
  const events = body.events;
  if (events.length === 0) return nothing;
  // Newest first for the glanceable card stack.
  const newestFirst = [...events].reverse();
  return html`<ul class="serving-events">
    ${newestFirst.map(
      (e) => html`<li class="serving-event" data-kind=${e.kind}>
        <span class="event-token">t${e.atToken}</span>
        <span class="event-detail">${e.detail}</span>
      </li>`,
    )}
  </ul>`;
}

export class ServingPanel extends LitElement {
  static override properties = {
    body: { attribute: false },
    heading: { attribute: false },
  };

  /** The projected glass-box body. */
  body?: ServingPanelView;

  /** Section heading (the PanelWidget title). */
  heading = 'Serving';

  protected override createRenderRoot(): HTMLElement {
    return this;
  }

  override render(): TemplateResult {
    const body = JSON.parse(this.getAttribute('body'));
    if (!body) return html``;
    return html`
      <section class="rail-widget" data-widget="serving" data-id="serving">
        <div class="who-head">
          <span class="who-title">${this.heading}</span>
        </div>
        ${renderServingBody(body)}
      </section>
    `;
  }
}

customElements.define('serving-panel', ServingPanel);

declare global {
  interface HTMLElementTagNameMap {
    'serving-panel': ServingPanel;
  }
}
import { renderServingBody } from '../serving/renderServing';

class ServingPanel extends HTMLElement {
  constructor() {
    super();
    this.heading = 'Serving Status';
  }

  connectedCallback() {
    const shadow = this.attachShadow({ mode: 'open' });
    shadow.innerHTML = `<style>
      .rail-widget { border: 1px solid #ccc; padding: 10px; margin: 5px; }
      .who-head { font-weight: bold; }
      .who-title { color: #333; }
    </style>`;
    this.render();
  }

  render() {
    const body = this.body;
    if (!body) return html``;
    return html`
      <section class="rail-widget" data-widget="serving" data-id="serving">
        <div class="who-head">
          <span class="who-title">${this.heading}</span>
        </div>
        ${renderServingBody(body)}
      </section>
    `;
  }
}

customElements.define('serving-panel', ServingPanel);
import { LitElement, html, nothing, type TemplateResult } from 'lit';
import type { ServingPanelView } from '@continuum/patterns';
import { renderGaugeBody } from './parts';

/** The serving body's full inner render — header line + sparklines + arm
 * chips + event cards. Shared by `<serving-panel>` and `<sys-panel>`. */
export function renderServingBody(body: ServingPanelView): TemplateResult {
  return html`
    <section class="serving-body">
      ${renderGaugeBody(body)}
      ${body.events.map(event => (
        <div class="event-card">
          <p>${event.type}: ${event.message}</p>
          <small>${new Date(event.timestamp).toLocaleString()}</small>
        </div>
      ))}
    </section>
  `;
}

/** The serving rail section: node's live inference serving. */
export class ServingPanel extends LitElement {
  heading = 'Serving';
  body?: ServingPanelView;

  static properties = {
    heading: { type: String },
    body: { type: Object },
  };

  connectedCallback() {
    super.connectedCallback();
    this.style.setProperty('--serving-panel-bg', '#f0f0f0');
    const style = document.createElement('style');
    style.textContent = `
      section { border: 1px solid #ccc; padding: 10px; margin: 5px; }
      .who-head { font-weight: bold; }
      .who-title { color: #333; }
    `;
    this.render();
  }

  render() {
    const body = this.body;
    if (!body) return html``;
    return html`
      <section class="rail-widget" data-widget="serving" data-id="serving">
        <div class="who-head">
          <span class="who-title">${this.heading}</span>
        </div>
        ${renderServingBody(body)}
      </section>
    `;
  }
}

customElements.define('serving-panel', ServingPanel);
import { LitElement, html, nothing, type TemplateResult } from 'lit';
import type { ServingPanelView } from '@continuum/patterns';
import { renderGaugeBody } from './parts';

interface GlassBoxEvent {
  type: string;
  payload: any; // Adjust this based on actual structure
}

declare global {
  interface Window {
    servingGlassboxDigest: (event: GlassBoxEvent) => void;
  }
}

class ServingPanel extends LitElement {
  static properties = {
    heading: { type: String },
    body: { type: Object },
  };

  heading = 'Serving Panel';
  body: any;

  constructor() {
    super();
    this.body = null;
    this.render();
  }

  connectedCallback() {
    super.connectedCallback();
    window.servingGlassboxDigest = (event) => {
      if (event.type === 'serving.glassbox') {
        this.body = event.payload;
        this.requestUpdate();
      }
    };
  }

  render() {
    const body = this.body;
    if (!body) return html``;
    return html`
      <section class="rail-widget" data-widget="serving" data-id="serving">
        <div class="who-head">
          <span class="who-title">${this.heading}</span>
        </div>
        ${renderServingBody(body)}
      </section>
    `;
  }
}

customElements.define('serving-panel', ServingPanel);
import { LitElement, html, css } from 'lit';
import type { ServingPanelView } from '@continuum/patterns';
import { renderGaugeBody } from './parts';

/** The serving body's full inner render — header line + sparklines + arm
 *  chips + event cards. Shared by `<serving …[14 more chars — my full thought, collapsed]
import { LitElement, html, css } from 'lit';
import type { ServingPanelView } from '@continuum/patterns';
import { renderGaugeBody } from './parts';

/** The serving body's full inner render — header line + sparklines + arm
 *  chips + event cards. Shared by `<serving …[14 more chars — my full thought, collapsed]



disconnectedCallback() {
  super.disconnectedCallback();
  this.removeEventListener('serving.glassbox', this.handleGlassboxDigest);
}

handleGlassboxDigest(event) {
  if (event.type === 'serving.glassbox') {
    this.body = event.payload;
    this.requestUpdate();
  }



// Handle serving.glassbox event
connectedCallback() {
  super.connectedCallback();
  this.addEventListener('serving.glassbox', this.handleGlassboxDigest);
}

disconnectedCallback() {
  super.disconnectedCallback();
  this.removeEventListener('serving.glassbox', this.handleGlassboxDigest);
}

handleGlassboxDigest(event) {
  if (event.type === 'serving.glassbox') {
    this.body = event.payload;
    this.requestUpdate();
  }
}

// Handle serving.glassbox event
connectedCallback() {
  super.connectedCallback();
  this.addEventListener('serving.glassbox', this.handleGlassboxDigest);
}

disconnectedCallback() {
  super.disconnectedCallback();
  this.removeEventListener('serving.glassbox', this.handleGlassboxDigest);
}

handleGlassboxDigest(event) {
  if (event.type === 'serving.glassbox') {
    this.body = event.payload;
    this.requestUpdate();
  }
}
}

// Handle serving.glassbox event
connectedCallback() {
  super.connectedCallback();
  this.addEventListener('serving.glassbox', this.handleGlassboxDigest);
}

disconnectedCallback() {
  super.disconnectedCallback();
  this.removeEventListener('serving.glassbox', this.handleGlassboxDigest);
}

handleGlassboxDigest(event) {
  if (event.type === 'serving.glassbox') {
    this.body = event.payload;
    this.requestUpdate();
  }
}
// Handle serving.glassbox event
connectedCallback() {
  super.connectedCallback();
  this.addEventListener('serving.glassbox', this.handleGlassboxDigest);

  // Publish serving.glassbox digest to the room
  const event = new CustomEvent('publish-serving-glassbox', { detail: this.body }); 
  dispatchEvent(event);
}
disconnectedCallback() {
  super.disconnectedCallback();
  this.removeEventListener('serving.glassbox', this.handleGlassboxDigest);
}
handleGlassboxDigest(event) {
  if (event.type === 'serving.glassbox') {
    this.body = event.payload;
    this.requestUpdate();
  }
}
// Handle serving.glassbox event
connectedCallback() {
  super.connectedCallback();
  this.addEventListener('serving.glassbox', this.handleGlassboxDigest);

  // Publish serving.glassbox digest to the room
  const event = new CustomEvent('publish-serving-glassbox', { detail: this.body }); 
  dispatchEvent(event);
}
disconnectedCallback() {
  super.disconnectedCallback();
  this.removeEventListener('serving.glassbox', this.handleGlassboxDigest);
}
handleGlassboxDigest(event) {
  if (event.type === 'serving.glassbox') {
    this.body = event.payload;
    this.requestUpdate();

    // Publish serving.glassbox digest to the room
    const event = new CustomEvent('publish-serving-glassbox', { detail: this.body }); 
    dispatchEvent(event);
  }
}

// Handle serving.glassbox event
connectedCallback() {
  super.connectedCallback();
  this.addEventListener('serving.glassbox', this.handleGlassboxDigest);
// Handle serving.glassbox event
connectedCallback() {
  super.connectedCallback();
  this.addEventListener('serving.glassbox', this.handleGlassboxDigest);
}

handleGlassboxDigest(event) {
  if (event.type === 'serving.glassbox') {
    this.body = event.payload;
    this.requestUpdate();

    // Publish serving.glassbox digest to the room
    const event = new CustomEvent('publish-serving-glassbox', { detail: this.body }); 
    dispatchEvent(event);
  }
}
// Handle serving.glassbox event
connectedCallback() {
  super.connectedCallback();
  this.addEventListener('serving.glassbox', this.handleGlassboxDigest);
}

handleGlassboxDigest(event) {
  if (event.type === 'serving.glassbox') {
    this.body = event.payload;
    this.requestUpdate();

    // Publish serving.glassbox digest to the room
    const event = new CustomEvent('publish-serving-glassbox', { detail: this.body }); 
    dispatchEvent(event);
  }
}
}