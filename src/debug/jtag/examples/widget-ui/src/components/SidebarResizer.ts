/**
 * SidebarResizer - Draggable resizer for desktop layout
 * Handles sidebar width with UIPreferencesEntity persistence
 */
import { Commands } from '../../../../system/core/shared/Commands';
import type { UIPreferencesEntity } from '../../../../system/data/entities/UIPreferencesEntity';
import type { DataReadResult } from '../../../../commands/data/read/shared/DataReadTypes';
import type { DataUpdateResult } from '../../../../commands/data/update/shared/DataUpdateTypes';

interface SidebarResizedDetail {
    width: number;
}

interface WidthLimits {
    min: number;
    max: number;
    default: number;
}

class SidebarResizer extends HTMLElement {
    private isDragging: boolean = false;
    private startX: number = 0;
    private startWidth: number = 0;
    private currentWidth?: number;
    private readonly minWidth: number = 100;
    private readonly maxWidth: number = 800;
    private readonly defaultWidth: number = 400;
    private readonly uiPrefsId: string = 'browser-ui-preferences';
    
    // Event handler references for proper cleanup
    private boundMouseMove?: (e: MouseEvent) => void;
    private boundMouseUp?: () => void;
    private boundTouchMove?: (e: TouchEvent) => void;
    private boundTouchEnd?: () => void;

    constructor() {
        super();
        this.attachShadow({ mode: 'open' });
    }

    connectedCallback(): void {
        this.render();
        this.setupEventListeners();
        this.loadSavedWidth();
    }

    disconnectedCallback(): void {
        this.removeEventListeners();
    }

    private async loadSavedWidth(): Promise<void> {
        try {
            // Try to load UIPreferencesEntity from localStorage
            const result = await Commands.execute<DataReadResult<UIPreferencesEntity>>('data/read', {
                collection: 'UIPreferences',
                id: this.uiPrefsId,
                backend: 'browser'
            });

            if (result.success && result.found && result.data) {
                const width = result.data.layout?.sidebarWidth ?? this.defaultWidth;
                if (width >= this.minWidth && width <= this.maxWidth) {
                    this.applySidebarWidth(width);
                    return;
                }
            }
        } catch (error) {
            const errorMessage = error instanceof Error ? error.message : 'Unknown error';
            console.warn('SidebarResizer: Failed to load saved width:', errorMessage);
        }

        // Fallback: check old localStorage key for migration
        try {
            const oldWidth = localStorage.getItem('continuum-sidebar-width');
            if (oldWidth) {
                const width = parseInt(oldWidth, 10);
                if (!isNaN(width) && width >= this.minWidth && width <= this.maxWidth) {
                    console.log('🔄 SidebarResizer: Migrating from old localStorage');
                    this.applySidebarWidth(width);
                    await this.saveWidth(width); // Save to new format
                    return;
                }
            }
        } catch {
            // Ignore migration errors
        }

        // Use default width if no valid saved width
        this.applySidebarWidth(this.defaultWidth);
    }

    private async saveWidth(width: number): Promise<void> {
        try {
            // Try to read first to see if entity exists
            const readResult = await Commands.execute<DataReadResult<UIPreferencesEntity>>('data/read', {
                collection: 'UIPreferences',
                id: this.uiPrefsId,
                backend: 'browser'
            });

            if (readResult.success && readResult.found) {
                // Entity exists, update it
                await Commands.execute<DataUpdateResult<UIPreferencesEntity>>('data/update', {
                    collection: 'UIPreferences',
                    id: this.uiPrefsId,
                    data: {
                        layout: {
                            sidebarWidth: width
                        }
                    },
                    backend: 'browser'
                });
            } else {
                // Entity doesn't exist, create it
                await Commands.execute(DATA_COMMANDS.CREATE, {
                    collection: 'UIPreferences',
                    data: {
                        id: this.uiPrefsId,
                        deviceId: 'default',
                        layout: {
                            sidebarWidth: width
                        },
                        theme: {
                            name: 'dark'
                        },
                        behavior: {
                            enableAnimations: true,
                            enableSounds: false,
                            autoSave: true,
                            autoSaveIntervalMs: 5000
                        }
                    },
                    backend: 'browser'
                });
            }
        } catch (error) {
            const errorMessage = error instanceof Error ? error.message : 'Unknown error';
            console.warn('SidebarResizer: Failed to save width:', errorMessage);
        }
    }

    private applySidebarWidth(width: number): void {
        // Find sidebar-container and desktop-container for proper resizing
        let sidebarContainer: HTMLElement | null = null;
        let desktopContainer: HTMLElement | null = null;
        
        // Strategy 1: Look for continuum-widget in the document and search its shadow DOM
        const continuumWidget = document.querySelector('continuum-widget') as any;
        if (continuumWidget && continuumWidget.shadowRoot) {
            sidebarContainer = continuumWidget.shadowRoot.querySelector('.sidebar-container') as HTMLElement;
            desktopContainer = continuumWidget.shadowRoot.querySelector('.desktop-container') as HTMLElement;
        }
        
        // Strategy 2: Try to find sidebar-container from this element's context
        if (!sidebarContainer) {
            sidebarContainer = this.closest('.sidebar-container') as HTMLElement;
        }
        
        // Strategy 3: Direct document queries (fallback)
        if (!sidebarContainer) {
            sidebarContainer = document.querySelector('.sidebar-container') as HTMLElement;
        }
        if (!desktopContainer) {
            desktopContainer = document.querySelector('.desktop-container') as HTMLElement;
        }
        
        if (sidebarContainer && desktopContainer) {
            // Set the sidebar container width directly
            sidebarContainer.style.width = `${width}px`;
            
            // Update the grid template to match the new sidebar width
            desktopContainer.style.gridTemplateColumns = `${width}px 1fr`;
            
            this.currentWidth = width;
            
            // Dispatch custom event for other components
            const event = new CustomEvent<SidebarResizedDetail>('sidebar-resized', {
                detail: { width },
                bubbles: true
            });
            this.dispatchEvent(event);            
        } else {
            console.warn('SidebarResizer: Could not find .sidebar-container or .desktop-container');
        }
    }

    private setupEventListeners(): void {
        // Create bound references for proper cleanup
        this.boundMouseMove = this.handleMouseMove.bind(this);
        this.boundMouseUp = this.handleMouseUp.bind(this);
        this.boundTouchMove = this.handleTouchMove.bind(this);
        this.boundTouchEnd = this.handleTouchEnd.bind(this);
        
        this.shadowRoot?.addEventListener('mousedown', this.handleMouseDown.bind(this));
        document.addEventListener('mousemove', this.boundMouseMove);
        document.addEventListener('mouseup', this.boundMouseUp);
        
        // Touch events for mobile
        this.shadowRoot?.addEventListener('touchstart', this.handleTouchStart.bind(this));
        document.addEventListener('touchmove', this.boundTouchMove);
        document.addEventListener('touchend', this.boundTouchEnd);
    }

    private removeEventListeners(): void {
        if (this.boundMouseMove) {
            document.removeEventListener('mousemove', this.boundMouseMove);
        }
        if (this.boundMouseUp) {
            document.removeEventListener('mouseup', this.boundMouseUp);
        }
        if (this.boundTouchMove) {
            document.removeEventListener('touchmove', this.boundTouchMove);
        }
        if (this.boundTouchEnd) {
            document.removeEventListener('touchend', this.boundTouchEnd);
        }
    }

    private handleMouseDown(e: MouseEvent): void {
        this.startDrag(e.clientX);
        e.preventDefault();
    }

    private handleTouchStart(e: TouchEvent): void {
        if (e.touches.length === 1) {
            this.startDrag(e.touches[0].clientX);
            e.preventDefault();
        }
    }

    private startDrag(clientX: number): void {
        this.isDragging = true;
        this.startX = clientX;
        this.startWidth = this.currentWidth || this.defaultWidth;
        
        // Set cursor for dragging (text selection already disabled globally)
        document.body.style.cursor = 'col-resize';
        
        // Add dragging class to root for additional styling
        document.documentElement.classList.add('sidebar-dragging');
        
        // Visual feedback on resizer
        this.shadowRoot?.host.classList.add('dragging');
        
        console.log('🔧 SidebarResizer: Started drag', { startX: this.startX, startWidth: this.startWidth });
    }

    private handleMouseMove(e: MouseEvent): void {
        if (this.isDragging) {
            this.doDrag(e.clientX);
        }
    }

    private handleTouchMove(e: TouchEvent): void {
        if (this.isDragging && e.touches.length === 1) {
            this.doDrag(e.touches[0].clientX);
            e.preventDefault();
        }
    }

    private doDrag(clientX: number): void {
        const deltaX = clientX - this.startX;
        const newWidth = Math.max(this.minWidth, Math.min(this.maxWidth, this.startWidth + deltaX));
        
        this.applySidebarWidth(newWidth);
    }

    private handleMouseUp(): void {
        this.endDrag();
    }

    private handleTouchEnd(): void {
        this.endDrag();
    }

    private endDrag(): void {
        if (this.isDragging) {
            this.isDragging = false;

            // Remove dragging feedback
            document.body.style.cursor = '';

            // Remove dragging class from root
            document.documentElement.classList.remove('sidebar-dragging');

            // Remove visual feedback on resizer
            this.shadowRoot?.host.classList.remove('dragging');

            // Save the final width (async, fire and forget)
            if (this.currentWidth !== undefined) {
                this.saveWidth(this.currentWidth).catch(err => {
                    console.error('SidebarResizer: Failed to save width:', err);
                });
            }

            console.log('🔧 SidebarResizer: Ended drag, final width:', this.currentWidth);
        }
    }

    private render(): void {
        if (!this.shadowRoot) return;
        
        this.shadowRoot.innerHTML = `
            <style>
                :host {
                    position: absolute;
                    top: 0;
                    right: -1px;
                    bottom: 0;
                    width: 2px;
                    cursor: col-resize;
                    z-index: 1000;
                    background: rgba(0, 212, 255, 0.1);
                    border-right: 1px solid rgba(0, 212, 255, 0.2);
                    transition: all 0.2s ease;
                }

                :host(:hover) {
                    background: rgba(0, 212, 255, 0.3);
                    border-right: 1px solid #00d4ff;
                    box-shadow: 0 0 4px rgba(0, 212, 255, 0.5);
                }

                :host(.dragging) {
                    background: rgba(0, 212, 255, 0.6);
                    border-right: 2px solid #00d4ff;
                    box-shadow: 0 0 6px rgba(0, 212, 255, 0.8);
                    width: 3px;
                }

                .resizer-handle {
                    position: absolute;
                    top: 0;
                    bottom: 0;
                    left: 0;
                    right: 0;
                    background: transparent;
                }
            </style>
            <div class="resizer-handle"></div>
        `;
    }

    // API methods for external control
    getCurrentWidth(): number {
        return this.currentWidth || this.defaultWidth;
    }

    async setWidth(width: number): Promise<void> {
        const clampedWidth = Math.max(this.minWidth, Math.min(this.maxWidth, width));
        this.applySidebarWidth(clampedWidth);
        await this.saveWidth(clampedWidth);
    }

    resetToDefault(): void {
        this.setWidth(this.defaultWidth).catch(err => {
            console.error('SidebarResizer: Failed to reset width:', err);
        });
    }

    getWidthLimits(): WidthLimits {
        return {
            min: this.minWidth,
            max: this.maxWidth,
            default: this.defaultWidth
        };
    }
}

// Register the custom element
customElements.define('sidebar-resizer', SidebarResizer);

export { SidebarResizer };
export type { SidebarResizedDetail, WidthLimits };