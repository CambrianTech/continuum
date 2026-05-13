/**
 * ModalWidget — generic Lit modal dialog.
 *
 * Reactive `open` property. When opened, traps focus inside, restores
 * focus on close, listens for Escape and backdrop clicks. Accessible
 * by default: role="dialog", aria-modal="true", aria-labelledby on the
 * title.
 *
 * Slots:
 *   - default: modal body content
 *   - footer: action buttons (optional)
 *
 * Properties:
 *   - open: boolean — whether the modal is visible
 *   - modalTitle: string — title text (drives aria-labelledby)
 *   - closable: boolean — whether the user can dismiss via X / Escape /
 *     backdrop. Set false for required flows. Defaults true.
 *
 * Events:
 *   - modal-close: fired when the user dismisses the modal
 *
 * Introduced under #1101 (first-run UX) as part of PR-A. Designed to
 * be reusable for any future modal need — settings dialogs, confirms,
 * onboarding flows.
 */

import { LitElement, html, css, type TemplateResult } from 'lit';

const FOCUSABLE_SELECTOR = [
  'a[href]',
  'button:not([disabled])',
  'input:not([disabled])',
  'textarea:not([disabled])',
  'select:not([disabled])',
  '[tabindex]:not([tabindex="-1"])',
].join(',');

export class ModalWidget extends LitElement {
  static override properties = {
    open: { type: Boolean, reflect: true },
    modalTitle: { type: String, attribute: 'modal-title' },
    closable: { type: Boolean },
  } as const;

  open = false;
  modalTitle = '';
  closable = true;

  private _previouslyFocused: HTMLElement | null = null;
  private _onKeyDown = (e: KeyboardEvent) => this.handleKeyDown(e);

  static override styles = css`
    :host {
      display: contents;
    }

    .modal-backdrop {
      position: fixed;
      inset: 0;
      background: rgba(0, 0, 0, 0.55);
      display: flex;
      align-items: center;
      justify-content: center;
      z-index: 9999;
      animation: fade-in 120ms ease-out;
    }

    .modal-dialog {
      background: var(--surface-primary, #1e1e1e);
      color: var(--text-primary, #e0e0e0);
      border: 1px solid var(--border-subtle, rgba(255, 255, 255, 0.1));
      border-radius: 10px;
      min-width: 320px;
      max-width: min(560px, 90vw);
      max-height: 90vh;
      display: flex;
      flex-direction: column;
      box-shadow: 0 12px 48px rgba(0, 0, 0, 0.45);
      animation: zoom-in 150ms cubic-bezier(0.2, 0.9, 0.2, 1.1);
    }

    .modal-header {
      display: flex;
      align-items: center;
      gap: 8px;
      padding: 14px 16px;
      border-bottom: 1px solid var(--border-subtle, rgba(255, 255, 255, 0.1));
    }

    .modal-title {
      flex: 1;
      font-size: 1.1em;
      font-weight: 600;
      margin: 0;
    }

    .modal-close {
      background: transparent;
      border: 0;
      color: inherit;
      cursor: pointer;
      font-size: 1.2em;
      padding: 4px 8px;
      border-radius: 4px;
      line-height: 1;
    }

    .modal-close:hover {
      background: rgba(255, 255, 255, 0.08);
    }

    .modal-body {
      padding: 16px;
      overflow-y: auto;
      flex: 1;
    }

    .modal-footer {
      display: flex;
      justify-content: flex-end;
      gap: 8px;
      padding: 12px 16px;
      border-top: 1px solid var(--border-subtle, rgba(255, 255, 255, 0.1));
    }

    .modal-footer:empty {
      display: none;
    }

    @keyframes fade-in {
      from { opacity: 0; }
      to { opacity: 1; }
    }

    @keyframes zoom-in {
      from { transform: scale(0.96); opacity: 0; }
      to { transform: scale(1); opacity: 1; }
    }
  `;

  override connectedCallback(): void {
    super.connectedCallback();
    document.addEventListener('keydown', this._onKeyDown);
  }

  override disconnectedCallback(): void {
    document.removeEventListener('keydown', this._onKeyDown);
    super.disconnectedCallback();
  }

  override updated(changed: Map<string, unknown>): void {
    if (changed.has('open')) {
      if (this.open) {
        this._previouslyFocused = (this.getRootNode() as Document).activeElement as HTMLElement | null;
        // Defer focusing to next paint so the dialog is in the DOM.
        requestAnimationFrame(() => this.focusFirstElement());
      } else if (this._previouslyFocused) {
        this._previouslyFocused.focus?.();
        this._previouslyFocused = null;
      }
    }
  }

  private handleKeyDown(e: KeyboardEvent): void {
    if (!this.open) return;
    if (e.key === 'Escape' && this.closable) {
      e.stopPropagation();
      this.requestClose();
      return;
    }
    if (e.key === 'Tab') {
      this.trapFocus(e);
    }
  }

  private trapFocus(e: KeyboardEvent): void {
    const focusable = this.getFocusableElements();
    if (focusable.length === 0) return;
    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    const active = this.shadowRoot?.activeElement as HTMLElement | null;
    if (e.shiftKey && active === first) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && active === last) {
      e.preventDefault();
      first.focus();
    }
  }

  private getFocusableElements(): HTMLElement[] {
    const dialog = this.shadowRoot?.querySelector('.modal-dialog');
    if (!dialog) return [];
    return Array.from(dialog.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR));
  }

  private focusFirstElement(): void {
    const focusable = this.getFocusableElements();
    if (focusable.length > 0) {
      focusable[0].focus();
    } else {
      // Fallback: focus the dialog itself so Escape still works
      (this.shadowRoot?.querySelector('.modal-dialog') as HTMLElement | null)?.focus();
    }
  }

  /**
   * Programmatic close — also fires the modal-close event so parents
   * can react (e.g., persist `hasOnboarded=true`).
   */
  requestClose(): void {
    if (!this.closable) return;
    this.open = false;
    this.dispatchEvent(new CustomEvent('modal-close', { bubbles: true, composed: true }));
  }

  private onBackdropClick(e: MouseEvent): void {
    if (e.target === e.currentTarget) {
      this.requestClose();
    }
  }

  override render(): TemplateResult | null {
    if (!this.open) return null;
    const titleId = `modal-title-${this.uniqueId}`;
    return html`
      <div
        class="modal-backdrop"
        @click=${(e: MouseEvent) => this.onBackdropClick(e)}
      >
        <div
          class="modal-dialog"
          role="dialog"
          aria-modal="true"
          aria-labelledby=${titleId}
          tabindex="-1"
        >
          <header class="modal-header">
            <h2 class="modal-title" id=${titleId}>${this.modalTitle}</h2>
            ${this.closable
              ? html`<button
                  class="modal-close"
                  type="button"
                  aria-label="Close dialog"
                  @click=${() => this.requestClose()}
                >×</button>`
              : null}
          </header>
          <div class="modal-body">
            <slot></slot>
          </div>
          <footer class="modal-footer">
            <slot name="footer"></slot>
          </footer>
        </div>
      </div>
    `;
  }

  // Stable id per instance — used for aria-labelledby. Random suffix
  // so two modals on the same page don't collide.
  private readonly uniqueId = Math.random().toString(36).slice(2, 10);
}

customElements.define('modal-widget', ModalWidget);

declare global {
  interface HTMLElementTagNameMap {
    'modal-widget': ModalWidget;
  }
}
