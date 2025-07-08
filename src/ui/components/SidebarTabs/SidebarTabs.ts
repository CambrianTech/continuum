/**
 * SidebarTabs Widget - Tab Control for Sidebar Sections
 * Controls which section is active in the sidebar with strong typing
 */
import { BaseWidget } from '../shared/BaseWidget';

export interface SidebarTab {
    readonly title: string;
    readonly panelName: string;
}

export class SidebarTabs extends BaseWidget {
    private tabs: SidebarTab[] = [];
    private activeTab: string = 'general';

    constructor() {
        super();
        this.widgetName = 'SidebarTabs';
        this.widgetIcon = '📑';
        this.widgetTitle = 'Sidebar Tabs';
    }

    protected async initializeWidget(): Promise<void> {
        await this.loadTabsFromConfig();
        this.render();
        console.log('🔧 SidebarTabs widget ready');
    }

    private async loadTabsFromConfig(): Promise<void> {
        try {
            const configPath = '/src/ui/components/Sidebar/sidebar-config.json';
            const response = await fetch(configPath);
            if (response.ok) {
                const config = await response.json();
                this.tabs = config.tabs || [];
                this.activeTab = config.defaultTab || 'general';
                console.log('📑 SidebarTabs loaded tabs from config:', this.tabs);
                // Re-render now that we have the tabs data
                this.render();
            } else {
                console.warn('Failed to load sidebar config');
            }
        } catch (error) {
            console.error('Error loading sidebar config:', error);
        }
    }

    protected setupEventListeners(): void {
        const tabElements = this.shadowRoot?.querySelectorAll('.room-tab');
        tabElements?.forEach(tab => {
            tab.addEventListener('click', (e: Event) => {
                const target = e.target as HTMLElement;
                const panelName = target.dataset.room;
                if (panelName) {
                    this.switchTab(panelName);
                }
            });
        });
    }

    private switchTab(panelName: string): void {
        // Validate that the panel exists
        const tab = this.tabs.find(t => t.panelName === panelName);
        if (!tab) {
            console.error(`Invalid panel name: ${panelName}`);
            return;
        }

        this.activeTab = panelName;
        this.updateActiveTab();
        this.emitTabChange(tab);
    }

    private updateActiveTab(): void {
        const tabElements = this.shadowRoot?.querySelectorAll('.room-tab');
        tabElements?.forEach(tab => {
            tab.classList.remove('active');
            if ((tab as HTMLElement).dataset.room === this.activeTab) {
                tab.classList.add('active');
            }
        });
    }

    private emitTabChange(tab: SidebarTab): void {
        this.dispatchEvent(new CustomEvent('tab-changed', {
            detail: { 
                tab,
                panelName: tab.panelName,
                title: tab.title
            },
            bubbles: true
        }));
        
        console.log(`🔄 Tab switched to: ${tab.title} (${tab.panelName})`);
    }

    public setTabs(tabs: SidebarTab[]): void {
        console.log('📑 SidebarTabs.setTabs called with:', tabs);
        this.tabs = tabs;
        console.log('📑 SidebarTabs.tabs now contains:', this.tabs.length, 'tabs');
        this.render();
        console.log('📑 SidebarTabs rendered');
    }

    public getActiveTab(): SidebarTab | undefined {
        return this.tabs.find(t => t.panelName === this.activeTab);
    }

    protected renderContent(): string {
        console.log('📑 SidebarTabs renderContent() called');
        console.log('📑 Current tabs array:', this.tabs);
        console.log('📑 Active tab:', this.activeTab);
        
        const content = `
            <div class="room-tabs">
                ${this.tabs.map(tab => `
                    <div class="room-tab ${tab.panelName === this.activeTab ? 'active' : ''}" 
                         data-room="${tab.panelName}">
                        ${tab.title}
                    </div>
                `).join('')}
            </div>
        `;
        
        console.log('📑 Generated HTML content:', content);
        return content;
    }
}

// Register the custom element
if (!customElements.get('sidebar-tabs')) {
    customElements.define('sidebar-tabs', SidebarTabs);
}