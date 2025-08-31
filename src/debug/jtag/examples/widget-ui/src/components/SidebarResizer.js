/**
 * SidebarResizer - Draggable resizer for desktop layout
 * Handles sidebar width with localStorage persistence
 */
class SidebarResizer extends HTMLElement {
    constructor() {
        super();
        this.attachShadow({ mode: 'open' });
        this.isDragging = false;
        this.startX = 0;
        this.startWidth = 0;
        this.minWidth = 150;
        this.maxWidth = 500;
        this.defaultWidth = 250;
        this.storageKey = 'continuum-sidebar-width';
    }

    connectedCallback() {
        this.render();
        this.setupEventListeners();
        this.loadSavedWidth();
    }

    disconnectedCallback() {
        this.removeEventListeners();
    }

    loadSavedWidth() {
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
            console.warn('SidebarResizer: Failed to load saved width:', error.message);
        }
        
        // Use default width if no valid saved width
        this.applySidebarWidth(this.defaultWidth);
    }

    saveWidth(width) {
        try {
            localStorage.setItem(this.storageKey, width.toString());
            console.log('🔧 SidebarResizer: Saved width:', width);
        } catch (error) {
            console.warn('SidebarResizer: Failed to save width:', error.message);
        }
    }

    applySidebarWidth(width) {
        const desktopContainer = document.querySelector('.desktop-container');
        if (desktopContainer) {
            desktopContainer.style.gridTemplateColumns = `${width}px 1fr`;
            this.currentWidth = width;
            
            // Dispatch custom event for other components
            this.dispatchEvent(new CustomEvent('sidebar-resized', {
                detail: { width },
                bubbles: true
            }));
        }
    }

    setupEventListeners() {
        this.shadowRoot.addEventListener('mousedown', this.handleMouseDown.bind(this));
        document.addEventListener('mousemove', this.handleMouseMove.bind(this));
        document.addEventListener('mouseup', this.handleMouseUp.bind(this));
        
        // Touch events for mobile
        this.shadowRoot.addEventListener('touchstart', this.handleTouchStart.bind(this));
        document.addEventListener('touchmove', this.handleTouchMove.bind(this));
        document.addEventListener('touchend', this.handleTouchEnd.bind(this));
    }

    removeEventListeners() {
        document.removeEventListener('mousemove', this.handleMouseMove.bind(this));
        document.removeEventListener('mouseup', this.handleMouseUp.bind(this));
        document.removeEventListener('touchmove', this.handleTouchMove.bind(this));
        document.removeEventListener('touchend', this.handleTouchEnd.bind(this));
    }

    handleMouseDown(e) {
        this.startDrag(e.clientX);
        e.preventDefault();
    }

    handleTouchStart(e) {
        if (e.touches.length === 1) {
            this.startDrag(e.touches[0].clientX);
            e.preventDefault();
        }
    }

    startDrag(clientX) {
        this.isDragging = true;
        this.startX = clientX;
        this.startWidth = this.currentWidth || this.defaultWidth;
        
        // Set cursor for dragging (text selection already disabled globally)
        document.body.style.cursor = 'col-resize';
        
        // Add dragging class to root for additional styling
        document.documentElement.classList.add('sidebar-dragging');
        
        // Visual feedback on resizer
        this.shadowRoot.host.classList.add('dragging');
        
        console.log('🔧 SidebarResizer: Started drag', { startX: this.startX, startWidth: this.startWidth });
    }

    handleMouseMove(e) {
        if (this.isDragging) {
            this.doDrag(e.clientX);
        }
    }

    handleTouchMove(e) {
        if (this.isDragging && e.touches.length === 1) {
            this.doDrag(e.touches[0].clientX);
            e.preventDefault();
        }
    }

    doDrag(clientX) {
        const deltaX = clientX - this.startX;
        const newWidth = Math.max(this.minWidth, Math.min(this.maxWidth, this.startWidth + deltaX));
        
        this.applySidebarWidth(newWidth);
    }

    handleMouseUp() {
        this.endDrag();
    }

    handleTouchEnd() {
        this.endDrag();
    }

    endDrag() {
        if (this.isDragging) {
            this.isDragging = false;
            
            // Remove dragging feedback
            document.body.style.cursor = '';
            
            // Remove dragging class from root
            document.documentElement.classList.remove('sidebar-dragging');
            
            // Remove visual feedback on resizer
            this.shadowRoot.host.classList.remove('dragging');
            
            // Save the final width
            this.saveWidth(this.currentWidth);
            
            console.log('🔧 SidebarResizer: Ended drag, final width:', this.currentWidth);
        }
    }

    render() {
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
    getCurrentWidth() {
        return this.currentWidth || this.defaultWidth;
    }

    setWidth(width) {
        const clampedWidth = Math.max(this.minWidth, Math.min(this.maxWidth, width));
        this.applySidebarWidth(clampedWidth);
        this.saveWidth(clampedWidth);
    }

    resetToDefault() {
        this.setWidth(this.defaultWidth);
    }

    getWidthLimits() {
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