/**
 * ProviderEntry - Renders a single provider configuration entry
 *
 * Handles the display of provider info, input field, test button, and status.
 */

import type { TestStatus, ProviderTestResult } from './ProviderStatusTester';

export interface ConfigEntry {
  key: string;
  value: string;
  isSecret: boolean;
  description?: string;
  provider: string;
  category: 'local' | 'cloud';
  isConfigured?: boolean;
  getKeyUrl?: string;
  billingUrl?: string;
  /** Masked key preview like "sk-...QfQA" */
  maskedKey?: string;
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

export class ProviderEntry {
  private entry: ConfigEntry;
  private testResult?: ProviderTestResult;
  private pendingValue?: string;

  constructor(entry: ConfigEntry) {
    this.entry = entry;
  }

  setTestResult(result?: ProviderTestResult): void {
    this.testResult = result;
  }

  setPendingValue(value?: string): void {
    this.pendingValue = value;
  }

  render(): string {
    const { entry, testResult, pendingValue } = this;
    const isConfigured = entry.isConfigured ?? false;
    const statusClass = isConfigured ? 'status-configured' : 'status-not-set';
    const statusText = isConfigured ? '✓ Configured' : '○ Not set';

    return `
      <div class="config-entry" data-entry-key="${entry.key}">
        <div class="provider-header">
          <span class="provider-name">${entry.provider}</span>
          <div class="provider-actions">
            ${this.renderLinks()}
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
            value="${pendingValue ?? entry.value}"
            placeholder="${isConfigured ? (entry.maskedKey || '••••••••') : (entry.isSecret ? 'Enter API key...' : 'Enter URL...')}"
          />
          ${this.renderTestButton()}
        </div>
        ${this.renderTestStatus()}
      </div>
    `;
  }

  private renderLinks(): string {
    const links: string[] = [];

    if (this.entry.getKeyUrl) {
      const label = this.entry.category === 'local' ? 'Download' : 'Get Key';
      links.push(`<a href="${this.entry.getKeyUrl}" target="_blank" class="provider-link">${label}</a>`);
    }

    if (this.entry.billingUrl && this.entry.isConfigured) {
      links.push(`<a href="${this.entry.billingUrl}" target="_blank" class="provider-link">Billing</a>`);
    }

    return links.length > 0 ? `<span class="provider-links">${links.join(' ')}</span>` : '';
  }

  private renderTestButton(): string {
    if (this.entry.category !== 'cloud') return '';

    const isTesting = this.testResult?.status === 'testing';
    return `
      <button class="btn-test ${isTesting ? 'testing' : ''}"
              data-provider="${this.entry.provider.toLowerCase()}"
              data-key="${this.entry.key}"
              ${isTesting ? 'disabled' : ''}>
        ${isTesting ? 'Testing...' : 'Test'}
      </button>
    `;
  }

  private renderTestStatus(): string {
    if (!this.testResult || this.testResult.status === 'idle') return '';

    const { icon, label } = STATUS_MESSAGES[this.testResult.status];
    const responseTimeHtml = this.testResult.responseTimeMs
      ? `<span class="response-time">(${this.testResult.responseTimeMs}ms)</span>`
      : '';

    const actionHtml = this.renderStatusAction();

    return `
      <div class="test-status ${this.testResult.status}">
        <span>${icon} ${label}</span>
        ${responseTimeHtml}
        ${actionHtml}
      </div>
    `;
  }

  private renderStatusAction(): string {
    if (!this.testResult) return '';

    if (this.testResult.status === 'out-of-funds' && this.entry.billingUrl) {
      return `<a href="${this.entry.billingUrl}" target="_blank" class="status-action">Add funds →</a>`;
    }

    if (this.testResult.status === 'invalid' && this.entry.getKeyUrl) {
      return `<a href="${this.entry.getKeyUrl}" target="_blank" class="status-action">Get new key →</a>`;
    }

    if (this.testResult.status === 'rate-limited') {
      return `<span class="status-action-hint">Try again in a few minutes</span>`;
    }

    return '';
  }
}
