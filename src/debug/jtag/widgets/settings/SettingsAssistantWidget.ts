/**
 * SettingsAssistant - AI-powered help for Settings configuration
 *
 * Subscribes to SettingsWidget events and provides proactive assistance:
 * - When provider tests fail, offers troubleshooting tips
 * - When config save fails, diagnoses the issue
 * - Suggests next steps based on current configuration
 *
 * Uses the reactive widget pattern via PositronWidgetState.subscribeToWidget()
 */

import { BaseWidget } from '../shared/BaseWidget';
import { PositronWidgetState } from '../shared/services/state/PositronWidgetState';
import { Commands } from '../../system/core/shared/Commands';

interface ProviderTestedEvent {
  provider: string;
  configKey: string;
  success: boolean;
  status: string;
  message: string | null;
  responseTime?: number;
  needsHelp: boolean;
}

interface ConfigErrorEvent {
  error: string;
  needsHelp: boolean;
}

export class SettingsAssistantWidget extends BaseWidget {
  private unsubscribers: Array<() => void> = [];
  private messages: Array<{ type: 'info' | 'success' | 'error' | 'help'; text: string; timestamp: number }> = [];
  private isGenerating = false;

  constructor() {
    super({
      widgetName: 'SettingsAssistantWidget',
      enableAI: true,
      enableDatabase: false
    });
  }

  protected async onWidgetInitialize(): Promise<void> {
    console.log('🤖 SettingsAssistant: Initializing...');

    // Subscribe to settings events
    this.unsubscribers.push(
      PositronWidgetState.subscribeToWidget('settings', 'provider:tested', (data) => {
        this.handleProviderTested(data as ProviderTestedEvent);
      })
    );

    this.unsubscribers.push(
      PositronWidgetState.subscribeToWidget('settings', 'config:error', (data) => {
        this.handleConfigError(data as ConfigErrorEvent);
      })
    );

    this.unsubscribers.push(
      PositronWidgetState.subscribeToWidget('settings', 'config:saved', () => {
        this.addMessage('success', '✅ Configuration saved successfully!');
      })
    );

    this.unsubscribers.push(
      PositronWidgetState.subscribeToWidget('settings', 'section:changed', (data: any) => {
        if (data.section === 'providers') {
          this.addMessage('info', '💡 Tip: Test your API keys before saving to verify they work.');
        }
      })
    );

    // Initial greeting
    this.addMessage('info', '👋 I\'m here to help you configure your AI providers. Click "Test" on any provider and I\'ll help troubleshoot any issues.');

    this.renderWidget();
    console.log('🤖 SettingsAssistant: Subscribed to settings events');
  }

  private addMessage(type: 'info' | 'success' | 'error' | 'help', text: string): void {
    this.messages.push({ type, text, timestamp: Date.now() });
    // Keep last 10 messages
    if (this.messages.length > 10) {
      this.messages.shift();
    }
    this.renderWidget();
  }

  private async handleProviderTested(data: ProviderTestedEvent): Promise<void> {
    const { provider, success, status, message, responseTime } = data;

    if (success) {
      this.addMessage('success', `✅ ${provider} is working! Response time: ${responseTime}ms`);
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
      const result = await Commands.execute('ai/generate', {
        prompt: `The user is trying to configure ${provider} API in their settings. The test failed with status "${status}" and message: "${errorMessage || 'No details'}".

Give a brief, helpful troubleshooting tip (2-3 sentences max). Focus on the most likely cause and solution. Be friendly and concise.`,
        maxTokens: 150
      } as any) as any;

      if (result?.text) {
        this.addMessage('help', `💡 ${result.text}`);
      } else {
        // Fallback help based on status
        this.addMessage('help', this.getFallbackHelp(provider, status));
      }
    } catch (error) {
      // Fallback help if AI generation fails
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

  protected async renderWidget(): Promise<void> {
    const messagesHtml = this.messages.map(msg => {
      const iconClass = {
        'info': 'msg-info',
        'success': 'msg-success',
        'error': 'msg-error',
        'help': 'msg-help'
      }[msg.type];

      return `<div class="assistant-msg ${iconClass}">${msg.text}</div>`;
    }).join('');

    this.shadowRoot!.innerHTML = `
      <style>
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
      </style>
      <div class="assistant-container">
        <div class="assistant-header">🤖 Settings Assistant</div>
        ${messagesHtml || '<div class="assistant-msg msg-info">Ready to help with your configuration.</div>'}
      </div>
    `;
  }

  async disconnectedCallback(): Promise<void> {
    // Clean up all subscriptions
    for (const unsub of this.unsubscribers) {
      unsub();
    }
    this.unsubscribers = [];
    console.log('🤖 SettingsAssistant: Cleaned up subscriptions');

    await super.disconnectedCallback();
  }

  protected async onWidgetCleanup(): Promise<void> {
    // Cleanup handled in disconnectedCallback
  }
}

// Register custom element
// Registration handled by centralized BROWSER_WIDGETS registry
