/**
 * SettingsAssistant - AI-powered help for Settings configuration
 *
 * Subscribes to SettingsWidget events and provides proactive assistance:
 * - When provider tests fail, offers troubleshooting tips
 * - When config save fails, diagnoses the issue
 * - Suggests next steps based on current configuration
 *
 * Uses ReactiveWidget with Lit templates for efficient rendering.
 */

import {
  ReactiveWidget,
  html,
  reactive,
  unsafeCSS,
  type TemplateResult,
  type CSSResultGroup
} from '../shared/ReactiveWidget';
import { PositronWidgetState } from '../shared/services/state/PositronWidgetState';
import { Commands } from '../../system/core/shared/Commands';

import { AIGenerate, type AIGenerateParams, type AIGenerateResult } from '../../commands/ai/generate/shared/AIGenerateTypes';
import type { CommandInput } from '../../system/core/types/JTAGTypes';
interface ProviderTestedEvent {
  provider: string;
  configKey: string;
  success: boolean;
  status: string;
  message: string | null;
  responseTimeMs?: number;
  needsHelp: boolean;
}

interface ConfigErrorEvent {
  error: string;
  needsHelp: boolean;
}

interface AssistantMessage {
  type: 'info' | 'success' | 'error' | 'help';
  text: string;
  timestamp: number;
}

export class SettingsAssistantWidget extends ReactiveWidget {
  // Static styles
  static override styles = [
    ReactiveWidget.styles,
    unsafeCSS(`
      :host {
        display: block;
        height: 100%;
        background: var(--surface-background, rgba(15, 20, 25, 0.95));
        color: var(--content-primary, #e0e0e0);
        font-family: system-ui, -apple-system, sans-serif;
      }
      .assistant-container {
        padding: 16px;
        display: flex;
        flex-direction: column;
        gap: 12px;
        height: 100%;
        overflow-y: auto;
      }
      .assistant-header {
        font-size: 14px;
        font-weight: 600;
        color: var(--content-accent, #00d4ff);
        margin-bottom: 8px;
      }
      .assistant-msg {
        padding: 10px 12px;
        border-radius: 8px;
        font-size: 13px;
        line-height: 1.4;
        animation: fadeIn 0.3s ease;
      }
      @keyframes fadeIn {
        from { opacity: 0; transform: translateY(5px); }
        to { opacity: 1; transform: translateY(0); }
      }
      .msg-info {
        background: rgba(0, 212, 255, 0.1);
        border-left: 3px solid var(--content-accent, #00d4ff);
      }
      .msg-success {
        background: rgba(0, 255, 100, 0.1);
        border-left: 3px solid #00ff64;
      }
      .msg-error {
        background: rgba(255, 80, 80, 0.1);
        border-left: 3px solid #ff5050;
      }
      .msg-help {
        background: rgba(255, 200, 0, 0.15);
        border-left: 3px solid #ffc800;
      }
    `)
  ] as CSSResultGroup;

  // Reactive state
  @reactive() private messages: AssistantMessage[] = [];

  // Non-reactive
  private isGenerating = false;

  constructor() {
    super({
      widgetName: 'SettingsAssistantWidget'
    });
  }

  protected override async onFirstRender(): Promise<void> {
    super.onFirstRender();

    // Subscribe to settings events
    this.createMountEffect(() => {
      const unsubs = [
        PositronWidgetState.subscribeToWidget('settings', 'provider:tested', (data) => {
          this.handleProviderTested(data as ProviderTestedEvent);
        }),
        PositronWidgetState.subscribeToWidget('settings', 'config:error', (data) => {
          this.handleConfigError(data as ConfigErrorEvent);
        }),
        PositronWidgetState.subscribeToWidget('settings', 'config:saved', () => {
          this.addMessage('success', '✅ Configuration saved successfully!');
        }),
        PositronWidgetState.subscribeToWidget('settings', 'section:changed', (data: any) => {
          if (data.section === 'providers') {
            this.addMessage('info', '💡 Tip: Test your API keys before saving to verify they work.');
          }
        })
      ];
      return () => unsubs.forEach(u => u());
    });

    // Initial greeting
    this.addMessage('info', '👋 I\'m here to help you configure your AI providers. Click "Test" on any provider and I\'ll help troubleshoot any issues.');
  }

  // === Render ===

  protected override renderContent(): TemplateResult {
    return html`
      <div class="assistant-container">
        <div class="assistant-header">🤖 Settings Assistant</div>
        ${this.messages.length > 0
          ? this.messages.map(msg => this.renderMessage(msg))
          : html`<div class="assistant-msg msg-info">Ready to help with your configuration.</div>`
        }
      </div>
    `;
  }

  private renderMessage(msg: AssistantMessage): TemplateResult {
    const iconClass = {
      'info': 'msg-info',
      'success': 'msg-success',
      'error': 'msg-error',
      'help': 'msg-help'
    }[msg.type];

    return html`<div class="assistant-msg ${iconClass}">${msg.text}</div>`;
  }

  // === Message Management ===

  private addMessage(type: 'info' | 'success' | 'error' | 'help', text: string): void {
    // Create new array for reactivity
    this.messages = [...this.messages, { type, text, timestamp: Date.now() }];
    // Keep last 10 messages
    if (this.messages.length > 10) {
      this.messages = this.messages.slice(-10);
    }
    this.requestUpdate();
  }

  // === Event Handlers ===

  private async handleProviderTested(data: ProviderTestedEvent): Promise<void> {
    const { provider, success, status, message, responseTimeMs } = data;

    if (success) {
      this.addMessage('success', `✅ ${provider} is working! Response time: ${responseTimeMs}ms`);
      return;
    }

    // Provider test failed - offer help
    this.addMessage('error', `❌ ${provider} test failed: ${message || status}`);

    if (data.needsHelp && !this.isGenerating) {
      await this.generateHelp(provider, status, message);
    }
  }

  private async handleConfigError(data: ConfigErrorEvent): Promise<void> {
    this.addMessage('error', `❌ Failed to save: ${data.error}`);

    if (data.needsHelp && !this.isGenerating) {
      this.addMessage('help', '🔧 Check that ~/.continuum/config.env is writable and try again.');
    }
  }

  private async generateHelp(provider: string, status: string, errorMessage: string | null): Promise<void> {
    this.isGenerating = true;
    this.addMessage('info', '🤔 Analyzing the issue...');

    try {
      // Use ai/generate to create helpful response
      const result = await AIGenerate.execute({
        prompt: `The user is trying to configure ${provider} API in their settings. The test failed with status "${status}" and message: "${errorMessage || 'No details'}".

Give a brief, helpful troubleshooting tip (2-3 sentences max). Focus on the most likely cause and solution. Be friendly and concise.`,
        maxTokens: 150
      } as unknown as CommandInput<AIGenerateParams>) as AIGenerateResult;

      if (result?.text) {
        this.addMessage('help', `💡 ${result.text}`);
      } else {
        this.addMessage('help', this.getFallbackHelp(provider, status));
      }
    } catch (error) {
      this.addMessage('help', this.getFallbackHelp(provider, status));
    }

    this.isGenerating = false;
  }

  private getFallbackHelp(provider: string, status: string): string {
    const tips: Record<string, string> = {
      'invalid': `🔑 Check that your ${provider} API key is correct. Make sure you copied the full key without extra spaces.`,
      'out-of-funds': `💳 Your ${provider} account may need more credits. Check your billing dashboard.`,
      'rate-limited': `⏱️ ${provider} is rate limiting requests. Wait a minute and try again.`,
      'error': `🔌 Could not connect to ${provider}. Check your internet connection or the service status.`
    };
    return tips[status] || `Check your ${provider} API key and try again. Visit their docs for help.`;
  }
}

// Registration handled by centralized BROWSER_WIDGETS registry
