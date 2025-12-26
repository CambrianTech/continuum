/**
 * PanelResizer - Unified draggable resizer for sidebar panels
 *
 * Works for both left sidebar and right panel with configurable:
 * - Side (left/right) - determines positioning and drag direction
 * - Collapsible - optional collapse with expand button
 * - CSS var prefix - reads dimensions from theme
 * - Storage key - localStorage persistence
 *
 * Features:
 * - Drag to resize
 * - Drag past threshold to collapse (if enabled)
 * - Double-click to toggle collapse (if enabled)
 * - Expand button when collapsed (if enabled)
 * - All colors from CSS theme vars
 * - Persists state to localStorage
 */

export interface PanelResizerConfig {
    /** Which side of the layout ('left' for sidebar, 'right' for right panel) */
    side: 'left' | 'right';

    /** CSS variable prefix for dimensions (e.g., 'sidebar' reads --sidebar-width) */
    cssVarPrefix: string;

    /** Container class to resize (e.g., 'sidebar-container' or 'right-panel-container') */
    containerClass: string;

    /** Enable collapse functionality */
    collapsible?: boolean;

    /** LocalStorage key prefix for persistence */
    storageKeyPrefix: string;
}

interface PanelResizedDetail {
    width: number;
    collapsed: boolean;
}

interface WidthLimits {
    min: number;
    max: number;
    default: number;
}

export class PanelResizer extends HTMLElement {
    private config!: PanelResizerConfig;
    private isDragging: boolean = false;
    private startX: number = 0;
    private startWidth: number = 0;
    private currentWidth?: number;
    private isCollapsed: boolean = false;
    private lastExpandedWidth: number = 320;

    // Read from CSS vars
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

    /**
     * Configure the resizer. Must be called before connectedCallback or via attribute.
     */
    configure(config: PanelResizerConfig): void {
        this.config = config;
    }

    connectedCallback(): void {
        // Try to get config from attributes if not set programmatically
        if (!this.config) {
            const side = this.getAttribute('side') as 'left' | 'right' || 'right';
            const cssVarPrefix = this.getAttribute('css-var-prefix') || (side === 'left' ? 'sidebar' : 'right-panel');
            const containerClass = this.getAttribute('container-class') || (side === 'left' ? 'sidebar-container' : 'right-panel-container');
            const collapsible = this.getAttribute('collapsible') !== 'false';
            const storageKeyPrefix = this.getAttribute('storage-key-prefix') || `continuum-${side}-panel`;

            this.config = {
                side,
                cssVarPrefix,
                containerClass,
                collapsible,
                storageKeyPrefix
            };
        }

        this.loadCSSVars();
        this.render();
        this.setupEventListeners();
        this.loadSavedState();
    }

    disconnectedCallback(): void {
        this.removeEventListeners();
    }

    /**
     * Read layout dimensions from CSS vars - single source of truth
     */
    private loadCSSVars(): void {
        const styles = getComputedStyle(document.documentElement);
        const prefix = this.config.cssVarPrefix;

        const getCSSPixelValue = (varName: string, fallback: number): number => {
            const value = styles.getPropertyValue(varName).trim();
            if (!value) return fallback;
            const parsed = parseInt(value, 10);
            return isNaN(parsed) ? fallback : parsed;
        };

        this.minWidth = getCSSPixelValue(`--${prefix}-min-width`, 150);
        this.maxWidth = getCSSPixelValue(`--${prefix}-max-width`, 600);
        this.defaultWidth = getCSSPixelValue(`--${prefix}-width`, 320);
        this.collapseThreshold = getCSSPixelValue(`--${prefix}-collapse-threshold`, 50);
        this.collapsedHandleWidth = getCSSPixelValue(`--${prefix}-collapsed-width`, 6);
        this.lastExpandedWidth = this.defaultWidth;
    }

    private loadSavedState(): void {
        const prefix = this.config.storageKeyPrefix;

        try {
            // Load collapsed state
            if (this.config.collapsible) {
                const savedCollapsed = localStorage.getItem(`${prefix}-collapsed`);
                if (savedCollapsed === 'true') {
                    this.isCollapsed = true;
                    this.applyCollapsedState(true);
                    this.updateHostClass();
                    return;
                }
            }

            const savedWidth = localStorage.getItem(`${prefix}-width`);
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
        const prefix = this.config.storageKeyPrefix;
        try {
            localStorage.setItem(`${prefix}-width`, width.toString());
            localStorage.setItem(`${prefix}-collapsed`, 'false');
        } catch {
            // Ignore errors
        }
    }

    private saveCollapsedState(collapsed: boolean): void {
        const prefix = this.config.storageKeyPrefix;
        try {
            localStorage.setItem(`${prefix}-collapsed`, collapsed.toString());
        } catch {
            // Ignore errors
        }
    }

    /**
     * Collapse the panel to a thin handle
     */
    collapse(): void {
        if (!this.config.collapsible || this.isCollapsed) return;

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
        const panelContainer = continuumWidget.shadowRoot.querySelector(`.${this.config.containerClass}`) as HTMLElement;

        if (!desktopContainer || !panelContainer) return;

        if (collapsed) {
            panelContainer.style.width = `${this.collapsedHandleWidth}px`;
            panelContainer.classList.add('collapsed');

            const currentCols = getComputedStyle(desktopContainer).gridTemplateColumns.split(' ');

            if (this.config.side === 'left') {
                const rightPanelWidth = currentCols[2] || 'var(--right-panel-width, 320px)';
                desktopContainer.style.gridTemplateColumns = `${this.collapsedHandleWidth}px 1fr ${rightPanelWidth}`;
            } else {
                const sidebarWidth = currentCols[0] || 'var(--sidebar-width, 250px)';
                desktopContainer.style.gridTemplateColumns = `${sidebarWidth} 1fr ${this.collapsedHandleWidth}px`;
            }

            this.currentWidth = 0;

            this.dispatchEvent(new CustomEvent<PanelResizedDetail>('panel-resized', {
                detail: { width: 0, collapsed: true },
                bubbles: true
            }));
        } else {
            panelContainer.classList.remove('collapsed');
        }
    }

    private applyPanelWidth(width: number, clipping: boolean = false): void {
        const continuumWidget = document.querySelector('continuum-widget') as any;
        if (!continuumWidget?.shadowRoot) return;

        const desktopContainer = continuumWidget.shadowRoot.querySelector('.desktop-container') as HTMLElement;
        const panelContainer = continuumWidget.shadowRoot.querySelector(`.${this.config.containerClass}`) as HTMLElement;

        if (!panelContainer || !desktopContainer) return;

        panelContainer.style.width = `${width}px`;

        if (clipping) {
            panelContainer.style.overflow = 'hidden';
            panelContainer.classList.add('clipping');
        } else {
            panelContainer.style.overflow = '';
            panelContainer.classList.remove('clipping');
        }

        const currentCols = getComputedStyle(desktopContainer).gridTemplateColumns.split(' ');

        if (this.config.side === 'left') {
            const rightPanelWidth = currentCols[2] || 'var(--right-panel-width, 320px)';
            desktopContainer.style.gridTemplateColumns = `${width}px 1fr ${rightPanelWidth}`;
        } else {
            const sidebarWidth = currentCols[0] || 'var(--sidebar-width, 250px)';
            desktopContainer.style.gridTemplateColumns = `${sidebarWidth} 1fr ${width}px`;
        }

        this.currentWidth = width;

        this.dispatchEvent(new CustomEvent<PanelResizedDetail>('panel-resized', {
            detail: { width, collapsed: false },
            bubbles: true
        }));
    }

    private setupEventListeners(): void {
        this.boundMouseMove = this.handleMouseMove.bind(this);
        this.boundMouseUp = this.handleMouseUp.bind(this);
        this.boundTouchMove = this.handleTouchMove.bind(this);
        this.boundTouchEnd = this.handleTouchEnd.bind(this);

        this.shadowRoot?.addEventListener('mousedown', this.handleMouseDown.bind(this));

        if (this.config.collapsible) {
            this.shadowRoot?.addEventListener('dblclick', this.handleDoubleClick.bind(this));

            const expandBtn = this.shadowRoot?.querySelector('.expand-btn');
            if (expandBtn) {
                expandBtn.addEventListener('click', (e) => {
                    e.stopPropagation();
                    this.expand();
                });
            }
        }

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
        document.documentElement.classList.add(`${this.config.side}-panel-dragging`);
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
        // For left sidebar: dragging RIGHT increases width (+deltaX)
        // For right panel: dragging LEFT increases width (-deltaX)
        const deltaX = this.config.side === 'left'
            ? clientX - this.startX
            : this.startX - clientX;

        const rawWidth = this.startWidth + deltaX;

        // If collapsed and dragging to expand
        if (this.isCollapsed && this.config.collapsible) {
            if (rawWidth >= this.minWidth) {
                this.isCollapsed = false;
                this.shadowRoot?.host.classList.remove('collapsed');
                const continuumWidget = document.querySelector('continuum-widget') as any;
                if (continuumWidget?.shadowRoot) {
                    const panelContainer = continuumWidget.shadowRoot.querySelector(`.${this.config.containerClass}`) as HTMLElement;
                    if (panelContainer) {
                        panelContainer.classList.remove('collapsed');
                    }
                }
                this.applyPanelWidth(this.minWidth);
            }
            return;
        }

        // Below snap threshold → collapse completely (if collapsible)
        if (this.config.collapsible && rawWidth < this.collapseThreshold) {
            this.collapse();
            return;
        }

        // Below minWidth but above snap → allow with clipping
        if (this.config.collapsible && rawWidth < this.minWidth) {
            this.applyPanelWidth(rawWidth, true);
            return;
        }

        // Normal resize within bounds
        const newWidth = Math.max(this.minWidth, Math.min(this.maxWidth, rawWidth));
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
            document.documentElement.classList.remove(`${this.config.side}-panel-dragging`);
            this.shadowRoot?.host.classList.remove('dragging');

            if (this.currentWidth !== undefined && this.currentWidth > 0) {
                this.saveWidth(this.currentWidth);
            }
        }
    }

    private render(): void {
        if (!this.shadowRoot) return;

        const isLeft = this.config.side === 'left';
        const prefix = this.config.cssVarPrefix;

        // Position: left sidebar has resizer on RIGHT edge, right panel on LEFT edge
        const positionStyles = isLeft
            ? 'right: -1px; border-right: 1px solid var(--resizer-border, rgba(0, 212, 255, 0.2));'
            : 'left: -1px; border-left: 1px solid var(--resizer-border, rgba(0, 212, 255, 0.2));';

        const hoverBorder = isLeft ? 'border-right' : 'border-left';
        const activeBorder = isLeft ? 'border-right' : 'border-left';

        // Expand button position for collapsed state
        const expandBtnPosition = isLeft ? 'left: 6px;' : 'right: var(--right-panel-collapsed-width, 6px);';

        this.shadowRoot.innerHTML = `
            <style>
                :host {
                    position: absolute;
                    top: 0;
                    bottom: 0;
                    ${positionStyles}
                    width: var(--resizer-width, 2px);
                    cursor: col-resize;
                    z-index: 1000;
                    background: var(--resizer-background, rgba(0, 212, 255, 0.1));
                    transition: all 0.2s ease;
                    overflow: visible;
                }

                :host(:hover) {
                    background: var(--resizer-background-hover, rgba(0, 212, 255, 0.3));
                    ${hoverBorder}: 1px solid var(--resizer-border-hover, #00d4ff);
                    box-shadow: 0 0 4px var(--resizer-glow-hover, rgba(0, 212, 255, 0.5));
                }

                :host(.dragging) {
                    background: var(--resizer-background-active, rgba(0, 212, 255, 0.6));
                    ${activeBorder}: 2px solid var(--resizer-border-active, #00d4ff);
                    box-shadow: 0 0 6px var(--resizer-glow-active, rgba(0, 212, 255, 0.8));
                    width: var(--resizer-width-active, 3px);
                }

                /* Collapsed state - thin glowing handle */
                :host(.collapsed) {
                    width: var(--${prefix}-collapsed-width, 6px);
                    ${isLeft ? 'right: 0;' : 'left: 0;'}
                    background: var(--resizer-background, rgba(0, 212, 255, 0.15));
                    ${hoverBorder}: 1px solid var(--resizer-border, rgba(0, 212, 255, 0.3));
                }

                :host(.collapsed:hover) {
                    background: var(--resizer-background-hover, rgba(0, 212, 255, 0.4));
                    ${hoverBorder}: 1px solid var(--resizer-border-hover, #00d4ff);
                    box-shadow: 0 0 8px var(--resizer-glow-hover, rgba(0, 212, 255, 0.6));
                    cursor: ${isLeft ? 'w-resize' : 'e-resize'};
                }

                .resizer-handle {
                    position: absolute;
                    top: 0;
                    bottom: 0;
                    left: -4px;
                    right: -4px;
                    background: transparent;
                }

                /* Expand button - ONLY visible when collapsed, fixed position */
                .expand-btn {
                    position: fixed;
                    top: 50%;
                    ${expandBtnPosition}
                    transform: translateY(-50%);
                    width: 18px;
                    height: 36px;
                    background: var(--sidebar-background, rgba(20, 25, 35, 0.95));
                    border: 1px solid var(--border-accent, rgba(0, 212, 255, 0.4));
                    border-radius: ${isLeft ? '0 4px 4px 0' : '4px 0 0 4px'};
                    color: var(--content-accent, #00d4ff);
                    font-size: 11px;
                    cursor: pointer;
                    display: none;
                    align-items: center;
                    justify-content: center;
                    z-index: 2000;
                    transition: all 0.2s ease;
                }

                :host(.collapsed) .expand-btn {
                    display: flex;
                    opacity: 0.8;
                }

                .expand-btn:hover {
                    opacity: 1;
                    background: var(--resizer-background-hover, rgba(0, 212, 255, 0.2));
                    border-color: var(--resizer-border-hover, #00d4ff);
                    box-shadow: 0 0 8px var(--resizer-glow-hover, rgba(0, 212, 255, 0.5));
                }
            </style>
            <div class="resizer-handle"></div>
            ${this.config.collapsible ? `<button class="expand-btn" title="Expand panel">${isLeft ? '»' : '«'}</button>` : ''}
        `;
    }

    // Public API
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

    isCurrentlyCollapsed(): boolean {
        return this.isCollapsed;
    }
}

// Don't auto-register - let specific implementations do that
export type { PanelResizedDetail, WidthLimits };
