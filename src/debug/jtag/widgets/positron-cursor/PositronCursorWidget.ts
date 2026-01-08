/**
 * PositronCursor - The AI's spatial presence in the interface
 *
 * Not a mouse cursor - a presence indicator showing where AI attention is focused.
 * Can point, highlight, draw attention, and later gesture.
 *
 * The orb in the corner is the AI's "eye" - this cursor is its "hand"
 */

import { BaseWidget } from '../shared/BaseWidget';
import { Events } from '../../system/core/shared/Events';

// Cursor modes
export type CursorMode = 'idle' | 'pointing' | 'highlighting' | 'drawing';

// Position target - can be coordinates or element selector
export interface CursorTarget {
  x?: number;
  y?: number;
  selector?: string;  // CSS selector to point at
  elementId?: string; // Specific element ID
}

// Event payloads
export interface PositronFocusEvent {
  personaId?: string;
  personaName?: string;
  target: CursorTarget;
  mode?: CursorMode;
  color?: string;
  duration?: number;  // Auto-hide after ms (0 = persistent)
  message?: string;   // Optional tooltip
}

export interface PositronDrawEvent {
  personaId?: string;
  personaName?: string;
  shape: 'circle' | 'rectangle' | 'arrow' | 'underline';
  target: CursorTarget;
  color?: string;
  duration?: number;
}

// Event names
export const POSITRON_CURSOR_EVENTS = {
  FOCUS: 'positron:focus',      // AI focuses attention on element
  UNFOCUS: 'positron:unfocus',  // AI releases focus
  DRAW: 'positron:draw',        // AI draws highlight shape
  CLEAR: 'positron:clear',      // Clear all drawings
  GESTURE: 'positron:gesture',  // Future: hand gestures
} as const;

export class PositronCursorWidget extends BaseWidget {
  private currentMode: CursorMode = 'idle';
  private currentPersona: string | null = null;
  private cursorElement: HTMLElement | null = null;
  private overlayCanvas: HTMLCanvasElement | null = null;
  private hideTimeout: ReturnType<typeof setTimeout> | null = null;
  private _eventUnsubscribers: Array<() => void> = [];

  constructor() {
    super({
      widgetName: 'PositronCursorWidget',
      enableAI: false,
      enableDatabase: false,
      enableRouterEvents: false,
      enableScreenshots: false
    });
  }

  protected async onWidgetInitialize(): Promise<void> {
    this.verbose() && console.log('👆 PositronCursor: Initializing AI presence cursor...');

    this.subscribeToEvents();

    this.verbose() && console.log('✅ PositronCursor: Ready');
  }

  private subscribeToEvents(): void {
    this._eventUnsubscribers.push(
      Events.subscribe(POSITRON_CURSOR_EVENTS.FOCUS, (data: PositronFocusEvent) => {
        this.handleFocus(data);
      }),
      Events.subscribe(POSITRON_CURSOR_EVENTS.UNFOCUS, () => {
        this.handleUnfocus();
      }),
      Events.subscribe(POSITRON_CURSOR_EVENTS.DRAW, (data: PositronDrawEvent) => {
        this.handleDraw(data);
      }),
      Events.subscribe(POSITRON_CURSOR_EVENTS.CLEAR, () => {
        this.clearOverlay();
      })
    );
  }

  private handleFocus(data: PositronFocusEvent): void {
    const { target, mode = 'pointing', color = '#00d4ff', duration = 0, personaName, message } = data;

    // Resolve target position
    const pos = this.resolveTarget(target);
    if (!pos) {
      console.warn('👆 PositronCursor: Could not resolve target', target);
      return;
    }

    this.currentMode = mode;
    this.currentPersona = personaName || null;

    // Position and show cursor
    this.showCursor(pos.x, pos.y, color, message);

    // Auto-hide if duration specified
    if (duration > 0) {
      if (this.hideTimeout) clearTimeout(this.hideTimeout);
      this.hideTimeout = setTimeout(() => this.handleUnfocus(), duration);
    }

    this.verbose() && console.log(`👆 PositronCursor: ${personaName || 'AI'} focusing at (${pos.x}, ${pos.y})`);
  }

  private handleUnfocus(): void {
    this.currentMode = 'idle';
    this.currentPersona = null;
    this.hideCursor();

    if (this.hideTimeout) {
      clearTimeout(this.hideTimeout);
      this.hideTimeout = null;
    }
  }

  private handleDraw(data: PositronDrawEvent): void {
    const { target, shape, color = '#00d4ff', duration = 3000 } = data;

    const pos = this.resolveTarget(target);
    if (!pos) return;

    this.drawShape(shape, pos, color);

    // Auto-clear drawing after duration
    if (duration > 0) {
      setTimeout(() => this.clearOverlay(), duration);
    }
  }

  private resolveTarget(target: CursorTarget): { x: number; y: number; width?: number; height?: number } | null {
    // Direct coordinates
    if (target.x !== undefined && target.y !== undefined) {
      return { x: target.x, y: target.y };
    }

    // Element selector - search through shadow DOMs
    if (target.selector || target.elementId) {
      const selector = target.selector || `#${target.elementId}`;
      const element = this.findElementDeep(selector);

      if (element) {
        const rect = element.getBoundingClientRect();
        return {
          x: rect.left + rect.width / 2,
          y: rect.top + rect.height / 2,
          width: rect.width,
          height: rect.height
        };
      }
    }

    return null;
  }

  /**
   * Find element through shadow DOMs
   */
  private findElementDeep(selector: string, root: Element | Document = document): Element | null {
    // Try direct query first
    let element = root.querySelector(selector);
    if (element) return element;

    // Search through shadow roots
    const allElements = Array.from(root.querySelectorAll('*'));
    for (const el of allElements) {
      if (el.shadowRoot) {
        element = this.findElementDeep(selector, el.shadowRoot as unknown as Document);
        if (element) return element;
      }
    }

    return null;
  }

  private showCursor(x: number, y: number, color: string, message?: string): void {
    if (!this.cursorElement) {
      console.warn('👆 PositronCursor: cursorElement is null - renderWidget may not have been called');
      return;
    }

    this.verbose() && console.log(`👆 PositronCursor: Showing cursor at (${x}, ${y}) with color ${color}`);
    this.cursorElement.style.left = `${x}px`;
    this.cursorElement.style.top = `${y}px`;
    this.cursorElement.style.setProperty('--cursor-color', color);
    this.cursorElement.classList.add('visible');
    this.cursorElement.classList.remove('idle');
    this.cursorElement.classList.add(this.currentMode);

    // Update tooltip
    const tooltip = this.cursorElement.querySelector('.cursor-tooltip') as HTMLElement;
    if (tooltip) {
      if (message || this.currentPersona) {
        tooltip.textContent = message || `${this.currentPersona} is looking here`;
        tooltip.style.display = 'block';
      } else {
        tooltip.style.display = 'none';
      }
    }
  }

  private hideCursor(): void {
    if (!this.cursorElement) return;

    this.cursorElement.classList.remove('visible', 'pointing', 'highlighting', 'drawing');
    this.cursorElement.classList.add('idle');
  }

  private drawShape(shape: string, pos: { x: number; y: number; width?: number; height?: number }, color: string): void {
    if (!this.overlayCanvas) return;

    const ctx = this.overlayCanvas.getContext('2d');
    if (!ctx) return;

    // Ensure canvas covers viewport
    this.overlayCanvas.width = window.innerWidth;
    this.overlayCanvas.height = window.innerHeight;

    // Make shapes highly visible with glow effect
    ctx.strokeStyle = color;
    ctx.lineWidth = 4;
    ctx.setLineDash([8, 4]);
    ctx.shadowColor = color;
    ctx.shadowBlur = 15;

    const width = pos.width || 100;
    const height = pos.height || 50;

    switch (shape) {
      case 'circle':
        ctx.beginPath();
        ctx.arc(pos.x, pos.y, Math.max(width, height) / 2 + 10, 0, Math.PI * 2);
        ctx.stroke();
        break;

      case 'rectangle':
        ctx.strokeRect(
          pos.x - width / 2 - 5,
          pos.y - height / 2 - 5,
          width + 10,
          height + 10
        );
        break;

      case 'underline':
        ctx.beginPath();
        ctx.moveTo(pos.x - width / 2, pos.y + height / 2 + 5);
        ctx.lineTo(pos.x + width / 2, pos.y + height / 2 + 5);
        ctx.stroke();
        break;

      case 'arrow':
        // Draw arrow pointing to position
        const arrowStart = { x: pos.x - 50, y: pos.y - 50 };
        ctx.beginPath();
        ctx.moveTo(arrowStart.x, arrowStart.y);
        ctx.lineTo(pos.x - 10, pos.y - 10);
        // Arrowhead
        ctx.lineTo(pos.x - 20, pos.y - 5);
        ctx.moveTo(pos.x - 10, pos.y - 10);
        ctx.lineTo(pos.x - 5, pos.y - 20);
        ctx.stroke();
        break;
    }
  }

  private clearOverlay(): void {
    if (!this.overlayCanvas) return;

    const ctx = this.overlayCanvas.getContext('2d');
    if (ctx) {
      ctx.clearRect(0, 0, this.overlayCanvas.width, this.overlayCanvas.height);
    }
  }

  protected async renderWidget(): Promise<void> {
    this.shadowRoot.innerHTML = `
      <style>
        :host {
          position: fixed;
          top: 0;
          left: 0;
          width: 100vw;
          height: 100vh;
          pointer-events: none;
          z-index: 10000;
        }

        .overlay-canvas {
          position: absolute;
          top: 0;
          left: 0;
          width: 100%;
          height: 100%;
        }

        .cursor {
          position: absolute;
          width: 40px;
          height: 40px;
          transform: translate(-50%, -50%);
          opacity: 0;
          transition: opacity 0.2s ease, transform 0.3s ease;
        }

        .cursor.visible {
          opacity: 1;
        }

        .cursor.idle {
          opacity: 0;
        }

        /* Cursor visual - glowing ring like the emoter orb */
        .cursor-ring {
          width: 100%;
          height: 100%;
          border-radius: 50%;
          border: 3px solid var(--cursor-color, #00d4ff);
          background: radial-gradient(circle,
            color-mix(in srgb, var(--cursor-color, #00d4ff) 20%, transparent) 0%,
            transparent 70%
          );
          box-shadow: 0 0 15px var(--cursor-color, #00d4ff),
                      0 0 30px var(--cursor-color, #00d4ff),
                      0 0 45px color-mix(in srgb, var(--cursor-color, #00d4ff) 50%, transparent);
          animation: pulse-cursor 1.5s ease-in-out infinite;
        }

        .cursor.pointing .cursor-ring {
          animation: pulse-point 0.8s ease-in-out infinite;
        }

        .cursor.highlighting .cursor-ring {
          width: 48px;
          height: 48px;
          animation: pulse-highlight 1s ease-in-out infinite;
        }

        @keyframes pulse-cursor {
          0%, 100% { opacity: 0.8; transform: scale(1); }
          50% { opacity: 1; transform: scale(1.1); }
        }

        @keyframes pulse-point {
          0%, 100% { opacity: 1; transform: scale(1); }
          50% { opacity: 0.7; transform: scale(0.9); }
        }

        @keyframes pulse-highlight {
          0%, 100% { opacity: 0.9; transform: scale(1); }
          50% { opacity: 1; transform: scale(1.15); }
        }

        /* Tooltip */
        .cursor-tooltip {
          position: absolute;
          top: 100%;
          left: 50%;
          transform: translateX(-50%);
          margin-top: 8px;
          padding: 4px 8px;
          background: rgba(0, 0, 0, 0.85);
          border: 1px solid var(--cursor-color, #00d4ff);
          border-radius: 4px;
          color: white;
          font-size: 11px;
          white-space: nowrap;
          display: none;
        }
      </style>

      <canvas class="overlay-canvas"></canvas>

      <div class="cursor idle">
        <div class="cursor-ring"></div>
        <div class="cursor-tooltip"></div>
      </div>
    `;

    this.cursorElement = this.shadowRoot.querySelector('.cursor');
    this.overlayCanvas = this.shadowRoot.querySelector('.overlay-canvas');
  }

  protected async onWidgetCleanup(): Promise<void> {
    for (const unsub of this._eventUnsubscribers) {
      try { unsub(); } catch { /* ignore */ }
    }
    this._eventUnsubscribers = [];

    if (this.hideTimeout) {
      clearTimeout(this.hideTimeout);
    }
  }
}
