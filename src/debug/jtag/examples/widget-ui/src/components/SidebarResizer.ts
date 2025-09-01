/**
 * SidebarResizer - Draggable resizer for desktop layout
 * Handles sidebar width with localStorage persistence
 */
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
    private readonly minWidth: number = 150;
    private readonly maxWidth: number = 500;
    private readonly defaultWidth: number = 250;
    private readonly storageKey: string = 'continuum-sidebar-width';
    
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

    private loadSavedWidth(): void {
        try {
            const savedWidth = localStorage.getItem(this.storageKey);
            if (savedWidth) {
                const width = parseInt(savedWidth, 10);
                if (width >= this.minWidth && width <= this.maxWidth) {
                    this.applySidebarWidth(width);
                    return;
                }
            }
        } catch (error) {
            const errorMessage = error instanceof Error ? error.message : 'Unknown error';
            console.warn('SidebarResizer: Failed to load saved width:', errorMessage);
        }
        
        // Use default width if no valid saved width
        this.applySidebarWidth(this.defaultWidth);
    }

    private saveWidth(width: number): void {
        try {
            localStorage.setItem(this.storageKey, width.toString());
            console.log('🔧 SidebarResizer: Saved width:', width);
        } catch (error) {
            const errorMessage = error instanceof Error ? error.message : 'Unknown error';
            console.warn('SidebarResizer: Failed to save width:', errorMessage);
        }
    }

    private applySidebarWidth(width: number): void {
        // Find desktop-container by traversing up from this element's location
        let desktopContainer: HTMLElement | null = null;
        
        // First, try to find it in the same shadow DOM or document as this element
        const root = this.shadowRoot?.host.getRootNode() as Document | ShadowRoot;
        if (root) {
            desktopContainer = (root as any).querySelector?.('.desktop-container') as HTMLElement;
        }
        
        // If still not found, try the document (for non-shadow DOM cases)
        if (!desktopContainer) {
            desktopContainer = document.querySelector('.desktop-container') as HTMLElement;
        }
        
        if (desktopContainer) {
            desktopContainer.style.gridTemplateColumns = `${width}px 1fr`;
            this.currentWidth = width;
            
            // Dispatch custom event for other components
            const event = new CustomEvent<SidebarResizedDetail>('sidebar-resized', {
                detail: { width },
                bubbles: true
            });
            this.dispatchEvent(event);
        } else {
            console.warn('SidebarResizer: Could not find .desktop-container');
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
            
            // Save the final width
            if (this.currentWidth !== undefined) {
                this.saveWidth(this.currentWidth);
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
                    right: -4px;
                    bottom: 0;
                    width: 8px;
                    cursor: col-resize;
                    z-index: 1000;
                    background: transparent;
                    transition: background 0.2s ease;
                }

                :host(:hover) {
                    background: rgba(0, 212, 255, 0.3);
                }

                :host(.dragging) {
                    background: rgba(0, 212, 255, 0.6);
                }

                .resizer-line {
                    position: absolute;
                    top: 0;
                    bottom: 0;
                    left: 50%;
                    width: 1px;
                    background: rgba(0, 212, 255, 0.2);
                    transform: translateX(-50%);
                    transition: all 0.2s ease;
                }

                :host(:hover) .resizer-line {
                    background: rgba(0, 212, 255, 0.6);
                    box-shadow: 0 0 4px rgba(0, 212, 255, 0.4);
                }

                :host(.dragging) .resizer-line {
                    background: rgba(0, 212, 255, 0.8);
                    box-shadow: 0 0 8px rgba(0, 212, 255, 0.6);
                    width: 2px;
                }
            </style>
            <div class="resizer-line"></div>
        `;
    }

    // API methods for external control
    getCurrentWidth(): number {
        return this.currentWidth || this.defaultWidth;
    }

    setWidth(width: number): void {
        const clampedWidth = Math.max(this.minWidth, Math.min(this.maxWidth, width));
        this.applySidebarWidth(clampedWidth);
        this.saveWidth(clampedWidth);
    }

    resetToDefault(): void {
        this.setWidth(this.defaultWidth);
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