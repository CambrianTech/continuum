/**
 * ReactiveFormExample - Demonstrates efficient form handling
 *
 * Shows the React-like pattern:
 * - Reactive state bindings (no manual DOM updates)
 * - Efficient re-renders (only changed fields update)
 * - Declarative validation
 * - Automatic event cleanup
 *
 * Compare to old pattern:
 *   OLD: element.innerHTML = `<input value="${value}">` // destroys focus!
 *   NEW: html`<input .value=${value}>` // preserves focus, updates efficiently
 */

import { ReactiveWidget, html, css, reactive, attr } from '../ReactiveWidget';
import type { TemplateResult, CSSResultGroup } from '../ReactiveWidget';

interface FormData {
  name: string;
  email: string;
  message: string;
}

interface ValidationErrors {
  name?: string;
  email?: string;
  message?: string;
}

export class ReactiveFormExample extends ReactiveWidget {
  // ═══════════════════════════════════════════════════════════════════════════
  // STATIC PROPERTIES - Declare reactive state using Lit's static properties
  // ═══════════════════════════════════════════════════════════════════════════

  static override properties = {
    ...ReactiveWidget.properties,
    formData: { type: Object, state: true },
    errors: { type: Object, state: true },
    submitted: { type: Boolean, state: true },
    submitCount: { type: Number, state: true },
    title: { type: String },
    showDebug: { type: Boolean, attribute: 'show-debug' }
  };

  // ═══════════════════════════════════════════════════════════════════════════
  // REACTIVE STATE - Changes trigger efficient re-renders
  // ═══════════════════════════════════════════════════════════════════════════

  protected formData: FormData = {
    name: '',
    email: '',
    message: ''
  };

  protected errors: ValidationErrors = {};

  protected submitted = false;

  protected submitCount = 0;

  // ═══════════════════════════════════════════════════════════════════════════
  // PROPERTIES - Can be set via attributes
  // ═══════════════════════════════════════════════════════════════════════════

  title = 'Contact Form';

  showDebug = false;

  // ═══════════════════════════════════════════════════════════════════════════
  // STYLES - Scoped to this component
  // ═══════════════════════════════════════════════════════════════════════════

  static styles: CSSResultGroup = [
    ReactiveWidget.styles,
    css`
      :host {
        display: block;
        padding: 24px;
        font-family: system-ui, sans-serif;
      }

      .form-container {
        max-width: 500px;
        margin: 0 auto;
        background: var(--bg-panel, rgba(20, 25, 35, 0.95));
        border: 1px solid var(--border-color, rgba(0, 212, 255, 0.2));
        border-radius: 8px;
        padding: 24px;
      }

      h2 {
        color: var(--color-primary, #00d4ff);
        margin: 0 0 24px 0;
        font-size: 20px;
      }

      .form-group {
        margin-bottom: 16px;
      }

      label {
        display: block;
        margin-bottom: 6px;
        color: var(--color-text, #e0e0e0);
        font-size: 14px;
        font-weight: 500;
      }

      input, textarea {
        width: 100%;
        padding: 10px 12px;
        background: rgba(255, 255, 255, 0.05);
        border: 1px solid var(--border-color, rgba(0, 212, 255, 0.2));
        border-radius: 4px;
        color: var(--color-text, #e0e0e0);
        font-size: 14px;
        font-family: inherit;
        box-sizing: border-box;
        transition: border-color 0.2s, box-shadow 0.2s;
      }

      input:focus, textarea:focus {
        outline: none;
        border-color: var(--color-primary, #00d4ff);
        box-shadow: 0 0 0 2px rgba(0, 212, 255, 0.1);
      }

      input.invalid, textarea.invalid {
        border-color: var(--color-error, #ff5050);
      }

      textarea {
        min-height: 100px;
        resize: vertical;
      }

      .error-text {
        color: var(--color-error, #ff5050);
        font-size: 12px;
        margin-top: 4px;
      }

      .button-row {
        display: flex;
        gap: 12px;
        margin-top: 24px;
      }

      button {
        padding: 10px 20px;
        border: none;
        border-radius: 4px;
        font-size: 14px;
        font-weight: 500;
        cursor: pointer;
        transition: all 0.2s;
      }

      button.primary {
        background: var(--color-primary, #00d4ff);
        color: var(--bg-darker, #0a0e14);
      }

      button.primary:hover {
        box-shadow: 0 0 12px rgba(0, 212, 255, 0.4);
      }

      button.primary:disabled {
        opacity: 0.5;
        cursor: not-allowed;
      }

      button.secondary {
        background: transparent;
        border: 1px solid var(--border-color, rgba(0, 212, 255, 0.2));
        color: var(--color-text, #e0e0e0);
      }

      button.secondary:hover {
        border-color: var(--color-primary, #00d4ff);
      }

      .success-message {
        padding: 16px;
        background: rgba(0, 255, 100, 0.1);
        border: 1px solid var(--color-success, #00ff64);
        border-radius: 4px;
        color: var(--color-success, #00ff64);
        text-align: center;
        margin-bottom: 16px;
      }

      .debug-panel {
        margin-top: 24px;
        padding: 16px;
        background: rgba(0, 0, 0, 0.3);
        border-radius: 4px;
        font-family: monospace;
        font-size: 12px;
        color: var(--color-text-muted, #888);
      }

      .debug-panel h4 {
        margin: 0 0 8px 0;
        color: var(--color-primary, #00d4ff);
      }

      .debug-panel pre {
        margin: 0;
        white-space: pre-wrap;
        word-break: break-all;
      }

      .render-indicator {
        position: absolute;
        top: 8px;
        right: 8px;
        width: 8px;
        height: 8px;
        background: var(--color-success, #00ff64);
        border-radius: 50%;
        animation: pulse 0.3s ease-out;
      }

      @keyframes pulse {
        0% { transform: scale(1.5); opacity: 0.5; }
        100% { transform: scale(1); opacity: 1; }
      }
    `
  ];

  // ═══════════════════════════════════════════════════════════════════════════
  // LIFECYCLE
  // ═══════════════════════════════════════════════════════════════════════════

  constructor() {
    super({ widgetName: 'ReactiveFormExample', debug: true });
  }

  protected onFirstRender(): void {
    this.emitContext({
      widgetType: 'form-example',
      title: this.title
    });
  }

  // ═══════════════════════════════════════════════════════════════════════════
  // RENDER - Declarative, efficient updates
  // ═══════════════════════════════════════════════════════════════════════════

  protected renderContent(): TemplateResult {
    return html`
      <div class="form-container" style="position: relative;">
        <!-- Render indicator flashes on each render -->
        ${this.showDebug ? html`<div class="render-indicator"></div>` : ''}

        <h2>${this.title}</h2>

        ${this.submitted ? this.renderSuccess() : this.renderForm()}

        ${this.showDebug ? this.renderDebug() : ''}
      </div>
    `;
  }

  private renderForm(): TemplateResult {
    return html`
      <form @submit=${this.handleSubmit}>
        <div class="form-group">
          <label for="name">Name</label>
          <input
            id="name"
            type="text"
            class=${this.errors.name ? 'invalid' : ''}
            .value=${this.formData.name}
            @input=${(e: InputEvent) => this.updateField('name', (e.target as HTMLInputElement).value)}
            @blur=${() => this.validateField('name')}
            placeholder="Enter your name"
          />
          ${this.errors.name ? html`<div class="error-text">${this.errors.name}</div>` : ''}
        </div>

        <div class="form-group">
          <label for="email">Email</label>
          <input
            id="email"
            type="email"
            class=${this.errors.email ? 'invalid' : ''}
            .value=${this.formData.email}
            @input=${(e: InputEvent) => this.updateField('email', (e.target as HTMLInputElement).value)}
            @blur=${() => this.validateField('email')}
            placeholder="Enter your email"
          />
          ${this.errors.email ? html`<div class="error-text">${this.errors.email}</div>` : ''}
        </div>

        <div class="form-group">
          <label for="message">Message</label>
          <textarea
            id="message"
            class=${this.errors.message ? 'invalid' : ''}
            .value=${this.formData.message}
            @input=${(e: InputEvent) => this.updateField('message', (e.target as HTMLTextAreaElement).value)}
            @blur=${() => this.validateField('message')}
            placeholder="Enter your message"
          ></textarea>
          ${this.errors.message ? html`<div class="error-text">${this.errors.message}</div>` : ''}
        </div>

        <div class="button-row">
          <button type="submit" class="primary" ?disabled=${!this.isValid}>
            Submit (${this.submitCount})
          </button>
          <button type="button" class="secondary" @click=${this.resetForm}>
            Reset
          </button>
        </div>
      </form>
    `;
  }

  private renderSuccess(): TemplateResult {
    return html`
      <div class="success-message">
        ✓ Form submitted successfully!
      </div>
      <button class="secondary" @click=${this.resetForm}>
        Submit Another
      </button>
    `;
  }

  private renderDebug(): TemplateResult {
    return html`
      <div class="debug-panel">
        <h4>🔍 Debug State (updates efficiently)</h4>
        <pre>${JSON.stringify({
          formData: this.formData,
          errors: this.errors,
          isValid: this.isValid,
          submitted: this.submitted,
          submitCount: this.submitCount
        }, null, 2)}</pre>
      </div>
    `;
  }

  // ═══════════════════════════════════════════════════════════════════════════
  // COMPUTED PROPERTIES
  // ═══════════════════════════════════════════════════════════════════════════

  private get isValid(): boolean {
    return (
      this.formData.name.length > 0 &&
      this.formData.email.length > 0 &&
      this.formData.message.length > 0 &&
      Object.keys(this.errors).length === 0
    );
  }

  // ═══════════════════════════════════════════════════════════════════════════
  // EVENT HANDLERS - Bound automatically by Lit
  // ═══════════════════════════════════════════════════════════════════════════

  private updateField(field: keyof FormData, value: string): void {
    // Create new object to trigger reactive update
    this.formData = { ...this.formData, [field]: value };

    // Clear error on input
    if (this.errors[field]) {
      const newErrors = { ...this.errors };
      delete newErrors[field];
      this.errors = newErrors;
    }
  }

  private validateField(field: keyof FormData): void {
    const value = this.formData[field];
    const newErrors = { ...this.errors };

    switch (field) {
      case 'name':
        if (!value.trim()) {
          newErrors.name = 'Name is required';
        } else if (value.length < 2) {
          newErrors.name = 'Name must be at least 2 characters';
        } else {
          delete newErrors.name;
        }
        break;

      case 'email':
        if (!value.trim()) {
          newErrors.email = 'Email is required';
        } else if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(value)) {
          newErrors.email = 'Please enter a valid email';
        } else {
          delete newErrors.email;
        }
        break;

      case 'message':
        if (!value.trim()) {
          newErrors.message = 'Message is required';
        } else if (value.length < 10) {
          newErrors.message = 'Message must be at least 10 characters';
        } else {
          delete newErrors.message;
        }
        break;
    }

    this.errors = newErrors;
  }

  private handleSubmit(e: Event): void {
    e.preventDefault();

    // Validate all fields
    this.validateField('name');
    this.validateField('email');
    this.validateField('message');

    if (this.isValid) {
      this.submitCount++;
      this.submitted = true;

      // Emit context for AI awareness
      this.emitContext(
        { widgetType: 'form-example', title: 'Form Submitted' },
        { action: 'editing', target: 'contact form' }
      );

      console.log('📝 Form submitted:', this.formData);
    }
  }

  private resetForm(): void {
    this.formData = { name: '', email: '', message: '' };
    this.errors = {};
    this.submitted = false;
  }
}

// Register custom element
customElements.define('reactive-form-example', ReactiveFormExample);

declare global {
  interface HTMLElementTagNameMap {
    'reactive-form-example': ReactiveFormExample;
  }
}
