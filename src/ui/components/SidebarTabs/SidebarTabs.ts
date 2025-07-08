/**
 * SidebarTabs Widget - Tab Control for Sidebar Sections
 * Controls which section is active in the sidebar with strong typing
 */
import { BaseWidget } from '../shared/BaseWidget';

export interface TabContent {
    readonly title: string;
    readonly panelName: string;
    readonly dataKey: string;
    readonly selected?: boolean;
}

export class SidebarTabs extends BaseWidget {
    private _tabs: TabContent[] = [];
    private activeTab: string = 'general';

    public set tabs(newTabs: TabContent[]) {
        this._tabs = newTabs;
        // Auto-render when tabs are set
        this.render();
    }

    public get tabs(): TabContent[] {
        return this._tabs;
    }

    constructor() {
        super();
        this.widgetName = 'SidebarTabs';
        this.widgetIcon = '📑';
        this.widgetTitle = 'Sidebar Tabs';
    }

    protected async initializeWidget(): Promise<void> {
        this.render();
        console.log('🔧 SidebarTabs widget ready');
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

    private emitTabChange(tab: TabContent): void {
        this.dispatchEvent(new CustomEvent('tab-changed', {
            detail: { 
                tab,
                panelName: tab.panelName,
                title: tab.title,
                dataKey: tab.dataKey
            },
            bubbles: true
        }));
        
        console.log(`🔄 Tab switched to: ${tab.title} (${tab.panelName})`);
    }

    public getActiveTab(): TabContent | undefined {
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