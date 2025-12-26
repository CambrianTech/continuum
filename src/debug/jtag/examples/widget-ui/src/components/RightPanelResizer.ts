/**
 * RightPanelResizer - Draggable resizer for right panel
 * Mirrors SidebarResizer but for the right side of the layout
 */

interface RightPanelResizedDetail {
    width: number;
}

interface WidthLimits {
    min: number;
    max: number;
    default: number;
}

class RightPanelResizer extends HTMLElement {
    private isDragging: boolean = false;
    private startX: number = 0;
    private startWidth: number = 0;
    private currentWidth?: number;
    private readonly minWidth: number = 200;
    private readonly maxWidth: number = 600;
    private readonly defaultWidth: number = 320;

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
            const savedWidth = localStorage.getItem('continuum-right-panel-width');
            if (savedWidth) {
                const width = parseInt(savedWidth, 10);
                if (!isNaN(width) && width >= this.minWidth && width <= this.maxWidth) {
                    this.applyPanelWidth(width);
                    return;
                }
            }
        } catch {
            // Ignore errors
        }
        this.applyPanelWidth(this.defaultWidth);
    }

    private saveWidth(width: number): void {
        try {
            localStorage.setItem('continuum-right-panel-width', width.toString());
        } catch {
            // Ignore errors
        }
    }

    private applyPanelWidth(width: number): void {
        let rightPanelContainer: HTMLElement | null = null;
        let desktopContainer: HTMLElement | null = null;

        // Find containers in shadow DOM
        const continuumWidget = document.querySelector('continuum-widget') as any;
        if (continuumWidget && continuumWidget.shadowRoot) {
            rightPanelContainer = continuumWidget.shadowRoot.querySelector('.right-panel-container') as HTMLElement;
            desktopContainer = continuumWidget.shadowRoot.querySelector('.desktop-container') as HTMLElement;
        }

        if (!rightPanelContainer) {
            rightPanelContainer = this.closest('.right-panel-container') as HTMLElement;
        }
        if (!desktopContainer) {
            desktopContainer = document.querySelector('.desktop-container') as HTMLElement;
        }

        if (rightPanelContainer && desktopContainer) {
            rightPanelContainer.style.width = `${width}px`;

            // Get current sidebar width from the grid
            const currentCols = getComputedStyle(desktopContainer).gridTemplateColumns.split(' ');
            const sidebarWidth = currentCols[0] || 'var(--sidebar-width, 250px)';

            desktopContainer.style.gridTemplateColumns = `${sidebarWidth} 1fr ${width}px`;

            this.currentWidth = width;

            const event = new CustomEvent<RightPanelResizedDetail>('right-panel-resized', {
                detail: { width },
                bubbles: true
            });
            this.dispatchEvent(event);
        }
    }

    private setupEventListeners(): void {
        this.boundMouseMove = this.handleMouseMove.bind(this);
        this.boundMouseUp = this.handleMouseUp.bind(this);
        this.boundTouchMove = this.handleTouchMove.bind(this);
        this.boundTouchEnd = this.handleTouchEnd.bind(this);

        this.shadowRoot?.addEventListener('mousedown', this.handleMouseDown.bind(this));
        document.addEventListener('mousemove', this.boundMouseMove);
        document.addEventListener('mouseup', this.boundMouseUp);

        this.shadowRoot?.addEventListener('touchstart', this.handleTouchStart.bind(this));
        document.addEventListener('touchmove', this.boundTouchMove);
        document.addEventListener('touchend', this.boundTouchEnd);
    }

    private removeEventListeners(): void {
        if (this.boundMouseMove) document.removeEventListener('mousemove', this.boundMouseMove);
        if (this.boundMouseUp) document.removeEventListener('mouseup', this.boundMouseUp);
        if (this.boundTouchMove) document.removeEventListener('touchmove', this.boundTouchMove);
        if (this.boundTouchEnd) document.removeEventListener('touchend', this.boundTouchEnd);
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

        document.body.style.cursor = 'col-resize';
        document.documentElement.classList.add('right-panel-dragging');
        this.shadowRoot?.host.classList.add('dragging');
    }

    private handleMouseMove(e: MouseEvent): void {
        if (this.isDragging) this.doDrag(e.clientX);
    }

    private handleTouchMove(e: TouchEvent): void {
        if (this.isDragging && e.touches.length === 1) {
            this.doDrag(e.touches[0].clientX);
            e.preventDefault();
        }
    }

    private doDrag(clientX: number): void {
        // For right panel, dragging LEFT increases width, RIGHT decreases
        const deltaX = this.startX - clientX;
        const newWidth = Math.max(this.minWidth, Math.min(this.maxWidth, this.startWidth + deltaX));
        this.applyPanelWidth(newWidth);
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
            document.body.style.cursor = '';
            document.documentElement.classList.remove('right-panel-dragging');
            this.shadowRoot?.host.classList.remove('dragging');

            if (this.currentWidth !== undefined) {
                this.saveWidth(this.currentWidth);
            }
        }
    }

    private render(): void {
        if (!this.shadowRoot) return;

        this.shadowRoot.innerHTML = `
            <style>
                :host {
                    position: absolute;
                    top: 0;
                    left: -1px;
                    bottom: 0;
                    width: 2px;
                    cursor: col-resize;
                    z-index: 1000;
                    background: rgba(0, 212, 255, 0.1);
                    border-left: 1px solid rgba(0, 212, 255, 0.2);
                    transition: all 0.2s ease;
                }

                :host(:hover) {
                    background: rgba(0, 212, 255, 0.3);
                    border-left: 1px solid #00d4ff;
                    box-shadow: 0 0 4px rgba(0, 212, 255, 0.5);
                }

                :host(.dragging) {
                    background: rgba(0, 212, 255, 0.6);
                    border-left: 2px solid #00d4ff;
                    box-shadow: 0 0 6px rgba(0, 212, 255, 0.8);
                    width: 3px;
                }

                .resizer-handle {
                    position: absolute;
                    top: 0;
                    bottom: 0;
                    left: -4px;
                    right: -4px;
                    background: transparent;
                }
            </style>
            <div class="resizer-handle"></div>
        `;
    }

    getCurrentWidth(): number {
        return this.currentWidth || this.defaultWidth;
    }

    setWidth(width: number): void {
        const clampedWidth = Math.max(this.minWidth, Math.min(this.maxWidth, width));
        this.applyPanelWidth(clampedWidth);
        this.saveWidth(clampedWidth);
    }

    getWidthLimits(): WidthLimits {
        return {
            min: this.minWidth,
            max: this.maxWidth,
            default: this.defaultWidth
        };
    }
}

customElements.define('right-panel-resizer', RightPanelResizer);

export { RightPanelResizer };
export type { RightPanelResizedDetail, WidthLimits };
