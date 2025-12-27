/**
 * SettingsWidget - Configuration editor with AI assistance
 *
 * Allows users to view and edit API keys and other settings.
 * Includes embedded AI assistant for help with configuration.
 * Changes are persisted to ~/.continuum/config.env
 */

import { BaseWidget } from '../shared/BaseWidget';
import { Commands } from '../../system/core/shared/Commands';
import { styles as SETTINGS_STYLES } from './styles/settings.styles';
import { ProviderEntry, type ConfigEntry } from './components/ProviderEntry';
import { ProviderStatusTester } from './components/ProviderStatusTester';
import { PositronWidgetState } from '../shared/services/state/PositronWidgetState';

type SettingsSection = 'providers' | 'appearance' | 'account' | 'about';

export class SettingsWidget extends BaseWidget {
  private configEntries: ConfigEntry[] = [];
  private isLoading = true;
  private saveStatus: 'idle' | 'saving' | 'saved' | 'error' = 'idle';
  private tester: ProviderStatusTester;
  private pendingChanges: Map<string, string> = new Map();
  private currentSection: SettingsSection = 'providers';

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
    this.emitPositronContext();
  }

  /**
   * Emit Positron context for AI awareness
   * Called on init and section changes
   */
  private emitPositronContext(): void {
    const sectionTitles: Record<SettingsSection, string> = {
      'providers': 'AI Providers',
      'appearance': 'Appearance',
      'account': 'Account',
      'about': 'About'
    };

    PositronWidgetState.emit(
      {
        widgetType: 'settings',
        section: this.currentSection,
        title: `Settings - ${sectionTitles[this.currentSection]}`,
        metadata: {
          configuredProviders: this.configEntries.filter(e => e.isConfigured).length,
          totalProviders: this.configEntries.length,
          hasPendingChanges: this.pendingChanges.size > 0
        }
      },
      { action: 'configuring', target: sectionTitles[this.currentSection] }
    );
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
          billingUrl: p.billingUrl,
          maskedKey: p.maskedKey
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
    // Preserve scroll position before re-render
    const scrollContainer = this.shadowRoot?.querySelector('.settings-main');
    const scrollTop = scrollContainer?.scrollTop || 0;

    const navHtml = this.renderNav();
    const contentHtml = this.isLoading
      ? `<div class="loading">Loading configuration...</div>`
      : this.renderSectionContent();

    const template = `
      <div class="settings-layout">
        <nav class="settings-nav">
          ${navHtml}
        </nav>
        <main class="settings-main">
          ${contentHtml}
        </main>
      </div>
    `;

    this.shadowRoot!.innerHTML = `<style>${SETTINGS_STYLES}</style>${template}`;
    this.setupEventListeners();
    this.setupNavListeners();

    // Restore scroll position after re-render
    const newScrollContainer = this.shadowRoot?.querySelector('.settings-main');
    if (newScrollContainer && scrollTop > 0) {
      newScrollContainer.scrollTop = scrollTop;
    }
  }

  private renderNav(): string {
    const sections: { id: SettingsSection; icon: string; label: string }[] = [
      { id: 'providers', icon: '🤖', label: 'AI Providers' },
      { id: 'appearance', icon: '🎨', label: 'Appearance' },
      { id: 'account', icon: '👤', label: 'Account' },
      { id: 'about', icon: 'ℹ️', label: 'About' }
    ];

    return `
      <div class="nav-section">
        <h3 class="nav-section-title">Settings</h3>
        ${sections.map(s => `
          <div class="nav-item ${this.currentSection === s.id ? 'active' : ''}" data-section="${s.id}">
            <span class="nav-icon">${s.icon}</span>
            <span>${s.label}</span>
          </div>
        `).join('')}
      </div>
    `;
  }

  private renderSectionContent(): string {
    switch (this.currentSection) {
      case 'providers':
        return this.renderProvidersSection();
      case 'appearance':
        return this.renderAppearanceSection();
      case 'account':
        return this.renderAccountSection();
      case 'about':
        return this.renderAboutSection();
      default:
        return this.renderProvidersSection();
    }
  }

  private renderProvidersSection(): string {
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

    return `
      <div class="settings-header">
        <h1 class="settings-title">AI Providers</h1>
        <p class="settings-subtitle">Connect AI services to power your assistants</p>
      </div>

      <div class="info-box">
        <strong>Choose your setup:</strong> Run AI locally for free with Ollama,
        or connect cloud providers for more powerful models. You can use multiple providers.
        <span class="storage-note">Keys stored in <code>~/.continuum/config.env</code> <button class="btn-refresh" id="refresh-btn" title="Reload from file">↻</button></span>
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
        ${this.saveStatus === 'error' ? '<span class="status-message status-error">Fix errors before saving</span>' : ''}
        <button class="btn btn-secondary" id="reset-btn">Reset</button>
        <button class="btn btn-primary" id="save-btn">Save Changes</button>
      </div>
    `;
  }

  private renderAppearanceSection(): string {
    return `
      <div class="settings-header">
        <h1 class="settings-title">Appearance</h1>
        <p class="settings-subtitle">Customize the look and feel</p>
      </div>

      <div class="settings-section">
        <h2 class="section-title">Theme</h2>
        <p class="section-intro">
          Theme customization coming soon. Currently using the default dark theme.
        </p>
      </div>
    `;
  }

  private renderAccountSection(): string {
    return `
      <div class="settings-header">
        <h1 class="settings-title">Account</h1>
        <p class="settings-subtitle">Manage your profile and preferences</p>
      </div>

      <div class="settings-section">
        <h2 class="section-title">Profile</h2>
        <p class="section-intro">
          Account settings coming soon.
        </p>
      </div>
    `;
  }

  private renderAboutSection(): string {
    return `
      <div class="settings-header">
        <h1 class="settings-title">About</h1>
        <p class="settings-subtitle">Continuum JTAG</p>
      </div>

      <div class="settings-section">
        <h2 class="section-title">Version</h2>
        <p class="section-intro">
          Version information and credits coming soon.
        </p>
      </div>
    `;
  }

  private setupNavListeners(): void {
    this.shadowRoot?.querySelectorAll('.nav-item').forEach(item => {
      item.addEventListener('click', (e) => {
        const target = e.currentTarget as HTMLElement;
        const section = target.dataset.section as SettingsSection;
        if (section && section !== this.currentSection) {
          this.currentSection = section;
          this.renderWidget();
          this.emitPositronContext();  // Notify AIs of section change
        }
      });
    });
  }

  private setupEventListeners(): void {
    const saveBtn = this.shadowRoot?.querySelector('#save-btn');
    const resetBtn = this.shadowRoot?.querySelector('#reset-btn');
    const refreshBtn = this.shadowRoot?.querySelector('#refresh-btn');

    saveBtn?.addEventListener('click', () => this.saveConfig());
    resetBtn?.addEventListener('click', () => this.loadConfig());
    refreshBtn?.addEventListener('click', () => this.loadConfig());

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
    const newValue = this.pendingChanges.get(configKey) || input?.value;
    const entry = this.configEntries.find(e => e.key === configKey);

    // If user entered a new value, test that
    if (newValue && !newValue.startsWith('sk-...') && !newValue.startsWith('gsk_...')) {
      console.log(`Testing new key for ${configKey}`);
      await this.tester.testKey({ provider, key: newValue }, configKey);
      return;
    }

    // If already configured, test the stored key (pass empty to use server-side key)
    if (entry?.isConfigured) {
      console.log(`Testing stored key for ${configKey}`);
      await this.tester.testKey({ provider, key: '', useStored: true } as any, configKey);
      return;
    }

    // Not configured and no new value - show error
    console.log(`No key to test for ${configKey}`);
    await this.tester.testKey({ provider, key: '' }, configKey);
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
    console.log('Settings: Cleanup complete');
  }
}

// Registration handled by centralized BROWSER_WIDGETS registry
