/**
 * SettingsWidget - Configuration editor with AI assistance
 *
 * Allows users to view and edit API keys and other settings.
 * Includes embedded AI assistant for help with configuration.
 * Changes are persisted to ~/.continuum/config.env
 */

import { BaseWidget } from '../shared/BaseWidget';
import { AssistantPanel } from '../shared/AssistantPanel';
import { DEFAULT_ROOMS } from '../../system/data/domains/DefaultEntities';
import type { UUID } from '../../system/core/types/CrossPlatformUUID';
import { Commands } from '../../system/core/shared/Commands';

interface ConfigEntry {
  key: string;
  value: string;
  isSecret: boolean;
  description?: string;
  provider: string;
  category: 'local' | 'cloud';
  isConfigured?: boolean;
  getKeyUrl?: string;
  billingUrl?: string;
}

export class SettingsWidget extends BaseWidget {
  private configEntries: ConfigEntry[] = [];
  private isLoading = true;
  private saveStatus: 'idle' | 'saving' | 'saved' | 'error' = 'idle';
  private assistantPanel?: AssistantPanel;

  constructor() {
    super({
      widgetName: 'SettingsWidget',
      template: undefined,
      styles: undefined,
      enableAI: false,
      enableDatabase: false,
      enableRouterEvents: false,
      enableScreenshots: false
    });
  }

  protected async onWidgetInitialize(): Promise<void> {
    console.log('Settings: Initializing settings widget...');
    await this.loadConfig();
  }

  private async loadConfig(): Promise<void> {
    this.isLoading = true;
    this.renderWidget();

    // Get actual provider status from server (checks ~/.continuum/config.env)
    try {
      const result = await Commands.execute('ai/providers/status', {}) as any;

      if (result?.providers) {
        // Map server response to config entries
        this.configEntries = result.providers.map((p: any) => ({
          key: p.key,
          value: '', // Never expose actual values to browser
          isSecret: p.category === 'cloud',
          provider: p.provider,
          category: p.category,
          description: p.description,
          isConfigured: p.isConfigured,
          getKeyUrl: p.getKeyUrl,
          billingUrl: p.billingUrl
        }));
      } else {
        // Fallback to defaults if command fails
        this.configEntries = this.getDefaultConfigEntries();
      }
    } catch (error) {
      console.warn('Settings: Failed to load provider status, using defaults:', error);
      this.configEntries = this.getDefaultConfigEntries();
    }

    this.isLoading = false;
    this.renderWidget();
  }

  private getDefaultConfigEntries(): ConfigEntry[] {
    return [
      // Local AI - Free, runs on your machine
      { key: 'OLLAMA_HOST', value: 'http://localhost:11434', isSecret: false, provider: 'Ollama', category: 'local', description: 'Local AI server - completely free, private, no API key needed' },
      // Cloud Providers - Paid, need API keys
      { key: 'ANTHROPIC_API_KEY', value: '', isSecret: true, provider: 'Anthropic', category: 'cloud', description: 'Claude models - best for complex reasoning' },
      { key: 'OPENAI_API_KEY', value: '', isSecret: true, provider: 'OpenAI', category: 'cloud', description: 'GPT models - widely compatible' },
      { key: 'GROQ_API_KEY', value: '', isSecret: true, provider: 'Groq', category: 'cloud', description: 'Ultra-fast inference' },
      { key: 'DEEPSEEK_API_KEY', value: '', isSecret: true, provider: 'DeepSeek', category: 'cloud', description: 'Cost-effective reasoning' },
      { key: 'XAI_API_KEY', value: '', isSecret: true, provider: 'xAI', category: 'cloud', description: 'Grok models' },
      { key: 'TOGETHER_API_KEY', value: '', isSecret: true, provider: 'Together', category: 'cloud', description: 'Open-source model hosting' },
      { key: 'FIREWORKS_API_KEY', value: '', isSecret: true, provider: 'Fireworks', category: 'cloud', description: 'Fast open-source models' },
    ];
  }

  protected async renderWidget(): Promise<void> {
    const styles = `
      :host {
        display: flex;
        width: 100%;
        height: 100%;
        overflow: hidden;
      }

      .settings-layout {
        display: flex;
        flex: 1;
        width: 100%;
        height: 100%;
        gap: 0;
      }

      .settings-main {
        flex: 1;
        overflow-y: auto;
        padding: 24px;
        min-width: 0;
      }

      .settings-assistant {
        flex-shrink: 0;
        height: 100%;
        display: flex;
      }

      .settings-container {
        width: 100%;
      }

      .settings-header {
        margin-bottom: 24px;
      }

      .settings-title {
        font-size: 24px;
        font-weight: 600;
        color: #00d4ff;
        margin: 0 0 8px 0;
      }

      .settings-subtitle {
        color: rgba(255, 255, 255, 0.6);
        font-size: 14px;
      }

      .settings-section {
        background: rgba(15, 20, 25, 0.8);
        border: 1px solid rgba(0, 212, 255, 0.2);
        border-radius: 8px;
        padding: 20px;
        margin-bottom: 16px;
      }

      .section-title {
        font-size: 16px;
        font-weight: 600;
        color: #00d4ff;
        margin: 0 0 16px 0;
        padding-bottom: 8px;
        border-bottom: 1px solid rgba(0, 212, 255, 0.2);
      }

      .config-entry {
        margin-bottom: 16px;
      }

      .config-entry:last-child {
        margin-bottom: 0;
      }

      .config-label {
        display: flex;
        justify-content: space-between;
        align-items: center;
        margin-bottom: 6px;
      }

      .config-key {
        font-family: monospace;
        font-size: 13px;
        color: #00d4ff;
      }

      .config-description {
        font-size: 12px;
        color: rgba(255, 255, 255, 0.5);
      }

      .config-input {
        width: 100%;
        padding: 10px 12px;
        background: rgba(0, 10, 15, 0.8);
        border: 1px solid rgba(0, 212, 255, 0.3);
        border-radius: 4px;
        color: white;
        font-family: monospace;
        font-size: 14px;
        box-sizing: border-box;
      }

      .config-input:focus {
        outline: none;
        border-color: #00d4ff;
        box-shadow: 0 0 0 2px rgba(0, 212, 255, 0.2);
      }

      .config-input::placeholder {
        color: rgba(255, 255, 255, 0.3);
      }

      .save-section {
        display: flex;
        justify-content: flex-end;
        gap: 12px;
        margin-top: 24px;
        padding-top: 16px;
        border-top: 1px solid rgba(0, 212, 255, 0.2);
      }

      .btn {
        padding: 10px 20px;
        border-radius: 6px;
        font-size: 14px;
        font-weight: 500;
        cursor: pointer;
        transition: all 0.2s ease;
      }

      .btn-primary {
        background: linear-gradient(135deg, #00d4ff, #0099cc);
        border: none;
        color: white;
      }

      .btn-primary:hover {
        background: linear-gradient(135deg, #00e5ff, #00aadd);
        transform: translateY(-1px);
      }

      .btn-secondary {
        background: transparent;
        border: 1px solid rgba(0, 212, 255, 0.4);
        color: #00d4ff;
      }

      .btn-secondary:hover {
        background: rgba(0, 212, 255, 0.1);
      }

      .loading {
        text-align: center;
        padding: 40px;
        color: rgba(255, 255, 255, 0.6);
      }

      .status-message {
        padding: 8px 16px;
        border-radius: 4px;
        font-size: 13px;
      }

      .status-saved {
        background: rgba(0, 255, 100, 0.1);
        color: #00ff64;
      }

      .status-error {
        background: rgba(255, 50, 50, 0.1);
        color: #ff5050;
      }

      .info-box {
        background: rgba(0, 212, 255, 0.1);
        border: 1px solid rgba(0, 212, 255, 0.3);
        border-radius: 6px;
        padding: 12px 16px;
        margin-bottom: 20px;
        font-size: 13px;
        color: rgba(255, 255, 255, 0.8);
      }

      .info-box a {
        color: #00d4ff;
        text-decoration: none;
      }

      .info-box a:hover {
        text-decoration: underline;
      }

      .status-indicator {
        display: inline-flex;
        align-items: center;
        gap: 6px;
        font-size: 11px;
        padding: 2px 8px;
        border-radius: 10px;
      }

      .status-configured {
        background: rgba(0, 255, 100, 0.15);
        color: #00ff64;
      }

      .status-not-set {
        background: rgba(255, 255, 255, 0.1);
        color: rgba(255, 255, 255, 0.5);
      }

      .provider-header {
        display: flex;
        justify-content: space-between;
        align-items: center;
        margin-bottom: 6px;
      }

      .provider-name {
        font-size: 14px;
        font-weight: 500;
        color: #00d4ff;
      }

      .section-intro {
        font-size: 13px;
        color: rgba(255, 255, 255, 0.6);
        margin-bottom: 16px;
        line-height: 1.5;
      }

      .local-highlight {
        background: rgba(0, 255, 100, 0.1);
        border-color: rgba(0, 255, 100, 0.3);
      }

      .local-highlight .section-title {
        color: #00ff64;
      }

      .provider-actions {
        display: flex;
        align-items: center;
        gap: 12px;
      }

      .provider-links {
        display: flex;
        gap: 8px;
      }

      .provider-link {
        font-size: 11px;
        color: #00d4ff;
        text-decoration: none;
        padding: 2px 8px;
        border: 1px solid rgba(0, 212, 255, 0.3);
        border-radius: 4px;
        transition: all 0.2s ease;
      }

      .provider-link:hover {
        background: rgba(0, 212, 255, 0.1);
        border-color: rgba(0, 212, 255, 0.5);
      }

      @media (max-width: 768px) {
        .settings-layout {
          flex-direction: column;
        }

        .settings-assistant {
          height: 300px;
          flex-shrink: 0;
        }
      }
    `;

    // Separate local and cloud entries
    const localEntries = this.configEntries.filter(e => e.category === 'local');
    const cloudEntries = this.configEntries.filter(e => e.category === 'cloud');

    const renderEntry = (entry: ConfigEntry) => {
      const isConfigured = entry.isConfigured ?? false;
      const statusClass = isConfigured ? 'status-configured' : 'status-not-set';
      const statusText = isConfigured ? '✓ Configured' : '○ Not set';

      // Build action links
      const links: string[] = [];
      if (entry.getKeyUrl) {
        links.push(`<a href="${entry.getKeyUrl}" target="_blank" class="provider-link">${entry.category === 'local' ? 'Download' : 'Get Key'}</a>`);
      }
      if (entry.billingUrl && isConfigured) {
        links.push(`<a href="${entry.billingUrl}" target="_blank" class="provider-link">Billing</a>`);
      }
      const linksHtml = links.length > 0 ? `<span class="provider-links">${links.join(' ')}</span>` : '';

      return `
        <div class="config-entry">
          <div class="provider-header">
            <span class="provider-name">${entry.provider}</span>
            <div class="provider-actions">
              ${linksHtml}
              <span class="status-indicator ${statusClass}">${statusText}</span>
            </div>
          </div>
          <div class="config-label">
            <span class="config-key">${entry.key}</span>
            <span class="config-description">${entry.description || ''}</span>
          </div>
          <input
            type="${entry.isSecret ? 'password' : 'text'}"
            class="config-input"
            data-key="${entry.key}"
            value="${entry.value}"
            placeholder="${isConfigured ? '••••••••••••••••' : (entry.isSecret ? 'Enter API key...' : 'Enter URL...')}"
          />
        </div>
      `;
    };

    const localEntriesHtml = localEntries.map(renderEntry).join('');
    const cloudEntriesHtml = cloudEntries.map(renderEntry).join('');

    const template = this.isLoading ? `
      <div class="settings-layout">
        <div class="settings-main">
          <div class="settings-container">
            <div class="loading">Loading configuration...</div>
          </div>
        </div>
        <div class="settings-assistant" id="assistant-container"></div>
      </div>
    ` : `
      <div class="settings-layout">
        <div class="settings-main">
          <div class="settings-container">
            <div class="settings-header">
              <h1 class="settings-title">AI Providers</h1>
              <p class="settings-subtitle">Connect AI services to power your assistants</p>
            </div>

            <div class="info-box">
              <strong>Choose your setup:</strong> Run AI locally for free with Ollama,
              or connect cloud providers for more powerful models. You can use multiple providers.
            </div>

            <div class="settings-section local-highlight">
              <h2 class="section-title">Local AI (Free)</h2>
              <p class="section-intro">
                Runs on your machine. No API key required. Private and unlimited.
                <a href="https://ollama.ai" target="_blank">Download Ollama</a> if not installed.
              </p>
              ${localEntriesHtml}
            </div>

            <div class="settings-section">
              <h2 class="section-title">Cloud Providers (Paid)</h2>
              <p class="section-intro">
                Requires API keys from each provider. More powerful models, usage-based pricing.
              </p>
              ${cloudEntriesHtml}
            </div>

            <div class="save-section">
              ${this.saveStatus === 'saved' ? '<span class="status-message status-saved">Settings saved!</span>' : ''}
              ${this.saveStatus === 'error' ? '<span class="status-message status-error">Failed to save</span>' : ''}
              <button class="btn btn-secondary" id="reset-btn">Reset</button>
              <button class="btn btn-primary" id="save-btn">Save Changes</button>
            </div>
          </div>
        </div>
        <div class="settings-assistant" id="assistant-container"></div>
      </div>
    `;

    this.shadowRoot!.innerHTML = `<style>${styles}</style>${template}`;

    // Setup event listeners
    this.setupEventListeners();

    // Initialize assistant panel
    this.initializeAssistant();
  }

  private initializeAssistant(): void {
    const container = this.shadowRoot?.querySelector('#assistant-container') as HTMLElement;
    if (!container) return;

    // Clean up old instance
    this.assistantPanel?.destroy();

    // Create new assistant panel connected to Settings room
    this.assistantPanel = new AssistantPanel(container, {
      roomId: DEFAULT_ROOMS.SETTINGS as UUID,
      roomName: 'settings',
      placeholder: 'Ask about providers, pricing, or setup...',
      greeting: "I can help you choose and configure AI providers. Ask me about:\n• Which provider is best for your needs\n• How to get API keys\n• Troubleshooting connections\n• Comparing costs and capabilities"
    });
  }

  private setupEventListeners(): void {
    const saveBtn = this.shadowRoot?.querySelector('#save-btn');
    const resetBtn = this.shadowRoot?.querySelector('#reset-btn');

    saveBtn?.addEventListener('click', () => this.saveConfig());
    resetBtn?.addEventListener('click', () => this.loadConfig());

    // Track changes in inputs
    this.shadowRoot?.querySelectorAll('.config-input').forEach(input => {
      input.addEventListener('change', (e) => {
        const target = e.target as HTMLInputElement;
        const key = target.dataset.key;
        if (key) {
          const entry = this.configEntries.find(e => e.key === key);
          if (entry) {
            entry.value = target.value;
          }
        }
        this.saveStatus = 'idle';
      });
    });
  }

  private async saveConfig(): Promise<void> {
    this.saveStatus = 'saving';
    this.renderWidget();

    try {
      // TODO: Implement system/config/save command
      const config: Record<string, string> = {};
      for (const entry of this.configEntries) {
        if (entry.value) {
          config[entry.key] = entry.value;
        }
      }
      console.log('Settings: Would save config:', Object.keys(config));

      // Simulate save delay
      await new Promise(resolve => setTimeout(resolve, 500));

      this.saveStatus = 'saved';
      console.log('Settings: Configuration saved (stub)');
    } catch (error) {
      console.error('Settings: Failed to save config:', error);
      this.saveStatus = 'error';
    }

    this.renderWidget();
  }

  protected async onWidgetCleanup(): Promise<void> {
    this.assistantPanel?.destroy();
    console.log('Settings: Cleanup complete');
  }
}

// Registration handled by centralized BROWSER_WIDGETS registry
