/**
 * WelcomeModalWidget — first-run introduction shown to a user whose
 * `UserEntity.hasOnboarded` is falsy. Two short panels:
 *
 *   1. Intro — what Continuum is, in one paragraph
 *   2. Hand-off — "Helper AI is in General, say hi"
 *
 * Wraps the generic ModalWidget. Fires `welcome-complete` when the user
 * advances past the final panel; the parent persists
 * `hasOnboarded=true` via `data/update`.
 *
 * Copy is intentionally short and revisable — see #1101 for the policy
 * (warm, brief, system-confident-not-salesy). Edit the strings below
 * directly; no separate i18n table yet.
 *
 * Introduced under #1101 PR-B. Depends on `widgets/shared/ModalWidget`
 * from PR-A.
 */

import { LitElement, html, css, type TemplateResult } from 'lit';
import '../shared/ModalWidget';

export class WelcomeModalWidget extends LitElement {
  static override properties = {
    open: { type: Boolean, reflect: true },
    step: { type: Number },
  } as const;

  open = false;
  step = 0;

  static override styles = css`
    :host {
      display: contents;
    }

    .panel {
      display: flex;
      flex-direction: column;
      gap: 12px;
    }

    .panel-title {
      font-size: 1.25em;
      font-weight: 600;
      margin: 0;
      line-height: 1.25;
    }

    .panel-body {
      font-size: 0.95em;
      line-height: 1.5;
      margin: 0;
      color: var(--text-secondary, rgba(255, 255, 255, 0.78));
    }

    .panel-body strong {
      color: var(--text-primary, #e0e0e0);
    }

    .step-indicator {
      display: flex;
      gap: 6px;
      margin-top: 8px;
    }

    .step-dot {
      width: 8px;
      height: 8px;
      border-radius: 50%;
      background: var(--border-subtle, rgba(255, 255, 255, 0.18));
    }

    .step-dot.active {
      background: var(--accent-color, #4a9eff);
    }

    button {
      padding: 8px 16px;
      border-radius: 6px;
      cursor: pointer;
      font-size: 0.95em;
      font-weight: 500;
      border: 0;
    }

    .btn-primary {
      background: var(--accent-color, #4a9eff);
      color: var(--button-text, #fff);
    }

    .btn-primary:hover {
      filter: brightness(1.08);
    }

    .btn-primary:focus-visible {
      outline: 2px solid var(--accent-color, #4a9eff);
      outline-offset: 2px;
    }

    .btn-secondary {
      background: transparent;
      color: var(--text-secondary, rgba(255, 255, 255, 0.7));
      border: 1px solid var(--border-subtle, rgba(255, 255, 255, 0.18));
    }

    .btn-secondary:hover {
      background: rgba(255, 255, 255, 0.05);
    }
  `;

  private readonly totalSteps = 2;

  private onNext(): void {
    if (this.step < this.totalSteps - 1) {
      this.step += 1;
    } else {
      this.complete();
    }
  }

  private onBack(): void {
    if (this.step > 0) this.step -= 1;
  }

  private complete(): void {
    this.open = false;
    this.dispatchEvent(new CustomEvent('welcome-complete', { bubbles: true, composed: true }));
  }

  /**
   * Modal-close fires when the user dismisses via Escape, backdrop, or
   * the X button. Treat that as "completed" too — the user has seen the
   * intro, no reason to nag them again on next session.
   */
  private onModalClose(): void {
    this.complete();
  }

  private renderStep(): TemplateResult {
    if (this.step === 0) {
      return html`
        <div class="panel">
          <h3 class="panel-title">Welcome to Continuum</h3>
          <p class="panel-body">
            Continuum is a shared workspace where you collaborate with humans
            and AI personas side-by-side — in chat rooms, on calls, on
            documents. The AIs here aren't tools you query; they're
            <strong>citizens</strong> of the workspace, with their own
            specialities, memory, and presence.
          </p>
          <p class="panel-body">
            Nothing to configure to get started — you already have a model
            running locally.
          </p>
        </div>
      `;
    }
    return html`
      <div class="panel">
        <h3 class="panel-title">Say hi to Helper AI</h3>
        <p class="panel-body">
          <strong>Helper AI</strong> is already in your <strong>General</strong> room.
          It runs locally on your machine — no API keys, no cloud round-trips.
          Send a message there to see the system in motion.
        </p>
        <p class="panel-body">
          When you want richer responses, head into Settings to plug in
          cloud providers like Anthropic, OpenAI, or others. Optional, never required.
        </p>
      </div>
    `;
  }

  private renderFooter(): TemplateResult {
    const isLast = this.step === this.totalSteps - 1;
    return html`
      <div class="step-indicator" aria-label="Welcome progress" role="presentation">
        ${Array.from({ length: this.totalSteps }, (_, i) => html`
          <span class="step-dot ${i === this.step ? 'active' : ''}"></span>
        `)}
      </div>
      <span style="flex: 1"></span>
      ${this.step > 0
        ? html`<button type="button" class="btn-secondary" @click=${() => this.onBack()}>Back</button>`
        : null}
      <button type="button" class="btn-primary" @click=${() => this.onNext()}>
        ${isLast ? 'Got it' : 'Next'}
      </button>
    `;
  }

  override render(): TemplateResult {
    return html`
      <modal-widget
        ?open=${this.open}
        modal-title="Get started"
        @modal-close=${() => this.onModalClose()}
      >
        ${this.renderStep()}
        <div slot="footer" style="display: flex; align-items: center; gap: 8px; width: 100%;">
          ${this.renderFooter()}
        </div>
      </modal-widget>
    `;
  }
}

customElements.define('welcome-modal', WelcomeModalWidget);

declare global {
  interface HTMLElementTagNameMap {
    'welcome-modal': WelcomeModalWidget;
  }
}
