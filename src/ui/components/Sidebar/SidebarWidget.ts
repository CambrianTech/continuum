/**
 * Sidebar Widget - Main Application Sidebar
 * Includes resize functionality, room tabs, and widget containers
 */

import { BaseWidget } from '../shared/BaseWidget.js';
import baseCSS from '../shared/BaseWidget.css';
import sidebarCSS from './SidebarWidget.css';

export class SidebarWidget extends BaseWidget {
    private isResizing: boolean = false;
    private startX: number = 0;
    private startWidth: number = 0;
    private currentRoom: string = 'general';

    constructor() {
        super();
        this.widgetName = 'Sidebar';
        this.widgetIcon = '📋';
        this.widgetTitle = 'Application Sidebar';
        // CSS is now bundled, no external path needed
    }

    getBundledCSS(): string {
        return baseCSS + '\n' + sidebarCSS;
    }

    protected async initializeWidget(): Promise<void> {
        this.setupResizeHandlers();
        this.setupRoomSwitching();
        this.loadChildWidgets();
    }

    setupEventListeners(): void {
        // Event listeners are set up in setupResizeHandlers and setupRoomSwitching
        console.log(`🎛️ ${this.widgetName}: Event listeners initialized`);
    }

    private setupResizeHandlers(): void {
        const resizeHandle = this.shadowRoot.querySelector('.sidebar-resize-handle') as HTMLElement;
        
        if (!resizeHandle) return;

        resizeHandle.addEventListener('mousedown', (e: MouseEvent) => {
            this.isResizing = true;
            this.startX = e.clientX;
            this.startWidth = this.offsetWidth;
            
            // Prevent text selection during resize
            document.body.classList.add('resizing');
            document.body.style.userSelect = 'none';
            document.body.style.cursor = 'col-resize';
            
            e.preventDefault();
            e.stopPropagation();
        });

        document.addEventListener('mousemove', (e: MouseEvent) => {
            if (!this.isResizing) return;

            const newWidth = this.startWidth + (e.clientX - this.startX);
            const minWidth = 250;
            const maxWidth = 800;

            if (newWidth >= minWidth && newWidth <= maxWidth) {
                this.style.width = newWidth + 'px';
            }
            
            e.preventDefault();
        });

        document.addEventListener('mouseup', (e: MouseEvent) => {
            if (this.isResizing) {
                this.isResizing = false;
                document.body.classList.remove('resizing');
                document.body.style.userSelect = '';
                document.body.style.cursor = '';
                
                e.preventDefault();
            }
        });
    }

    private setupRoomSwitching(): void {
        this.shadowRoot.querySelectorAll('.room-tab').forEach(tab => {
            tab.addEventListener('click', (e: Event) => {
                const target = e.target as HTMLElement;
                const room = target.dataset.room;
                if (room) {
                    this.switchRoom(room);
                }
            });
        });
    }

    private switchRoom(room: string): void {
        this.currentRoom = room;
        console.log(`🔄 Switched to room: ${this.currentRoom}`);
        
        // Update active tab
        this.shadowRoot.querySelectorAll('.room-tab').forEach(tab => {
            tab.classList.remove('active');
        });
        
        const targetTab = this.shadowRoot.querySelector(`[data-room="${room}"]`);
        if (targetTab) {
            targetTab.classList.add('active');
        }

        // Emit room change event
        this.dispatchEvent(new CustomEvent('room-changed', {
            detail: { room },
            bubbles: true
        }));

        console.log(`🔄 Sidebar switched to room: ${room}`);
    }

    private loadChildWidgets(): void {
        // Child widgets are loaded by the main application
        // This sidebar provides the container for them
        console.log('✅ Sidebar container ready for child widgets');
    }

    renderContent(): string {
        return `
            <div class="sidebar-resize-handle"></div>
            
            <div class="sidebar-header">
                <div class="logo">
                    <div class="logo-row">
                        <div class="continuon-orb-integrated">
                            <div class="orb-ring"></div>
                            <div class="orb-center status-healthy">
                                <span class="orb-emotion"></span>
                            </div>
                            <div class="orb-glow"></div>
                        </div>
                        <div class="logo-text">continuum</div>
                    </div>
                    <div class="subtitle">AI Workforce Construction</div>
                </div>
            </div>
            
            <!-- Room Tabs -->
            <div class="room-tabs">
                <div class="room-tab active" data-room="general">General</div>
                <div class="room-tab" data-room="academy">Academy</div>
                <div class="room-tab" data-room="projects">Projects</div>
            </div>
            
            <div class="sidebar-content">
                <!-- Child Widgets -->
                <active-projects></active-projects>
                <user-selector></user-selector>
                <saved-personas title="Saved Personas"></saved-personas>
            </div>
        `;
    }
}

// Register the custom element
customElements.define('continuum-sidebar', SidebarWidget);