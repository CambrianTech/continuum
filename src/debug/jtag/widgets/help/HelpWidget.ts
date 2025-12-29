/**
 * HelpWidget - Onboarding and documentation with AI assistance
 *
 * Helps new users get started with Continuum.
 * Includes embedded AI assistant for contextual help.
 *
 * Structure:
 * - public/help-widget.html - Template container
 * - public/help-widget.scss - Styles (compiled to .css)
 * - HelpWidget.ts - Logic (this file)
 */

import { BaseWidget } from '../shared/BaseWidget';
import { PositronWidgetState } from '../shared/services/state/PositronWidgetState';

interface HelpSection {
  id: string;
  title: string;
  icon: string;
  content: string;
}

export class HelpWidget extends BaseWidget {
  private activeSection: string = 'getting-started';

  constructor() {
    super({
      widgetName: 'HelpWidget',
      template: 'help-widget.html',
      styles: 'help-widget.css',
      enableAI: false,
      enableDatabase: false,
      enableRouterEvents: false,
      enableScreenshots: false
    });
  }

  /**
   * Override path resolution - directory is 'help' (matches class name pattern)
   */
  protected resolveResourcePath(filename: string): string {
    return `widgets/help/public/${filename}`;
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
    // Inject loaded template and styles into shadow DOM
    if (this.shadowRoot && (this.templateHTML || this.templateCSS)) {
      const styleTag = this.templateCSS ? `<style>${this.templateCSS}</style>` : '';
      this.shadowRoot.innerHTML = styleTag + (this.templateHTML || '');
    }

    // Render dynamic content
    this.renderContent();
    this.setupEventListeners();
  }

  private renderContent(): void {
    const sections = this.getSections();
    const activeContent = sections.find(s => s.id === this.activeSection)?.content || '';

    // Render nav items
    const navItemsContainer = this.shadowRoot?.querySelector('.nav-items');
    if (navItemsContainer) {
      navItemsContainer.innerHTML = sections.map(s => `
        <div class="nav-item ${s.id === this.activeSection ? 'active' : ''}" data-section="${s.id}">
          <span class="nav-icon">${s.icon}</span>
          <span>${s.title}</span>
        </div>
      `).join('');
    }

    // Render help content
    const contentContainer = this.shadowRoot?.querySelector('.help-content');
    if (contentContainer) {
      contentContainer.innerHTML = activeContent;
    }
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
    console.log('Help: Cleanup complete');
  }
}

// Registration handled by centralized BROWSER_WIDGETS registry
