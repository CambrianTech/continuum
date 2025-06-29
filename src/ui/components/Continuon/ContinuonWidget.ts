/**
 * ContinuonWidget - HAL 9000-style status orb with scrolling status feed
 * Shows system health, emotions, and live status updates
 */

import { BaseWidget } from '../shared/BaseWidget.js';

interface StatusMessage {
  text: string;
  timestamp: number;
  id: string;
}

export class ContinuonWidget extends BaseWidget {
  private currentStatus: 'red' | 'yellow' | 'green' = 'red';
  private currentEmotion: string | null = null;
  private statusFeed: StatusMessage[] = [];
  private maxStatusMessages: number = 5;

  constructor() {
    super();
    this.widgetName = 'ContinuonWidget';
    this.widgetIcon = '🔮';
    this.widgetTitle = 'System Status Orb';
    this.cssPath = '/src/ui/components/Continuon/ContinuonWidget.css';
  }

  protected async initializeWidget(): Promise<void> {
    await this.loadCSS();
    this.setupEventListeners();
    this.setupStatusFeed();
    
    // Log version on startup
    const version = this.getSystemVersion();
    console.log(`🚀 Continuum v${version} - ContinuonWidget initialized`);
    
    // Initial status
    this.addStatusMessage('System initializing...');
    this.updateStatus('yellow', 'Starting up...');
    this.render();
    
    // Set to green after a brief delay
    setTimeout(() => {
      this.updateStatus('green', 'System ready');
    }, 2000);
  }

  setupEventListeners(): void {
    // Listen for status changes
    document.addEventListener('continuum:status-change', (e: Event) => {
      const customEvent = e as CustomEvent;
      this.updateStatus(customEvent.detail.status, customEvent.detail.message);
    });

    // Listen for emotions
    document.addEventListener('continuum:emotion', (e: Event) => {
      const customEvent = e as CustomEvent;
      this.showEmotion(customEvent.detail.emotion, customEvent.detail.duration || 3000);
    });

    // Listen for system events
    document.addEventListener('continuum:system-event', (e: Event) => {
      const customEvent = e as CustomEvent;
      this.addStatusMessage(customEvent.detail.message);
    });

    // Setup orb interactions
    this.setupOrbEventListeners();
  }

  renderContent(): string {
    const orbContent = this.currentEmotion || '';
    const statusColor = this.getStatusColor();
    
    return `
      <div class="continuon-container">
        <div class="continuon-orb-container">
          <div class="continuon-orb ${statusColor}" data-emotion="${this.currentEmotion || ''}">
            <div class="orb-ring"></div>
            <div class="orb-center">
              <span class="orb-emotion">${orbContent}</span>
            </div>
            <div class="orb-glow"></div>
          </div>
          <span class="continuon-label">continuum</span>
        </div>
        
        <div class="status-feed">
          <div class="status-messages">
            ${this.renderStatusMessages()}
          </div>
        </div>
      </div>
    `;
  }

  private renderStatusMessages(): string {
    return this.statusFeed
      .slice(-this.maxStatusMessages)
      .map((msg, index) => `
        <div class="status-message fade-${index}" 
             style="animation-delay: ${index * 0.1}s">
          ${msg.text}
        </div>
      `).join('');
  }

  private updateStatus(status: 'red' | 'yellow' | 'green', message?: string): void {
    if (this.currentStatus !== status) {
      this.currentStatus = status;
      
      if (message) {
        this.addStatusMessage(message);
      }
      
      // Update title and favicon
      this.updateTitleAndFavicon();
      
      this.render();
      
      // Log version only on status changes
      this.logVersionIfChanged();
    }
  }

  private showEmotion(emotion: string, duration: number): void {
    this.currentEmotion = emotion;
    
    // Update favicon to show emotion
    this.updateTitleAndFavicon();
    
    this.render();
    
    // Clear emotion after duration
    setTimeout(() => {
      this.currentEmotion = null;
      this.updateTitleAndFavicon(); // Restore status color
      this.render();
    }, duration);
  }

  private addStatusMessage(text: string): void {
    const message: StatusMessage = {
      text,
      timestamp: Date.now(),
      id: Math.random().toString(36).substr(2, 9)
    };
    
    this.statusFeed.push(message);
    
    // Keep only recent messages
    if (this.statusFeed.length > this.maxStatusMessages * 2) {
      this.statusFeed = this.statusFeed.slice(-this.maxStatusMessages);
    }
    
    this.render();
  }

  private getStatusColor(): string {
    switch (this.currentStatus) {
      case 'green': return 'status-healthy';
      case 'yellow': return 'status-degraded';
      case 'red': return 'status-error';
      default: return 'status-error';
    }
  }

  private logVersionIfChanged(): void {
    // Only log version when status changes (not spam)
    const version = this.getSystemVersion();
    const lastVersion = localStorage.getItem('continuum-last-version');
    
    if (version && version !== lastVersion) {
      console.log(`🚀 Continuum ${lastVersion ? `${lastVersion} → ${version}` : `v${version}`}`);
      localStorage.setItem('continuum-last-version', version);
    }
  }

  private getSystemVersion(): string | null {
    // Get version from continuum API or DOM
    const continuum = (window as any).continuum;
    return continuum?.version || '0.2.2177';
  }

  private setupStatusFeed(): void {
    // Connect to WebSocket events for live status updates
    const continuum = (window as any).continuum;
    if (continuum) {
      // Listen for connection events
      continuum.on('connected', () => {
        this.updateStatus('green', 'Connected');
      });
      
      continuum.on('disconnected', () => {
        this.updateStatus('red', 'Disconnected');
      });
      
      continuum.on('reconnecting', () => {
        this.updateStatus('yellow', 'Reconnecting...');
      });
    }
  }

  private updateTitleAndFavicon(): void {
    const displayIcon = this.currentEmotion || this.getStatusIcon();
    
    // Update favicon
    this.updateFavicon(displayIcon);
    
    // Update title (keep it simple)
    document.title = 'continuum';
  }

  private getStatusIcon(): string {
    switch (this.currentStatus) {
      case 'green': return '🟢';
      case 'yellow': return '🟡';
      case 'red': return '🔴';
      default: return '🔴';
    }
  }

  private updateFavicon(icon: string): void {
    const favicon = document.getElementById('favicon') as HTMLLinkElement;
    if (favicon) {
      const svg = `<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 100 100'><text y='.9em' font-size='90'>${icon}</text></svg>`;
      favicon.href = `data:image/svg+xml,${encodeURIComponent(svg)}`;
    }
  }

  private setupOrbEventListeners(): void {
    // Orb click interactions
    const orb = this.shadowRoot?.querySelector('.continuon-orb');
    if (orb) {
      orb.addEventListener('click', () => {
        this.triggerEmotionDemo();
      });
    }
  }

  private triggerEmotionDemo(): void {
    const emotions = ['😉', '🎉', '🚀', '💫', '✨'];
    const randomEmotion = emotions[Math.floor(Math.random() * emotions.length)];
    this.showEmotion(randomEmotion, 2000);
  }
}

// Register the widget
customElements.define('continuon-widget', ContinuonWidget);