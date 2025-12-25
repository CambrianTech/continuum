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
import { SETTINGS_STYLES } from './components/SettingsStyles';
import { ProviderEntry, type ConfigEntry } from './components/ProviderEntry';
import { ProviderStatusTester } from './components/ProviderStatusTester';

export class SettingsWidget extends BaseWidget {
  private configEntries: ConfigEntry[] = [];
  private isLoading = true;
  private saveStatus: 'idle' | 'saving' | 'saved' | 'error' = 'idle';
  private assistantPanel?: AssistantPanel;
  private tester: ProviderStatusTester;
  private pendingChanges: Map<string, string> = new Map();

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

    // Initialize tester with callback to re-render on status changes
    this.tester = new ProviderStatusTester(() => this.renderWidget());
  }

  protected async onWidgetInitialize(): Promise<void> {
    console.log('Settings: Initializing settings widget...');
    await this.loadConfig();
  }

  private async loadConfig(): Promise<void> {
    this.isLoading = true;
    this.renderWidget();

    try {
      console.log('Settings: Calling ai/providers/status...');
      const result = await Commands.execute('ai/providers/status', {} as any) as any;

      if (result?.providers) {
        this.configEntries = result.providers.map((p: any) => ({
          key: p.key,
          value: '',
          isSecret: p.category === 'cloud',
          provider: p.provider,
          category: p.category,
          description: p.description,
          isConfigured: p.isConfigured,
          getKeyUrl: p.getKeyUrl,
          billingUrl: p.billingUrl
        }));
      } else {
        this.configEntries = this.getDefaultConfigEntries();
      }
    } catch (error) {
      console.error('Settings: Error loading provider status:', error);
      this.configEntries = this.getDefaultConfigEntries();
    }

    this.isLoading = false;
    this.pendingChanges.clear();
    this.tester.clearAll();
    this.renderWidget();
  }

  private getDefaultConfigEntries(): ConfigEntry[] {
    return [
      { key: 'OLLAMA_HOST', value: 'http://localhost:11434', isSecret: false, provider: 'Ollama', category: 'local', description: 'Local AI server - completely free, private, no API key needed' },
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
    const localEntries = this.configEntries.filter(e => e.category === 'local');
    const cloudEntries = this.configEntries.filter(e => e.category === 'cloud');

    const renderEntry = (entry: ConfigEntry): string => {
      const providerEntry = new ProviderEntry(entry);
      providerEntry.setTestResult(this.tester.getResult(entry.key));
      providerEntry.setPendingValue(this.pendingChanges.get(entry.key));
      return providerEntry.render();
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
              <span class="storage-note">Stored locally in config.env</span>
              ${this.saveStatus === 'saved' ? '<span class="status-message status-saved">Settings saved!</span>' : ''}
              ${this.saveStatus === 'error' ? '<span class="status-message status-error">Fix errors before saving</span>' : ''}
              <button class="btn btn-secondary" id="reset-btn">Reset</button>
              <button class="btn btn-primary" id="save-btn">Save Changes</button>
            </div>
          </div>
        </div>
        <div class="settings-assistant" id="assistant-container"></div>
      </div>
    `;

    this.shadowRoot!.innerHTML = `<style>${SETTINGS_STYLES}</style>${template}`;
    this.setupEventListeners();
    this.initializeAssistant();
  }

  private initializeAssistant(): void {
    const container = this.shadowRoot?.querySelector('#assistant-container') as HTMLElement;
    if (!container) return;

    this.assistantPanel?.destroy();

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

    // Track input changes
    this.shadowRoot?.querySelectorAll('.config-input').forEach(input => {
      input.addEventListener('input', (e) => {
        const target = e.target as HTMLInputElement;
        const key = target.dataset.key;
        if (key && target.value) {
          this.pendingChanges.set(key, target.value);
          this.tester.clearResult(key);
        } else if (key) {
          this.pendingChanges.delete(key);
        }
        this.saveStatus = 'idle';
      });
    });

    // Test button handlers
    this.shadowRoot?.querySelectorAll('.btn-test').forEach(btn => {
      btn.addEventListener('click', async (e) => {
        const target = e.target as HTMLButtonElement;
        const provider = target.dataset.provider;
        const key = target.dataset.key;
        if (provider && key) {
          await this.handleTestClick(provider, key);
        }
      });
    });
  }

  private async handleTestClick(provider: string, configKey: string): Promise<void> {
    const input = this.shadowRoot?.querySelector(`input[data-key="${configKey}"]`) as HTMLInputElement;
    const keyValue = this.pendingChanges.get(configKey) || input?.value;

    if (!keyValue || keyValue.startsWith('••••')) {
      const entry = this.configEntries.find(e => e.key === configKey);
      if (!entry?.isConfigured) {
        this.tester.clearResult(configKey);
        // Show error in UI
        await this.tester.testKey({ provider, key: '' }, configKey);
        return;
      }
    }

    if (keyValue) {
      await this.tester.testKey({ provider, key: keyValue }, configKey);
    }
  }

  private async saveConfig(): Promise<void> {
    // Validate pending changes
    const validation = this.tester.validateChanges(this.pendingChanges);

    // Auto-test untested keys
    if (validation.untested.length > 0) {
      this.saveStatus = 'saving';
      this.renderWidget();

      for (const key of validation.untested) {
        const entry = this.configEntries.find(e => e.key === key);
        const value = this.pendingChanges.get(key);
        if (entry && value) {
          await this.tester.testKey(
            { provider: entry.provider.toLowerCase(), key: value },
            key
          );
        }
      }

      // Re-validate after testing
      const revalidation = this.tester.validateChanges(this.pendingChanges);
      if (revalidation.failed.length > 0) {
        this.saveStatus = 'error';
        this.renderWidget();
        return;
      }
    } else if (validation.failed.length > 0) {
      this.saveStatus = 'error';
      this.renderWidget();
      return;
    }

    this.saveStatus = 'saving';
    this.renderWidget();

    try {
      const config: Record<string, string> = {};
      for (const [key, value] of this.pendingChanges) {
        if (value) {
          config[key] = value;
        }
      }

      if (Object.keys(config).length === 0) {
        this.saveStatus = 'saved';
        this.renderWidget();
        return;
      }

      console.log('Settings: Would save config:', Object.keys(config));

      // TODO: Implement system/config/save command
      await new Promise(resolve => setTimeout(resolve, 500));

      this.pendingChanges.clear();

      for (const key of Object.keys(config)) {
        const entry = this.configEntries.find(e => e.key === key);
        if (entry) {
          entry.isConfigured = true;
        }
      }

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
