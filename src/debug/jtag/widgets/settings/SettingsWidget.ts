/**
 * SettingsWidget - Configuration editor with AI assistance
 *
 * Allows users to view and edit API keys and other settings.
 * Uses ReactiveWidget with Lit templates for efficient rendering.
 * Changes are persisted to ~/.continuum/config.env
 */

import {
  ReactiveWidget,
  html,
  reactive,
  unsafeCSS,
  type TemplateResult,
  type CSSResultGroup
} from '../shared/ReactiveWidget';
import { Events } from '../../system/core/shared/Events';
import type { AIProvidersStatusParams, AIProvidersStatusResult } from '../../commands/ai/providers/status/shared/AIProvidersStatusTypes';
import { styles as SETTINGS_STYLES } from './styles/settings.styles';
import type { ConfigEntry } from './components/ProviderEntry';
import { ProviderStatusTester, type ProviderTestResult } from './components/ProviderStatusTester';
import { PositronWidgetState } from '../shared/services/state/PositronWidgetState';
import { SETTINGS_NAV_EVENTS, type SettingsSection, type SettingsSectionChangedPayload } from '../settings-nav/SettingsNavWidget';
// Import ProvidersSection Lit component
import './components/providers-section/ProvidersSection';

export class SettingsWidget extends ReactiveWidget {
  // Static styles using SCSS compiled to CSS string
  static override styles = [
    ReactiveWidget.styles,
    unsafeCSS(SETTINGS_STYLES)
  ] as CSSResultGroup;

  // Reactive state - changes automatically trigger re-render
  @reactive() private configEntries: ConfigEntry[] = [];
  @reactive() private isLoading = true;
  @reactive() private saveStatus: 'idle' | 'saving' | 'saved' | 'error' = 'idle';
  @reactive() private currentSection: SettingsSection = 'providers';
  @reactive() private testResults: Map<string, ProviderTestResult> = new Map();

  // Non-reactive state (doesn't need to trigger re-renders directly)
  private tester: ProviderStatusTester;
  private pendingChanges: Map<string, string> = new Map();
  private lastTestResult: {
    provider: string;
    success: boolean;
    status: string;
    message: string | null;
    testedAt: number;
  } | null = null;

  constructor() {
    super({ widgetName: 'SettingsWidget' });

    // Initialize tester with callback to update testResults (triggers re-render)
    this.tester = new ProviderStatusTester(() => {
      // Create new Map to trigger reactivity
      this.testResults = new Map(
        this.configEntries
          .map(entry => [entry.key, this.tester.getResult(entry.key)] as const)
          .filter(([_, result]) => result !== undefined) as [string, ProviderTestResult][]
      );
    });
  }

  // === LIFECYCLE ===

  protected override onFirstRender(): void {
    super.onFirstRender();

    // Subscribe to section changes from SettingsNavWidget
    this.createMountEffect(() => {
      const unsubscribe = Events.subscribe(SETTINGS_NAV_EVENTS.SECTION_CHANGED, (payload: SettingsSectionChangedPayload) => {
        if (payload.section !== this.currentSection) {
          this.log(`Section changed to ${payload.section}`);
          this.currentSection = payload.section;
          this.emitPositronContext();

          PositronWidgetState.emitWidgetEvent('settings', 'section:changed', {
            section: payload.section,
            previousSection: this.currentSection
          });
        }
      });
      return () => unsubscribe();
    });

    // Load config on mount
    this.loadConfig();
  }

  // === RENDER ===

  protected override renderContent(): TemplateResult {
    if (this.isLoading) {
      return html`
        <div class="settings-layout settings-layout--no-nav">
          <main class="settings-main">
            <div class="loading">Loading configuration...</div>
          </main>
        </div>
      `;
    }

    return html`
      <div class="settings-layout settings-layout--no-nav">
        <main class="settings-main">
          ${this.renderSectionContent()}
        </main>
      </div>
    `;
  }

  private renderSectionContent(): TemplateResult {
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

  private renderProvidersSection(): TemplateResult {
    return html`
      <providers-section
        .entries=${this.configEntries}
        .testResults=${this.testResults}
        .pendingChanges=${this.pendingChanges}
        .saveStatus=${this.saveStatus}
        @input-change=${this.handleInputChange}
        @test-click=${this.handleTestClick}
        @save-click=${this.handleSave}
        @reset-click=${this.handleReset}
        @refresh-click=${this.handleRefresh}
      ></providers-section>
    `;
  }

  private renderAppearanceSection(): TemplateResult {
    return html`
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

  private renderAccountSection(): TemplateResult {
    return html`
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

  private renderAboutSection(): TemplateResult {
    return html`
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

  // === EVENT HANDLERS ===

  private handleInputChange = (e: CustomEvent<{ key: string; value: string }>) => {
    const { key, value } = e.detail;
    if (value) {
      this.pendingChanges.set(key, value);
      this.tester.clearResult(key);
    } else {
      this.pendingChanges.delete(key);
    }
    this.saveStatus = 'idle';
    // Force update since pendingChanges is a Map (reference doesn't change)
    this.requestUpdate();
  };

  private handleTestClick = async (e: CustomEvent<{ provider: string; configKey: string }>) => {
    const { provider, configKey } = e.detail;
    const newValue = this.pendingChanges.get(configKey);
    const entry = this.configEntries.find(e => e.key === configKey);

    PositronWidgetState.emitWidgetEvent('settings', 'provider:testing', {
      provider,
      configKey,
      hasNewKey: !!newValue && !newValue.startsWith('sk-...'),
      isConfigured: entry?.isConfigured || false
    });

    // If user entered a new value, test that
    if (newValue && !newValue.startsWith('sk-...') && !newValue.startsWith('gsk_...')) {
      const result = await this.tester.testKey({ provider, key: newValue }, configKey);
      this.emitTestResult(provider, configKey, result);
      return;
    }

    // If already configured, test the stored key
    if (entry?.isConfigured) {
      const result = await this.tester.testKey({ provider, key: '', useStored: true } as any, configKey);
      this.emitTestResult(provider, configKey, result);
      return;
    }

    // Not configured and no new value
    const result = await this.tester.testKey({ provider, key: '' }, configKey);
    this.emitTestResult(provider, configKey, result);
  };

  private handleSave = () => {
    this.saveConfig();
  };

  private handleReset = () => {
    this.loadConfig();
  };

  private handleRefresh = () => {
    this.loadConfig();
  };

  // === DATA LOADING ===

  private async loadConfig(): Promise<void> {
    this.isLoading = true;

    try {
      // Use ReactiveWidget's executeCommand with proper types
      const result = await this.executeCommand<AIProvidersStatusParams, AIProvidersStatusResult>(
        'ai/providers/status',
        {}
      );

      if (result?.providers) {
        this.configEntries = result.providers.map((p) => ({
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
    this.testResults = new Map();
    this.emitPositronContext();

    // Force re-render - TODO: investigate why @reactive() doesn't auto-trigger
    this.requestUpdate();
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
      { key: 'DASHSCOPE_API_KEY', value: '', isSecret: true, provider: 'Alibaba', category: 'cloud', description: 'Qwen3-Omni - audio-native, open-source' },
    ];
  }

  // === SAVE CONFIG ===

  private async saveConfig(): Promise<void> {
    const validation = this.tester.validateChanges(this.pendingChanges);

    if (validation.untested.length > 0) {
      this.saveStatus = 'saving';

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

      const revalidation = this.tester.validateChanges(this.pendingChanges);
      if (revalidation.failed.length > 0) {
        this.saveStatus = 'error';
        return;
      }
    } else if (validation.failed.length > 0) {
      this.saveStatus = 'error';
      return;
    }

    this.saveStatus = 'saving';

    try {
      const config: Record<string, string> = {};
      for (const [key, value] of this.pendingChanges) {
        if (value) {
          config[key] = value;
        }
      }

      if (Object.keys(config).length === 0) {
        this.saveStatus = 'saved';
        return;
      }

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

      PositronWidgetState.emitWidgetEvent('settings', 'config:saved', {
        providers: Object.keys(config),
        success: true
      });
    } catch (error) {
      console.error('Settings: Failed to save config:', error);
      this.saveStatus = 'error';

      PositronWidgetState.emitWidgetEvent('settings', 'config:error', {
        error: String(error),
        needsHelp: true
      });
    }
  }

  // === POSITRON CONTEXT ===

  private emitPositronContext(): void {
    const metadata: Record<string, unknown> = {
      configuredProviders: this.configEntries.filter(e => e.isConfigured).length,
      totalProviders: this.configEntries.length,
      hasPendingChanges: this.pendingChanges.size > 0
    };

    if (this.lastTestResult) {
      const ageMs = Date.now() - this.lastTestResult.testedAt;
      const ageSeconds = Math.round(ageMs / 1000);

      if (ageMs < 5 * 60 * 1000) {
        metadata.lastTestedProvider = this.lastTestResult.provider;
        metadata.lastTestSuccess = this.lastTestResult.success;
        metadata.lastTestStatus = this.lastTestResult.status;
        metadata.lastTestMessage = this.lastTestResult.message;
        metadata.lastTestAgeSeconds = ageSeconds;

        if (!this.lastTestResult.success) {
          metadata.needsHelp = true;
          metadata.helpContext = `User just tested ${this.lastTestResult.provider} API key but it failed with: ${this.lastTestResult.message || this.lastTestResult.status}`;
        }
      }
    }
  }

  private emitTestResult(provider: string, configKey: string, _result: any): void {
    const testResult = this.tester.getResult(configKey);
    const success = testResult?.status === 'operational';
    const message = testResult?.message;

    this.lastTestResult = {
      provider,
      success,
      status: testResult?.status || 'unknown',
      message: message || null,
      testedAt: Date.now()
    };

    PositronWidgetState.emitWidgetEvent('settings', 'provider:tested', {
      provider,
      configKey,
      success,
      status: testResult?.status || 'unknown',
      message: message || null,
      responseTime: testResult?.responseTime,
      needsHelp: !success
    });

    this.emitPositronContext();
  }
}

// Register custom element
customElements.define('settings-widget', SettingsWidget);

// TypeScript declaration
declare global {
  interface HTMLElementTagNameMap {
    'settings-widget': SettingsWidget;
  }
}
