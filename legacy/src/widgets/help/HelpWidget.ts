/**
 * HelpWidget - Onboarding and documentation
 *
 * MIGRATED TO ReactiveWidget:
 * - Lit's reactive properties
 * - Declarative templates
 * - Automatic DOM diffing
 */

import { ReactiveWidget, html, css, reactive, type TemplateResult } from '../shared/ReactiveWidget';
import { PositronWidgetState } from '../shared/services/state/PositronWidgetState';

interface HelpSection {
  id: string;
  title: string;
  icon: string;
  content: TemplateResult;
}

export class HelpWidget extends ReactiveWidget {
  @reactive() private activeSection: string = 'getting-started';

  static override styles = css`
    :host {
      display: block;
      height: 100%;
    }

    .help-layout {
      display: grid;
      grid-template-columns: 200px 1fr;
      height: 100%;
      gap: var(--spacing-md, 12px);
      padding: var(--spacing-md, 12px);
    }

    .help-nav {
      display: flex;
      flex-direction: column;
      gap: var(--spacing-xs, 4px);
      border-right: 1px solid var(--border-color, #333);
      padding-right: var(--spacing-md, 12px);
    }

    .nav-item {
      display: flex;
      align-items: center;
      gap: var(--spacing-sm, 8px);
      padding: var(--spacing-sm, 8px) var(--spacing-md, 12px);
      border-radius: var(--border-radius-sm, 4px);
      cursor: pointer;
      transition: background-color 0.15s ease;
      color: var(--text-secondary, #aaa);
    }

    .nav-item:hover {
      background-color: var(--hover-background, rgba(255, 255, 255, 0.05));
    }

    .nav-item.active {
      background-color: var(--active-background, rgba(0, 200, 255, 0.1));
      color: var(--text-primary, #fff);
    }

    .nav-icon {
      width: 20px;
      height: 20px;
      border-radius: 50%;
      background: var(--accent-color, #00c8ff);
      color: var(--background-color, #1a1a2e);
      display: flex;
      align-items: center;
      justify-content: center;
      font-size: 0.75em;
      font-weight: bold;
    }

    .help-content {
      overflow-y: auto;
      padding: var(--spacing-md, 12px);
    }

    .help-content h3 {
      margin-top: 0;
      color: var(--text-primary, #fff);
    }

    .help-content h4 {
      color: var(--accent-color, #00c8ff);
      margin-top: 1.5em;
    }

    .help-content p {
      color: var(--text-secondary, #aaa);
      line-height: 1.6;
    }

    .help-content ol, .help-content ul {
      color: var(--text-secondary, #aaa);
      padding-left: 1.5em;
    }

    .help-content li {
      margin-bottom: 0.5em;
    }

    .help-content code {
      background: var(--code-background, rgba(0, 200, 255, 0.1));
      padding: 0.2em 0.4em;
      border-radius: 3px;
      font-family: monospace;
    }

    .help-content a {
      color: var(--accent-color, #00c8ff);
    }

    .help-content table {
      width: 100%;
      border-collapse: collapse;
    }

    .help-content td {
      padding: 0.5em;
      border-bottom: 1px solid var(--border-color, #333);
    }

    .help-content td:first-child {
      width: 150px;
    }
  `;

  override connectedCallback(): void {
    super.connectedCallback();
    console.log('Help: Initializing help widget...');
    this.emitPositronContext();
  }

  private emitPositronContext(): void {
    // NOTE: Removed PositronWidgetState.emit() - MainWidget handles context
    // Widgets should RECEIVE state, not emit it (avoid cascade)
  }

  private getSections(): HelpSection[] {
    return [
      {
        id: 'getting-started',
        title: 'Getting Started',
        icon: '1',
        content: html`
          <h3>Welcome to Continuum</h3>
          <p>Continuum is a collaborative AI workspace where you can chat with multiple AI models,
          configure your environment, and work together with AI assistants.</p>

          <h4>Quick Start</h4>
          <ol>
            <li><strong>Chat</strong> - Click on any room in the sidebar to start chatting</li>
            <li><strong>AI Models</strong> - Multiple AI assistants are available to help</li>
            <li><strong>Settings</strong> - Add API keys to enable cloud AI providers</li>
            <li><strong>Free AI</strong> - Candle provides free local AI with no API keys needed</li>
          </ol>
        `
      },
      {
        id: 'local-ai',
        title: 'Free Local AI with Candle',
        icon: '2',
        content: html`
          <h3>Local AI - No API Keys Required</h3>
          <p>Candle runs AI models locally on your machine via native Rust inference, completely free.</p>

          <h4>How It Works</h4>
          <p>Continuum includes a built-in Candle inference engine that automatically downloads and runs
          HuggingFace models locally. No external dependencies needed.</p>

          <h4>Available Models</h4>
          <ul>
            <li><strong>Llama 3.1 8B</strong> - General chat and reasoning (4GB)</li>
            <li><strong>Qwen2 1.5B</strong> - Fast, good for classification (1.5GB)</li>
            <li><strong>SmolLM2 135M</strong> - Ultra-light for LoRA training (270MB)</li>
          </ul>
        `
      },
      {
        id: 'api-keys',
        title: 'API Keys',
        icon: '3',
        content: html`
          <h3>Cloud AI Providers</h3>
          <p>For more powerful AI models, add API keys in Settings.</p>

          <h4>Supported Providers</h4>
          <ul>
            <li><strong>Anthropic (Claude)</strong> - Best for complex reasoning</li>
            <li><strong>OpenAI (GPT)</strong> - General purpose AI</li>
            <li><strong>Groq</strong> - Extremely fast inference</li>
            <li><strong>Together.ai</strong> - Open source models</li>
            <li><strong>DeepSeek</strong> - Cost-effective coding</li>
            <li><strong>xAI (Grok)</strong> - Latest AI research</li>
          </ul>

          <p>Go to <strong>Settings</strong> to add your API keys.</p>
        `
      },
      {
        id: 'chat-rooms',
        title: 'Chat Rooms',
        icon: '4',
        content: html`
          <h3>Collaborative Spaces</h3>
          <p>Chat rooms are collaborative spaces where you and AI assistants can work together.</p>

          <h4>Default Rooms</h4>
          <ul>
            <li><strong>General</strong> - Open discussion for any topic</li>
            <li><strong>Academy</strong> - Learning and tutorials</li>
            <li><strong>Dev Updates</strong> - Development activity feed</li>
            <li><strong>Pantheon</strong> - Advanced multi-model reasoning</li>
          </ul>

          <h4>Tips</h4>
          <ul>
            <li>Click a room in the sidebar to open it</li>
            <li>Multiple rooms can be open as tabs</li>
            <li>AI assistants respond automatically</li>
          </ul>
        `
      },
      {
        id: 'keyboard',
        title: 'Keyboard Shortcuts',
        icon: '5',
        content: html`
          <h3>Keyboard Shortcuts</h3>
          <table>
            <tr><td><code>Enter</code></td><td>Send message</td></tr>
            <tr><td><code>Shift + Enter</code></td><td>New line in message</td></tr>
            <tr><td><code>Ctrl/Cmd + K</code></td><td>Quick command palette</td></tr>
            <tr><td><code>Ctrl/Cmd + ,</code></td><td>Open settings</td></tr>
            <tr><td><code>Ctrl/Cmd + 1-9</code></td><td>Switch to tab N</td></tr>
          </table>
        `
      }
    ];
  }

  private handleSectionClick(sectionId: string): void {
    if (sectionId !== this.activeSection) {
      this.activeSection = sectionId;
      this.emitPositronContext();
    }
  }

  private renderNavItem(section: HelpSection): TemplateResult {
    return html`
      <div
        class="nav-item ${section.id === this.activeSection ? 'active' : ''}"
        @click=${() => this.handleSectionClick(section.id)}
      >
        <span class="nav-icon">${section.icon}</span>
        <span>${section.title}</span>
      </div>
    `;
  }

  override render(): TemplateResult {
    const sections = this.getSections();
    const activeContent = sections.find(s => s.id === this.activeSection)?.content;

    return html`
      <div class="help-layout">
        <div class="help-nav">
          ${sections.map(s => this.renderNavItem(s))}
        </div>
        <div class="help-content">
          ${activeContent}
        </div>
      </div>
    `;
  }
}

// Registration handled by centralized BROWSER_WIDGETS registry
