/**
 * EmptyStateWidget — generic "no items yet" panel.
 *
 * Drop into any list or content area that can be empty (no messages,
 * no rooms, no personas). The user sees an icon, a title, an optional
 * subtitle, and an optional action button instead of an unexplained
 * blank surface.
 *
 * Properties:
 *   - icon: string — emoji or single character (decorative, aria-hidden)
 *   - emptyTitle: string — heading text
 *   - subtitle: string — explanatory text under the heading (optional)
 *   - actionLabel: string — text on the call-to-action button. If empty,
 *     no button is rendered.
 *
 * Events:
 *   - empty-state-action: fired when the action button is clicked
 *
 * Slots:
 *   - default: extra content rendered below the subtitle
 *
 * Introduced under #1101 (first-run UX) as part of PR-A.
 */

import { LitElement, html, css, type TemplateResult } from 'lit';

export class EmptyStateWidget extends LitElement {
  static override properties = {
    icon: { type: String },
    emptyTitle: { type: String, attribute: 'empty-title' },
    subtitle: { type: String },
    actionLabel: { type: String, attribute: 'action-label' },
  } as const;

  icon = '';
  emptyTitle = '';
  subtitle = '';
  actionLabel = '';

  static override styles = css`
    :host {
      display: flex;
      flex-direction: column;
      align-items: center;
      justify-content: center;
      gap: 8px;
      padding: 32px 24px;
      text-align: center;
      color: var(--text-muted, rgba(255, 255, 255, 0.55));
      min-height: 200px;
    }

    /* The HTML \`hidden\` attribute applies \`display: none\` via the
     * user-agent stylesheet — but the \`:host { display: flex }\` above is
     * a more-specific author rule that wins, so \`hidden\` would have no
     * visual effect by default on a custom element with an explicit
     * \`:host { display: ... }\`.
     *
     * Caller pattern (e.g., ChatWidget.updateEntityCount) toggles the
     * \`hidden\` attribute to show/hide the empty state. Without this
     * rule the toggle silently no-ops and the "Send your first message"
     * panel keeps rendering even when there ARE messages — the
     * Joel-reported bug where the placeholder never cleared after a
     * room loaded with prior history. The HTML5 spec specifically
     * calls this out for custom elements with explicit display:
     * https://html.spec.whatwg.org/multipage/interaction.html#the-hidden-attribute
     */
    :host([hidden]) {
      display: none;
    }

    .empty-icon {
      font-size: 2.5em;
      line-height: 1;
      opacity: 0.7;
    }

    .empty-title {
      font-size: 1.1em;
      font-weight: 600;
      margin: 0;
      color: var(--text-primary, #e0e0e0);
    }

    .empty-subtitle {
      font-size: 0.92em;
      max-width: 42ch;
      margin: 0;
      line-height: 1.45;
    }

    .empty-action {
      margin-top: 8px;
      padding: 8px 16px;
      background: var(--accent-color, #4a9eff);
      color: var(--button-text, #fff);
      border: 0;
      border-radius: 6px;
      cursor: pointer;
      font-size: 0.95em;
      font-weight: 500;
    }

    .empty-action:hover {
      filter: brightness(1.08);
    }

    .empty-action:focus-visible {
      outline: 2px solid var(--accent-color, #4a9eff);
      outline-offset: 2px;
    }
  `;

  private onActionClick(): void {
    this.dispatchEvent(new CustomEvent('empty-state-action', { bubbles: true, composed: true }));
  }

  override render(): TemplateResult {
    return html`
      ${this.icon
        ? html`<div class="empty-icon" aria-hidden="true">${this.icon}</div>`
        : null}
      ${this.emptyTitle
        ? html`<h3 class="empty-title">${this.emptyTitle}</h3>`
        : null}
      ${this.subtitle
        ? html`<p class="empty-subtitle">${this.subtitle}</p>`
        : null}
      <slot></slot>
      ${this.actionLabel
        ? html`<button
            class="empty-action"
            type="button"
            @click=${() => this.onActionClick()}
          >${this.actionLabel}</button>`
        : null}
    `;
  }
}

customElements.define('empty-state', EmptyStateWidget);

declare global {
  interface HTMLElementTagNameMap {
    'empty-state': EmptyStateWidget;
  }
}
