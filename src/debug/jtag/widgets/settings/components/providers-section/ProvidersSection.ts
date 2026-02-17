/**
 * ProvidersSection - AI Providers Settings Component
 *
 * A Lit component that displays and manages AI provider configurations.
 * Extracted from SettingsWidget for component decomposition.
 *
 * Uses reactive properties for state management and emits events
 * to parent for actions like save/reset.
 */

import {
  ReactiveWidget,
  html,
  reactive,
  unsafeCSS,
  type TemplateResult,
  type CSSResultGroup
} from '../../../shared/ReactiveWidget';

import { styles } from './providers-section.styles';
import type { ConfigEntry } from '../ProviderEntry';
import type { TestStatus, ProviderTestResult } from '../ProviderStatusTester';

/**
 * Events emitted by ProvidersSection
 */
export interface ProvidersSectionEvents {
  /** Emitted when input value changes */
  'input-change': { key: string; value: string };
  /** Emitted when test button is clicked */
  'test-click': { provider: string; configKey: string };
  /** Emitted when save button is clicked */
  'save-click': void;
  /** Emitted when reset button is clicked */
  'reset-click': void;
  /** Emitted when refresh button is clicked */
  'refresh-click': void;
}

const STATUS_MESSAGES: Record<TestStatus, { icon: string; label: string }> = {
  'idle': { icon: '', label: '' },
  'testing': { icon: '⏳', label: 'Testing connection...' },
  'operational': { icon: '✓', label: 'Operational' },
  'invalid': { icon: '✗', label: 'Invalid key' },
  'out-of-funds': { icon: '⚠', label: 'Out of funds' },
  'rate-limited': { icon: '⏱', label: 'Rate limited' },
  'error': { icon: '✗', label: 'Connection error' }
};

export class ProvidersSection extends ReactiveWidget {
  static override styles = [
    ReactiveWidget.styles,
    unsafeCSS(styles)
  ] as CSSResultGroup;

  // Reactive properties (passed from parent)
  @reactive() entries: ConfigEntry[] = [];
  @reactive() testResults: Map<string, ProviderTestResult> = new Map();
  @reactive() pendingChanges: Map<string, string> = new Map();
  @reactive() saveStatus: 'idle' | 'saving' | 'saved' | 'error' = 'idle';

  constructor() {
    super({ widgetName: 'ProvidersSection' });
  }

  protected override renderContent(): TemplateResult {
    const localEntries = this.entries.filter(e => e.category === 'local');
    const cloudEntries = this.entries.filter(e => e.category === 'cloud');

    return html`
      <div class="providers-header">
        <h1 class="providers-title">AI Providers</h1>
        <p class="providers-subtitle">Connect AI services to power your assistants</p>
      </div>

      <div class="info-box">
        <strong>Choose your setup:</strong> Run AI locally for free with Ollama,
        or connect cloud providers for more powerful models. You can use multiple providers.
        <span class="storage-note">
          Keys stored in <code>~/.continuum/config.env</code>
          <button class="btn-refresh" @click=${this.handleRefresh} title="Reload from file">↻</button>
        </span>
      </div>

      <div class="provider-category local-highlight">
        <h2 class="category-title">Local AI (Free)</h2>
        <p class="category-intro">
          Runs on your machine. No API key required. Private and unlimited.
          <a href="https://ollama.ai" target="_blank">Download Ollama</a> if not installed.
        </p>
        ${localEntries.map(entry => this.renderProviderEntry(entry))}
      </div>

      <div class="provider-category">
        <h2 class="category-title">Cloud Providers (Paid)</h2>
        <p class="category-intro">
          Requires API keys from each provider. More powerful models, usage-based pricing.
        </p>
        ${cloudEntries.map(entry => this.renderProviderEntry(entry))}
      </div>

      <div class="save-section">
        ${this.saveStatus === 'saved' ? html`<span class="status-message status-saved">Settings saved!</span>` : ''}
        ${this.saveStatus === 'error' ? html`<span class="status-message status-error">Fix errors before saving</span>` : ''}
        <button class="btn btn-secondary" @click=${this.handleReset}>Reset</button>
        <button class="btn btn-primary" @click=${this.handleSave}>Save Changes</button>
      </div>
    `;
  }

  private renderProviderEntry(entry: ConfigEntry): TemplateResult {
    const isConfigured = entry.isConfigured ?? false;
    const statusClass = isConfigured ? 'status-configured' : 'status-not-set';
    const statusText = isConfigured ? '✓ Configured' : '○ Not set';
    const testResult = this.testResults.get(entry.key);
    const pendingValue = this.pendingChanges.get(entry.key);

    return html`
      <div class="provider-entry" data-entry-key="${entry.key}">
        <div class="provider-header">
          <span class="provider-name">${entry.provider}</span>
          <div class="provider-actions">
            ${this.renderLinks(entry)}
            <span class="status-indicator ${statusClass}">${statusText}</span>
          </div>
        </div>
        <div class="config-label">
          <span class="config-key">${entry.key}</span>
          <span class="config-description">${entry.description || ''}</span>
        </div>
        <div class="input-row">
          <input
            type="${entry.isSecret ? 'password' : 'text'}"
            class="config-input"
            data-key="${entry.key}"
            data-provider="${entry.provider.toLowerCase()}"
            .value="${pendingValue ?? entry.value}"
            placeholder="${isConfigured ? (entry.maskedKey || '••••••••') : (entry.isSecret ? 'Enter API key...' : 'Enter URL...')}"
            @input=${(e: InputEvent) => this.handleInput(entry.key, e)}
          />
          ${this.renderTestButton(entry, testResult)}
        </div>
        ${this.renderTestStatus(entry, testResult)}
      </div>
    `;
  }

  private renderLinks(entry: ConfigEntry): TemplateResult {
    const links: TemplateResult[] = [];

    if (entry.getKeyUrl) {
      const label = entry.category === 'local' ? 'Download' : 'Get Key';
      links.push(html`<a href="${entry.getKeyUrl}" target="_blank" class="provider-link">${label}</a>`);
    }

    if (entry.billingUrl && entry.isConfigured) {
      links.push(html`<a href="${entry.billingUrl}" target="_blank" class="provider-link">Billing</a>`);
    }

    return links.length > 0
      ? html`<span class="provider-links">${links}</span>`
      : html``;
  }

  private renderTestButton(entry: ConfigEntry, testResult?: ProviderTestResult): TemplateResult {
    if (entry.category !== 'cloud') return html``;

    const isTesting = testResult?.status === 'testing';
    return html`
      <button
        class="btn-test ${isTesting ? 'testing' : ''}"
        data-provider="${entry.provider.toLowerCase()}"
        data-key="${entry.key}"
        ?disabled=${isTesting}
        @click=${() => this.handleTest(entry.provider.toLowerCase(), entry.key)}
      >
        ${isTesting ? 'Testing...' : 'Test'}
      </button>
    `;
  }

  private renderTestStatus(entry: ConfigEntry, testResult?: ProviderTestResult): TemplateResult {
    if (!testResult || testResult.status === 'idle') return html``;

    const { icon, label } = STATUS_MESSAGES[testResult.status];
    const responseTimeHtml = testResult.responseTimeMs
      ? html`<span class="response-time">(${testResult.responseTimeMs}ms)</span>`
      : '';

    return html`
      <div class="test-status ${testResult.status}">
        <span>${icon} ${label}</span>
        ${responseTimeHtml}
        ${this.renderStatusAction(entry, testResult)}
      </div>
    `;
  }

  private renderStatusAction(entry: ConfigEntry, testResult: ProviderTestResult): TemplateResult {
    if (testResult.status === 'out-of-funds' && entry.billingUrl) {
      return html`<a href="${entry.billingUrl}" target="_blank" class="status-action">Add funds →</a>`;
    }

    if (testResult.status === 'invalid' && entry.getKeyUrl) {
      return html`<a href="${entry.getKeyUrl}" target="_blank" class="status-action">Get new key →</a>`;
    }

    if (testResult.status === 'rate-limited') {
      return html`<span class="status-action-hint">Try again in a few minutes</span>`;
    }

    return html``;
  }

  // Event handlers - emit custom events to parent

  private handleInput(key: string, e: InputEvent): void {
    const value = (e.target as HTMLInputElement).value;
    this.dispatchEvent(new CustomEvent('input-change', {
      detail: { key, value },
      bubbles: true,
      composed: true
    }));
  }

  private handleTest(provider: string, configKey: string): void {
    this.dispatchEvent(new CustomEvent('test-click', {
      detail: { provider, configKey },
      bubbles: true,
      composed: true
    }));
  }

  private handleSave(): void {
    this.dispatchEvent(new CustomEvent('save-click', {
      bubbles: true,
      composed: true
    }));
  }

  private handleReset(): void {
    this.dispatchEvent(new CustomEvent('reset-click', {
      bubbles: true,
      composed: true
    }));
  }

  private handleRefresh(): void {
    this.dispatchEvent(new CustomEvent('refresh-click', {
      bubbles: true,
      composed: true
    }));
  }
}

// Register custom element
customElements.define('providers-section', ProvidersSection);

// TypeScript declaration for HTML usage
declare global {
  interface HTMLElementTagNameMap {
    'providers-section': ProvidersSection;
  }
}
