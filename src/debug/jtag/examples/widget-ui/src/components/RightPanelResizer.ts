/**
 * RightPanelResizer - Draggable resizer for right panel
 *
 * Better than VS Code:
 * - Drag past threshold to collapse (no buttons needed)
 * - Thin glowing handle when collapsed
 * - Double-click to toggle collapse/expand
 * - Smooth animations
 */

interface RightPanelResizedDetail {
    width: number;
    collapsed: boolean;
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
    private isCollapsed: boolean = false;
    private lastExpandedWidth: number = 320;

    // These are read from CSS vars in connectedCallback
    private minWidth: number = 150;
    private maxWidth: number = 600;
    private defaultWidth: number = 320;
    private collapseThreshold: number = 50;
    private collapsedHandleWidth: number = 6;

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
        this.loadCSSVars();
        this.render();
        this.setupEventListeners();
        this.loadSavedWidth();
    }

    /**
     * Read layout dimensions from CSS vars - single source of truth
     */
    private loadCSSVars(): void {
        const styles = getComputedStyle(document.documentElement);

        const getCSSPixelValue = (varName: string, fallback: number): number => {
            const value = styles.getPropertyValue(varName).trim();
            if (!value) return fallback;
            const parsed = parseInt(value, 10);
            return isNaN(parsed) ? fallback : parsed;
        };

        this.minWidth = getCSSPixelValue('--right-panel-min-width', 150);
        this.maxWidth = getCSSPixelValue('--right-panel-max-width', 600);
        this.defaultWidth = getCSSPixelValue('--right-panel-width', 320);
        this.collapseThreshold = getCSSPixelValue('--right-panel-collapse-threshold', 50);
        this.collapsedHandleWidth = getCSSPixelValue('--right-panel-collapsed-width', 6);
        this.lastExpandedWidth = this.defaultWidth;
    }

    disconnectedCallback(): void {
        this.removeEventListeners();
    }

    private loadSavedWidth(): void {
        try {
            // Load collapsed state
            const savedCollapsed = localStorage.getItem('continuum-right-panel-collapsed');
            if (savedCollapsed === 'true') {
                this.isCollapsed = true;
                this.applyCollapsedState(true);
                return;
            }

            const savedWidth = localStorage.getItem('continuum-right-panel-width');
            if (savedWidth) {
                const width = parseInt(savedWidth, 10);
                if (!isNaN(width) && width >= this.minWidth && width <= this.maxWidth) {
                    this.lastExpandedWidth = width;
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
            localStorage.setItem('continuum-right-panel-collapsed', 'false');
        } catch {
            // Ignore errors
        }
    }

    private saveCollapsedState(collapsed: boolean): void {
        try {
            localStorage.setItem('continuum-right-panel-collapsed', collapsed.toString());
        } catch {
            // Ignore errors
        }
    }

    /**
     * Collapse the panel to a thin handle
     */
    collapse(): void {
        if (this.isCollapsed) return;

        // Save current width for restore
        if (this.currentWidth && this.currentWidth >= this.minWidth) {
            this.lastExpandedWidth = this.currentWidth;
        }

        this.isCollapsed = true;
        this.applyCollapsedState(true);
        this.saveCollapsedState(true);
        this.updateHostClass();
    }

    /**
     * Expand the panel to last width
     */
    expand(): void {
        if (!this.isCollapsed) return;

        this.isCollapsed = false;
        this.applyCollapsedState(false);
        this.applyPanelWidth(this.lastExpandedWidth);
        this.saveCollapsedState(false);
        this.saveWidth(this.lastExpandedWidth);
        this.updateHostClass();
    }

    /**
     * Toggle collapse state
     */
    toggle(): void {
        if (this.isCollapsed) {
            this.expand();
        } else {
            this.collapse();
        }
    }

    private updateHostClass(): void {
        if (this.isCollapsed) {
            this.shadowRoot?.host.classList.add('collapsed');
        } else {
            this.shadowRoot?.host.classList.remove('collapsed');
        }
    }

    private applyCollapsedState(collapsed: boolean): void {
        const continuumWidget = document.querySelector('continuum-widget') as any;
        if (!continuumWidget?.shadowRoot) return;

        const desktopContainer = continuumWidget.shadowRoot.querySelector('.desktop-container') as HTMLElement;
        const rightPanelContainer = continuumWidget.shadowRoot.querySelector('.right-panel-container') as HTMLElement;

        if (!desktopContainer || !rightPanelContainer) return;

        if (collapsed) {
            // Collapse to thin handle
            rightPanelContainer.style.width = `${this.collapsedHandleWidth}px`;
            rightPanelContainer.classList.add('collapsed');

            const currentCols = getComputedStyle(desktopContainer).gridTemplateColumns.split(' ');
            const sidebarWidth = currentCols[0] || 'var(--sidebar-width, 250px)';
            desktopContainer.style.gridTemplateColumns = `${sidebarWidth} 1fr ${this.collapsedHandleWidth}px`;

            this.currentWidth = 0;

            // Dispatch event
            this.dispatchEvent(new CustomEvent<RightPanelResizedDetail>('right-panel-resized', {
                detail: { width: 0, collapsed: true },
                bubbles: true
            }));
        } else {
            rightPanelContainer.classList.remove('collapsed');
        }
    }

    private applyPanelWidth(width: number, clipping: boolean = false): void {
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

            // Apply overflow clipping when dragging smaller than min
            if (clipping) {
                rightPanelContainer.style.overflow = 'hidden';
                rightPanelContainer.classList.add('clipping');
            } else {
                rightPanelContainer.style.overflow = '';
                rightPanelContainer.classList.remove('clipping');
            }

            // Get current sidebar width from the grid
            const currentCols = getComputedStyle(desktopContainer).gridTemplateColumns.split(' ');
            const sidebarWidth = currentCols[0] || 'var(--sidebar-width, 250px)';

            desktopContainer.style.gridTemplateColumns = `${sidebarWidth} 1fr ${width}px`;

            this.currentWidth = width;

            const event = new CustomEvent<RightPanelResizedDetail>('right-panel-resized', {
                detail: { width, collapsed: false },
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
        this.shadowRoot?.addEventListener('dblclick', this.handleDoubleClick.bind(this));
        document.addEventListener('mousemove', this.boundMouseMove);
        document.addEventListener('mouseup', this.boundMouseUp);

        this.shadowRoot?.addEventListener('touchstart', this.handleTouchStart.bind(this));
        document.addEventListener('touchmove', this.boundTouchMove);
        document.addEventListener('touchend', this.boundTouchEnd);
    }

    private handleDoubleClick(e: MouseEvent): void {
        e.preventDefault();
        this.toggle();
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
        const rawWidth = this.startWidth + deltaX;

        // If collapsed and dragging to expand
        if (this.isCollapsed) {
            if (rawWidth >= this.minWidth) {
                // Expand when dragged past minimum
                this.isCollapsed = false;
                this.shadowRoot?.host.classList.remove('collapsed');
                const continuumWidget = document.querySelector('continuum-widget') as any;
                if (continuumWidget?.shadowRoot) {
                    const rightPanelContainer = continuumWidget.shadowRoot.querySelector('.right-panel-container') as HTMLElement;
                    if (rightPanelContainer) {
                        rightPanelContainer.classList.remove('collapsed');
                    }
                }
                this.applyPanelWidth(this.minWidth);
            }
            return;
        }

        // Below snap threshold → collapse completely
        const snapThreshold = 50;
        if (rawWidth < snapThreshold) {
            this.collapse();
            return;
        }

        // Below minWidth but above snap → allow with clipping (content clips off)
        // This gives visual feedback that panel is closing
        if (rawWidth < this.minWidth) {
            this.applyPanelWidth(rawWidth, true); // true = clipping mode
            return;
        }

        // Normal resize within bounds
        const newWidth = Math.min(this.maxWidth, rawWidth);
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

                /* Collapsed state - thin glowing handle */
                :host(.collapsed) {
                    width: var(--right-panel-collapsed-width, 6px);
                    left: 0;
                    background: rgba(0, 212, 255, 0.15);
                    border-left: 1px solid rgba(0, 212, 255, 0.3);
                }

                :host(.collapsed:hover) {
                    background: rgba(0, 212, 255, 0.4);
                    border-left: 1px solid #00d4ff;
                    box-shadow: 0 0 8px rgba(0, 212, 255, 0.6);
                    cursor: e-resize;
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
