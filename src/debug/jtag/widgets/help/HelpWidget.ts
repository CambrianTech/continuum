/**
 * HelpWidget - Onboarding and documentation with AI assistance
 *
 * Helps new users get started with Continuum.
 * Includes embedded AI assistant for contextual help.
 */

import { BaseWidget } from '../shared/BaseWidget';
import { AssistantPanel } from '../shared/AssistantPanel';
import { DEFAULT_ROOMS } from '../../system/data/domains/DefaultEntities';
import type { UUID } from '../../system/core/types/CrossPlatformUUID';
import { PositronWidgetState } from '../shared/services/state/PositronWidgetState';

interface HelpSection {
  id: string;
  title: string;
  icon: string;
  content: string;
}

export class HelpWidget extends BaseWidget {
  private activeSection: string = 'getting-started';
  private assistantPanel?: AssistantPanel;

  constructor() {
    super({
      widgetName: 'HelpWidget',
      template: undefined,
      styles: undefined,
      enableAI: false,
      enableDatabase: false,
      enableRouterEvents: false,
      enableScreenshots: false
    });
  }

  protected async onWidgetInitialize(): Promise<void> {
    console.log('Help: Initializing help widget...');
    this.emitPositronContext();
  }

  /**
   * Emit Positron context for AI awareness
   */
  private emitPositronContext(): void {
    const sections = this.getSections();
    const currentSectionData = sections.find(s => s.id === this.activeSection);

    PositronWidgetState.emit(
      {
        widgetType: 'help',
        section: this.activeSection,
        title: `Help - ${currentSectionData?.title || 'Documentation'}`,
        metadata: {
          totalSections: sections.length,
          sectionTitles: sections.map(s => s.title)
        }
      },
      { action: 'viewing', target: currentSectionData?.title || 'help documentation' }
    );
  }

  private getSections(): HelpSection[] {
    return [
      {
        id: 'getting-started',
        title: 'Getting Started',
        icon: '1',
        content: `
          <h3>Welcome to Continuum</h3>
          <p>Continuum is a collaborative AI workspace where you can chat with multiple AI models,
          configure your environment, and work together with AI assistants.</p>

          <h4>Quick Start</h4>
          <ol>
            <li><strong>Chat</strong> - Click on any room in the sidebar to start chatting</li>
            <li><strong>AI Models</strong> - Multiple AI assistants are available to help</li>
            <li><strong>Settings</strong> - Add API keys to enable cloud AI providers</li>
            <li><strong>Free AI</strong> - Ollama provides free local AI with no API keys needed</li>
          </ol>
        `
      },
      {
        id: 'ollama',
        title: 'Free AI with Ollama',
        icon: '2',
        content: `
          <h3>Local AI - No API Keys Required</h3>
          <p>Ollama runs AI models locally on your machine, completely free.</p>

          <h4>Setup</h4>
          <ol>
            <li>Download Ollama from <a href="https://ollama.ai" target="_blank">ollama.ai</a></li>
            <li>Install and run Ollama</li>
            <li>Pull a model: <code>ollama pull llama3.2</code></li>
            <li>That's it! Local Assistant will now respond in chat</li>
          </ol>

          <h4>Recommended Models</h4>
          <ul>
            <li><strong>llama3.2:3b</strong> - Fast, good for general chat (2GB)</li>
            <li><strong>llama3.2:7b</strong> - Better quality (4GB)</li>
            <li><strong>codellama</strong> - Optimized for code</li>
          </ul>
        `
      },
      {
        id: 'api-keys',
        title: 'API Keys',
        icon: '3',
        content: `
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
        content: `
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
        content: `
          <h3>Keyboard Shortcuts</h3>
          <table style="width: 100%; border-collapse: collapse;">
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

  protected async renderWidget(): Promise<void> {
    const sections = this.getSections();
    const activeContent = sections.find(s => s.id === this.activeSection)?.content || '';

    const styles = `
      :host {
        display: block;
        height: 100%;
        overflow: hidden;
      }

      .help-layout {
        display: grid;
        grid-template-columns: 220px 1fr 350px;
        height: 100%;
      }

      .help-sidebar {
        background: rgba(10, 15, 20, 0.95);
        border-right: 1px solid rgba(0, 212, 255, 0.2);
        padding: 16px 0;
        overflow-y: auto;
      }

      .sidebar-title {
        padding: 0 16px 12px;
        font-size: 12px;
        text-transform: uppercase;
        color: rgba(255, 255, 255, 0.4);
        letter-spacing: 1px;
      }

      .nav-item {
        display: flex;
        align-items: center;
        gap: 12px;
        padding: 12px 16px;
        cursor: pointer;
        transition: all 0.15s ease;
        color: rgba(255, 255, 255, 0.7);
        font-size: 14px;
      }

      .nav-item:hover {
        background: rgba(0, 212, 255, 0.1);
        color: white;
      }

      .nav-item.active {
        background: rgba(0, 212, 255, 0.15);
        color: var(--content-accent, #00d4ff);
        border-left: 3px solid var(--content-accent, #00d4ff);
      }

      .nav-icon {
        width: 24px;
        height: 24px;
        display: flex;
        align-items: center;
        justify-content: center;
        background: rgba(0, 212, 255, 0.2);
        border-radius: 50%;
        font-size: 12px;
        font-weight: 600;
        color: var(--content-accent, #00d4ff);
      }

      .help-content {
        padding: 32px;
        overflow-y: auto;
      }

      .help-content h3 {
        font-size: 24px;
        color: var(--content-accent, #00d4ff);
        margin: 0 0 16px 0;
      }

      .help-content h4 {
        font-size: 16px;
        color: white;
        margin: 24px 0 12px 0;
      }

      .help-content p {
        color: rgba(255, 255, 255, 0.8);
        line-height: 1.6;
        margin: 0 0 16px 0;
      }

      .help-content ol, .help-content ul {
        color: rgba(255, 255, 255, 0.8);
        line-height: 1.8;
        padding-left: 24px;
        margin: 0 0 16px 0;
      }

      .help-content li {
        margin-bottom: 8px;
      }

      .help-content code {
        background: rgba(0, 212, 255, 0.15);
        padding: 2px 6px;
        border-radius: 4px;
        font-family: monospace;
        color: var(--content-accent, #00d4ff);
        font-size: 13px;
      }

      .help-content a {
        color: var(--content-accent, #00d4ff);
        text-decoration: none;
      }

      .help-content a:hover {
        text-decoration: underline;
      }

      .help-content table {
        width: 100%;
        border-collapse: collapse;
        margin: 16px 0;
      }

      .help-content td {
        padding: 8px 12px;
        border-bottom: 1px solid rgba(0, 212, 255, 0.1);
        color: rgba(255, 255, 255, 0.8);
      }

      .help-content td:first-child {
        width: 150px;
      }

      .help-assistant {
        border-left: 1px solid rgba(0, 212, 255, 0.2);
        height: 100%;
      }

      @media (max-width: 1100px) {
        .help-layout {
          grid-template-columns: 180px 1fr 300px;
        }
      }

      @media (max-width: 900px) {
        .help-layout {
          grid-template-columns: 1fr;
          grid-template-rows: auto 1fr 300px;
        }

        .help-sidebar {
          border-right: none;
          border-bottom: 1px solid rgba(0, 212, 255, 0.2);
          display: flex;
          overflow-x: auto;
          padding: 8px;
        }

        .sidebar-title {
          display: none;
        }

        .nav-item {
          white-space: nowrap;
          padding: 8px 12px;
        }

        .nav-item.active {
          border-left: none;
          border-bottom: 2px solid var(--content-accent, #00d4ff);
        }

        .help-assistant {
          border-left: none;
          border-top: 1px solid rgba(0, 212, 255, 0.2);
        }
      }
    `;

    const navItems = sections.map(s => `
      <div class="nav-item ${s.id === this.activeSection ? 'active' : ''}" data-section="${s.id}">
        <span class="nav-icon">${s.icon}</span>
        <span>${s.title}</span>
      </div>
    `).join('');

    const template = `
      <div class="help-layout">
        <div class="help-sidebar">
          <div class="sidebar-title">Help Topics</div>
          ${navItems}
        </div>
        <div class="help-content">
          ${activeContent}
        </div>
        <div class="help-assistant" id="assistant-container"></div>
      </div>
    `;

    this.shadowRoot!.innerHTML = `<style>${styles}</style>${template}`;
    this.setupEventListeners();
    this.initializeAssistant();
  }

  private initializeAssistant(): void {
    const container = this.shadowRoot?.querySelector('#assistant-container') as HTMLElement;
    if (!container) return;

    // Clean up old instance
    this.assistantPanel?.destroy();

    // Create new assistant panel connected to Help room
    this.assistantPanel = new AssistantPanel(container, {
      roomId: DEFAULT_ROOMS.HELP as UUID,
      roomName: 'help',
      placeholder: 'Ask for help...',
      greeting: "Hi! I'm here to help you get started with Continuum. What would you like to know?"
    });
  }

  private setupEventListeners(): void {
    this.shadowRoot?.querySelectorAll('.nav-item').forEach(item => {
      item.addEventListener('click', (e) => {
        const section = (e.currentTarget as HTMLElement).dataset.section;
        if (section && section !== this.activeSection) {
          this.activeSection = section;
          this.renderWidget();
          this.emitPositronContext();  // Notify AIs of section change
        }
      });
    });
  }

  protected async onWidgetCleanup(): Promise<void> {
    this.assistantPanel?.destroy();
    console.log('Help: Cleanup complete');
  }
}

// Registration handled by centralized BROWSER_WIDGETS registry
