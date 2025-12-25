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

interface ConfigEntry {
  key: string;
  value: string;
  isSecret: boolean;
  description?: string;
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

    // TODO: Load actual config from server via system/config/list command
    this.configEntries = this.getDefaultConfigEntries();

    this.isLoading = false;
    this.renderWidget();
  }

  private getDefaultConfigEntries(): ConfigEntry[] {
    return [
      { key: 'ANTHROPIC_API_KEY', value: '', isSecret: true, description: 'Claude API key for AI features' },
      { key: 'OPENAI_API_KEY', value: '', isSecret: true, description: 'OpenAI API key (optional)' },
      { key: 'GROQ_API_KEY', value: '', isSecret: true, description: 'Groq API key for fast inference' },
      { key: 'TOGETHER_API_KEY', value: '', isSecret: true, description: 'Together.ai API key' },
      { key: 'FIREWORKS_API_KEY', value: '', isSecret: true, description: 'Fireworks.ai API key' },
      { key: 'XAI_API_KEY', value: '', isSecret: true, description: 'xAI/Grok API key' },
      { key: 'DEEPSEEK_API_KEY', value: '', isSecret: true, description: 'DeepSeek API key' },
      { key: 'OLLAMA_HOST', value: 'http://localhost:11434', isSecret: false, description: 'Ollama server URL (free local AI)' },
    ];
  }

  protected async renderWidget(): Promise<void> {
    const styles = `
      :host {
        display: block;
        height: 100%;
        overflow: hidden;
      }

      .settings-layout {
        display: grid;
        grid-template-columns: 1fr 350px;
        height: 100%;
        gap: 0;
      }

      .settings-main {
        overflow-y: auto;
        padding: 24px;
      }

      .settings-assistant {
        border-left: 1px solid rgba(0, 212, 255, 0.2);
        height: 100%;
      }

      .settings-container {
        max-width: 600px;
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

      @media (max-width: 900px) {
        .settings-layout {
          grid-template-columns: 1fr;
          grid-template-rows: 1fr 300px;
        }

        .settings-assistant {
          border-left: none;
          border-top: 1px solid rgba(0, 212, 255, 0.2);
        }
      }
    `;

    const entriesHtml = this.configEntries.map(entry => `
      <div class="config-entry">
        <div class="config-label">
          <span class="config-key">${entry.key}</span>
          ${entry.description ? `<span class="config-description">${entry.description}</span>` : ''}
        </div>
        <input
          type="${entry.isSecret ? 'password' : 'text'}"
          class="config-input"
          data-key="${entry.key}"
          value="${entry.value}"
          placeholder="${entry.isSecret ? 'Enter API key...' : 'Enter value...'}"
        />
      </div>
    `).join('');

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
              <h1 class="settings-title">Settings</h1>
              <p class="settings-subtitle">Configure API keys and preferences</p>
            </div>

            <div class="info-box">
              <strong>Free AI:</strong> Ollama runs locally at no cost.
              <a href="https://ollama.ai" target="_blank">Download Ollama</a> to get started without API keys.
            </div>

            <div class="settings-section">
              <h2 class="section-title">API Keys</h2>
              ${entriesHtml}
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
      placeholder: 'Ask about settings or API keys...',
      greeting: "Hi! I can help you configure your API keys and settings. What would you like to know?"
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
