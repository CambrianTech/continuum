/**
 * SidebarPanel Widget - Panel Content for Sidebar Sections
 * Renders widgets dynamically based on JSON configuration
 */
import { BaseWidget } from '../shared/BaseWidget';

export interface PanelWidget {
    readonly type: string;
    readonly config: Record<string, any>;
}

export interface PanelContent {
    readonly panelName: string;
    readonly widgets: PanelWidget[];
    readonly active?: boolean;
}

export class SidebarPanelWidget extends BaseWidget {
    private _panels: PanelContent[] = [];
    private _activePanel: string = 'general';

    public set panels(newPanels: PanelContent[]) {
        this._panels = newPanels;
        // Auto-render when panels are set
        this.render();
    }

    public get panels(): PanelContent[] {
        return this._panels;
    }

    public set activePanel(panelName: string) {
        this._activePanel = panelName;
        this.render();
    }

    public get activePanel(): string {
        return this._activePanel;
    }

    constructor() {
        super();
        this.widgetName = 'SidebarPanelWidget';
        this.widgetIcon = '📄';
        this.widgetTitle = 'Sidebar Panel Widget';
    }

    protected async initializeWidget(): Promise<void> {
        this.render();
        console.log('🗂️ SidebarPanelWidget widget ready');
    }

    protected setupEventListeners(): void {
        // Panel widgets will handle their own events
        console.log('🗂️ SidebarPanelWidget: Event listeners setup');
    }

    public switchPanel(panelName: string): void {
        // Validate that the panel exists
        const panel = this.panels.find(p => p.panelName === panelName);
        if (!panel) {
            console.error(`Invalid panel name: ${panelName}`);
            return;
        }

        this._activePanel = panelName;
        this.updateActivePanel();
        this.emitPanelChange(panel);
    }

    private updateActivePanel(): void {
        const panelElements = this.shadowRoot?.querySelectorAll('.sidebar-panel');
        panelElements?.forEach(panel => {
            const panelElement = panel as HTMLElement;
            const panelName = panelElement.dataset.panel;
            
            if (panelName === this._activePanel) {
                panelElement.style.display = 'block';
            } else {
                panelElement.style.display = 'none';
            }
        });
    }

    private emitPanelChange(panel: PanelContent): void {
        this.dispatchEvent(new CustomEvent('panel-changed', {
            detail: { 
                panel,
                panelName: panel.panelName,
                widgets: panel.widgets
            },
            bubbles: true
        }));
        
        console.log(`🔄 Panel switched to: ${panel.panelName}`);
    }

    public getActivePanel(): PanelContent | undefined {
        return this.panels.find(p => p.panelName === this.activePanel);
    }

    protected renderContent(): string {
        console.log('🗂️ SidebarPanelWidget renderContent() called');
        console.log('🗂️ Current panels array:', this.panels);
        console.log('🗂️ Active panel:', this._activePanel);
        
        const content = `
            <div class="sidebar-panels">
                ${this.panels.map(panel => this.renderPanel(panel)).join('')}
            </div>
        `;
        
        console.log('🗂️ Generated HTML content:', content);
        return content;
    }

    private renderPanel(panel: PanelContent): string {
        const isActive = panel.panelName === this._activePanel;
        
        return `
            <div class="sidebar-panel ${isActive ? 'active' : ''}" 
                 data-panel="${panel.panelName}"
                 style="display: ${isActive ? 'block' : 'none'}">
                ${this.renderPanelWidgets(panel.widgets)}
            </div>
        `;
    }

    private renderPanelWidgets(widgets: PanelWidget[]): string {
        return widgets.map(widget => this.renderWidget(widget)).join('');
    }

    private renderWidget(widget: PanelWidget): string {
        switch (widget.type) {
            case 'session-costs':
                return this.renderSessionCosts();
            
            case 'user-selector':
                return `<user-selector></user-selector>`;
            
            case 'active-projects':
                return `<active-projects></active-projects>`;
            
            case 'saved-personas':
                return `<saved-personas></saved-personas>`;
            
            case 'academy-status':
                return `<academy-status-widget></academy-status-widget>`;
            
            case 'academy-ready-status':
                return this.renderAcademyReadyStatus();
            
            default:
                console.warn(`🗂️ Unknown widget type: ${widget.type}`);
                return `<div class="unknown-widget">Unknown widget: ${widget.type}</div>`;
        }
    }

    private renderSessionCosts(): string {
        return `
            <!-- Session Costs Section -->
            <div class="session-costs-section">
                <div class="section-header">
                    <span class="section-icon">💰</span>
                    <span class="section-title">Session Costs</span>
                    <span class="section-status">Active</span>
                </div>
                <div class="cost-display">
                    <div class="cost-row">
                        <span class="cost-label">Requests</span>
                        <span class="cost-value">47</span>
                    </div>
                    <div class="cost-row">
                        <span class="cost-label">Cost</span>
                        <span class="cost-value highlight">$0.0000</span>
                    </div>
                </div>
            </div>
        `;
    }

    private renderAcademyReadyStatus(): string {
        return `
            <!-- Academy Ready Status -->
            <div class="academy-ready-section">
                <div class="section-header">
                    <span class="section-icon">🎓</span>
                    <span class="section-title">Academy Status</span>
                    <span class="section-status">Ready</span>
                </div>
                <div class="ready-display">
                    <div class="ready-indicator">
                        <span class="ready-icon">✅</span>
                        <span class="ready-text">Training System Online</span>
                    </div>
                </div>
            </div>
        `;
    }
}

// Register the custom element
if (!customElements.get('sidebar-panel')) {
    customElements.define('sidebar-panel', SidebarPanelWidget);
}