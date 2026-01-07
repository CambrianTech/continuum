/**
 * DrawingCanvasWidget - Shared visual workspace for humans and AIs
 *
 * A collaborative drawing canvas where:
 * - Humans can draw freehand, shapes, and annotations
 * - Vision-capable AIs can "see" the canvas content
 * - AIs can draw using the positron/cursor commands
 * - Everyone sees changes in real-time
 *
 * The canvas content can be captured and sent to vision models (GPT-4V, Claude 3)
 * for description, analysis, and conversation about what's being drawn.
 *
 * Collaborative features:
 * - Strokes persist to database (canvas_strokes collection)
 * - Real-time sync via Events (canvas:stroke:added)
 * - Multiple canvas instances (activityId = canvas instance)
 * - Full stroke replay for new participants
 */

import { BaseWidget } from '../shared/BaseWidget';
import { Events } from '../../system/core/shared/Events';
import { Commands } from '../../system/core/shared/Commands';
import type { CommandParams, CommandResult } from '../../system/core/types/JTAGTypes';
import type { StrokePoint, CanvasTool } from '../../system/data/entities/CanvasStrokeEntity';
import type { CanvasStrokeAddParams, CanvasStrokeAddResult } from '../../commands/canvas/stroke/add/shared/CanvasStrokeAddTypes';
import type { CanvasStrokeListParams, CanvasStrokeListResult, StrokeData } from '../../commands/canvas/stroke/list/shared/CanvasStrokeListTypes';
import type { UUID } from '../../system/core/types/CrossPlatformUUID';
import { ACTIVITY_UNIQUE_IDS } from '../../system/data/constants/ActivityConstants';
import { PositronWidgetState } from '../shared/services/state/PositronWidgetState';

// Verbose logging helper for browser
const verbose = () => typeof window !== 'undefined' && (window as any).JTAG_VERBOSE === true;

/**
 * Result from user/get-me command
 */
interface UserGetMeResult extends CommandResult {
  success: boolean;
  user?: {
    id: UUID;
    displayName: string;
  };
}

// Drawing modes
export type DrawingTool = 'brush' | 'eraser' | 'line' | 'rectangle' | 'circle' | 'arrow' | 'text';

// Brush settings
export interface BrushSettings {
  color: string;
  size: number;
  opacity: number;
}

// Drawing action for undo/redo
interface DrawingAction {
  type: 'stroke' | 'shape' | 'clear';
  data: ImageData | null;
  timestamp: number;
}

// Events for AI integration
export const DRAWING_CANVAS_EVENTS = {
  STROKE_ADDED: 'canvas:stroke:added',
  CANVAS_CLEARED: 'canvas:cleared',
  CANVAS_CAPTURED: 'canvas:captured',
  AI_DRAW_REQUEST: 'canvas:ai:draw',
} as const;

export class DrawingCanvasWidget extends BaseWidget {
  private canvas: HTMLCanvasElement | null = null;
  private ctx: CanvasRenderingContext2D | null = null;
  private isDrawing = false;
  private lastX = 0;
  private lastY = 0;
  private currentTool: DrawingTool = 'brush';
  private brushSettings: BrushSettings = {
    color: '#00d4ff',
    size: 4,
    opacity: 1.0
  };
  private history: DrawingAction[] = [];
  private historyIndex = -1;
  private shapeStartX = 0;
  private shapeStartY = 0;
  private previewCanvas: HTMLCanvasElement | null = null;

  // Collaborative canvas properties
  private _activityId: UUID | null = null;
  private currentStrokePoints: StrokePoint[] = [];
  private _userId: UUID | null = null;
  private _userName: string = 'Unknown';
  private strokeEventUnsubscribe: (() => void) | null = null;
  private loadedStrokeIds = new Set<string>(); // Prevent re-rendering same stroke

  constructor() {
    super({
      widgetName: 'DrawingCanvasWidget',
      enableAI: false,
      enableDatabase: true, // Enable for stroke persistence
      enableRouterEvents: true,
      enableScreenshots: true
    });
  }

  /**
   * Get the canvas activity ID (canvas instance)
   * Uses the seeded CANVAS_MAIN activity by default
   */
  get activityId(): UUID | null {
    // Check attribute first (set by recipe/content system)
    const attrId = this.getAttribute('activity-id') || this.getAttribute('entity-id');
    // Use default canvas if no specific ID provided (allows testing)
    // ACTIVITY_UNIQUE_IDS.CANVAS_MAIN matches the seeded activity's uniqueId
    return attrId || this._activityId || ACTIVITY_UNIQUE_IDS.CANVAS_MAIN;
  }

  /**
   * Set the canvas activity ID
   */
  set activityId(id: UUID | null) {
    this._activityId = id;
  }

  protected async onWidgetInitialize(): Promise<void> {
    verbose() && console.log('🎨 DrawingCanvas: Initializing collaborative canvas...');

    // Get current user info for stroke attribution
    await this.loadUserInfo();

    // Subscribe to AI draw requests
    Events.subscribe(DRAWING_CANVAS_EVENTS.AI_DRAW_REQUEST, (data: any) => {
      this.handleAIDrawRequest(data);
    });

    // Subscribe to real-time stroke events from other users
    this.subscribeToStrokeEvents();

    // Emit initial Positron context so AIs know canvas is active
    this.emitPositronContext();

    // Note: loadStrokes is called from setupCanvas after ctx is ready
    // This is because renderWidget runs AFTER onWidgetInitialize

    verbose() && console.log('✅ DrawingCanvas: Ready for collaborative drawing');
  }

  /**
   * Emit Positron context for AI awareness
   * Called on initialization and after significant canvas changes
   */
  private emitPositronContext(): void {
    PositronWidgetState.emit({
      widgetType: 'drawing-canvas',
      title: 'Collaborative Canvas',
      entityId: this.activityId || undefined,
      metadata: {
        activityId: this.activityId,
        strokeCount: this.loadedStrokeIds.size,
        currentTool: this.currentTool,
        brushColor: this.brushSettings.color,
        brushSize: this.brushSettings.size,
        description: `Collaborative canvas with ${this.loadedStrokeIds.size} strokes. Users and AIs can draw together.`
      }
    }, {
      action: 'viewing',
      target: 'canvas',
      details: `Viewing canvas ${this.activityId} with ${this.loadedStrokeIds.size} strokes`
    });
  }

  /**
   * Load current user info for stroke attribution
   */
  private async loadUserInfo(): Promise<void> {
    try {
      const result = await Commands.execute<CommandParams, UserGetMeResult>('user/get-me', {});
      if (result.success && result.user) {
        this._userId = result.user.id;
        this._userName = result.user.displayName || 'Unknown';
        verbose() && console.log(`🎨 DrawingCanvas: User identified as ${this._userName}`);
      }
    } catch (err) {
      console.warn('🎨 DrawingCanvas: Could not get user info, using session ID');
    }
  }

  /**
   * Subscribe to stroke events for real-time collaboration
   */
  private subscribeToStrokeEvents(): void {
    // Listen for strokes from other users
    this.strokeEventUnsubscribe = Events.subscribe(
      DRAWING_CANVAS_EVENTS.STROKE_ADDED,
      (data: {
        canvasId: UUID;
        strokeId: UUID;
        stroke: StrokeData;
      }) => {
        // Only process strokes for our canvas
        if (data.canvasId !== this.activityId) return;

        // Don't re-render strokes we already have
        if (this.loadedStrokeIds.has(data.strokeId)) return;

        // Don't render our own strokes (we drew them locally)
        if (data.stroke.creatorId === this._userId) return;

        verbose() && console.log(`🎨 DrawingCanvas: Received stroke from ${data.stroke.creatorName}`);
        this.renderRemoteStroke(data.stroke);
        this.loadedStrokeIds.add(data.strokeId);
      }
    );
  }

  /**
   * Load all strokes for this canvas from the database
   */
  private async loadStrokes(): Promise<void> {
    if (!this.activityId) return;

    try {
      const result = await Commands.execute<CanvasStrokeListParams, CanvasStrokeListResult>(
        'canvas/stroke/list',
        {
          canvasId: this.activityId,
          limit: 1000 // Load up to 1000 strokes for replay
        }
      );

      if (result.success && result.strokes) {
        verbose() && console.log(`🎨 DrawingCanvas: Loading ${result.strokes.length} strokes`);
        for (const stroke of result.strokes) {
          if (!this.loadedStrokeIds.has(stroke.id)) {
            this.renderRemoteStroke(stroke);
            this.loadedStrokeIds.add(stroke.id);
          }
        }
        // Save final state to history
        this.saveToHistory();
      }
    } catch (err) {
      console.error('🎨 DrawingCanvas: Failed to load strokes:', err);
    }
  }

  /**
   * Render a stroke from another user (or from database load)
   */
  private renderRemoteStroke(stroke: StrokeData): void {
    if (!this.ctx) return;

    // Save current state
    const savedColor = this.brushSettings.color;
    const savedSize = this.brushSettings.size;
    const savedOpacity = this.brushSettings.opacity;
    const savedTool = this.currentTool;

    // Apply stroke settings
    this.brushSettings.color = stroke.color;
    this.brushSettings.size = stroke.size;
    this.brushSettings.opacity = stroke.opacity ?? 1;
    this.currentTool = stroke.tool as DrawingTool;
    this.applyBrushSettings();

    if (stroke.compositeOp) {
      this.ctx.globalCompositeOperation = stroke.compositeOp as GlobalCompositeOperation;
    }

    // Render based on tool type
    if (stroke.tool === 'brush' || stroke.tool === 'eraser') {
      // Draw path through all points
      if (stroke.points.length > 0) {
        this.ctx.beginPath();
        const [startX, startY] = stroke.points[0];
        this.ctx.moveTo(startX, startY);

        for (let i = 1; i < stroke.points.length; i++) {
          const [x, y] = stroke.points[i];
          this.ctx.lineTo(x, y);
        }
        this.ctx.stroke();
      }
    } else if (stroke.points.length >= 2) {
      // Shape tools: use first and last point
      const [startX, startY] = stroke.points[0];
      const [endX, endY] = stroke.points[stroke.points.length - 1];

      this.ctx.beginPath();
      this.drawShapePath(this.ctx, startX, startY, endX, endY);
      this.ctx.stroke();
    }

    // Restore settings
    this.brushSettings.color = savedColor;
    this.brushSettings.size = savedSize;
    this.brushSettings.opacity = savedOpacity;
    this.currentTool = savedTool;
    this.applyBrushSettings();
  }

  protected async renderWidget(): Promise<void> {
    this.shadowRoot.innerHTML = `
      <style>
        :host {
          display: flex;
          flex-direction: column;
          height: 100%;
          background: var(--widget-background, #0a0a0f);
        }

        .toolbar {
          display: flex;
          gap: 8px;
          padding: 12px;
          background: var(--toolbar-background, #1a1a2e);
          border-bottom: 1px solid var(--border-color, #2d2d44);
          flex-wrap: wrap;
          align-items: center;
        }

        .tool-group {
          display: flex;
          gap: 4px;
          padding: 4px;
          background: var(--tool-group-bg, #0d0d15);
          border-radius: 6px;
        }

        .tool-btn {
          width: 36px;
          height: 36px;
          border: none;
          border-radius: 4px;
          background: transparent;
          color: var(--text-color, #e0e0e0);
          cursor: pointer;
          display: flex;
          align-items: center;
          justify-content: center;
          font-size: 16px;
          transition: all 0.2s;
        }

        .tool-btn:hover {
          background: var(--hover-bg, #2d2d44);
        }

        .tool-btn.active {
          background: var(--accent-color, #00d4ff);
          color: #000;
        }

        .color-picker-wrapper {
          display: flex;
          align-items: center;
          gap: 8px;
        }

        .color-picker {
          width: 36px;
          height: 36px;
          border: 2px solid var(--border-color, #2d2d44);
          border-radius: 4px;
          cursor: pointer;
          padding: 0;
        }

        .size-slider {
          width: 80px;
          accent-color: var(--accent-color, #00d4ff);
        }

        .size-label {
          font-size: 12px;
          color: var(--text-muted, #888);
          min-width: 30px;
        }

        .action-btn {
          padding: 8px 16px;
          border: none;
          border-radius: 4px;
          background: var(--button-bg, #2d2d44);
          color: var(--text-color, #e0e0e0);
          cursor: pointer;
          font-size: 13px;
          transition: all 0.2s;
        }

        .action-btn:hover {
          background: var(--button-hover, #3d3d54);
        }

        .action-btn.primary {
          background: var(--accent-color, #00d4ff);
          color: #000;
        }

        .action-btn.danger {
          background: #ff4444;
          color: #fff;
        }

        .canvas-container {
          flex: 1;
          position: relative;
          overflow: hidden;
        }

        .drawing-canvas {
          position: absolute;
          top: 0;
          left: 0;
          cursor: crosshair;
          background: #111;
        }

        .preview-canvas {
          position: absolute;
          top: 0;
          left: 0;
          pointer-events: none;
        }

        .status-bar {
          display: flex;
          justify-content: space-between;
          padding: 8px 12px;
          background: var(--toolbar-background, #1a1a2e);
          border-top: 1px solid var(--border-color, #2d2d44);
          font-size: 12px;
          color: var(--text-muted, #888);
        }

        .ai-status {
          display: flex;
          align-items: center;
          gap: 6px;
        }

        .ai-indicator {
          width: 8px;
          height: 8px;
          border-radius: 50%;
          background: var(--accent-color, #00d4ff);
          animation: pulse 2s infinite;
        }

        @keyframes pulse {
          0%, 100% { opacity: 0.5; }
          50% { opacity: 1; }
        }
      </style>

      <div class="toolbar">
        <div class="tool-group">
          <button class="tool-btn active" data-tool="brush" title="Brush (B)">🖌️</button>
          <button class="tool-btn" data-tool="eraser" title="Eraser (E)">🧽</button>
          <button class="tool-btn" data-tool="line" title="Line (L)">📏</button>
          <button class="tool-btn" data-tool="rectangle" title="Rectangle (R)">⬜</button>
          <button class="tool-btn" data-tool="circle" title="Circle (C)">⭕</button>
          <button class="tool-btn" data-tool="arrow" title="Arrow (A)">➡️</button>
        </div>

        <div class="color-picker-wrapper">
          <input type="color" class="color-picker" value="#00d4ff" title="Brush Color">
          <input type="range" class="size-slider" min="1" max="50" value="4" title="Brush Size">
          <span class="size-label">4px</span>
        </div>

        <div class="tool-group">
          <button class="action-btn" id="undo-btn" title="Undo (Ctrl+Z)">↶ Undo</button>
          <button class="action-btn" id="redo-btn" title="Redo (Ctrl+Y)">↷ Redo</button>
        </div>

        <button class="action-btn danger" id="clear-btn">🗑️ Clear</button>
        <button class="action-btn primary" id="capture-btn">📷 Capture for AI</button>
      </div>

      <div class="canvas-container">
        <canvas class="drawing-canvas"></canvas>
        <canvas class="preview-canvas"></canvas>
      </div>

      <div class="status-bar">
        <span class="tool-status">Tool: Brush | Size: 4px</span>
        <span class="ai-status">
          <span class="ai-indicator"></span>
          Vision AIs can see this canvas
        </span>
      </div>
    `;

    this.setupCanvas();
    this.setupEventListeners();
  }

  private setupCanvas(): void {
    this.canvas = this.shadowRoot.querySelector('.drawing-canvas');
    this.previewCanvas = this.shadowRoot.querySelector('.preview-canvas');

    if (!this.canvas || !this.previewCanvas) return;

    this.ctx = this.canvas.getContext('2d');

    // Size canvases to container
    const container = this.shadowRoot.querySelector('.canvas-container') as HTMLElement;
    if (container) {
      const resize = () => {
        const rect = container.getBoundingClientRect();
        this.canvas!.width = rect.width;
        this.canvas!.height = rect.height;
        this.previewCanvas!.width = rect.width;
        this.previewCanvas!.height = rect.height;

        // Restore canvas content after resize
        if (this.history.length > 0 && this.historyIndex >= 0) {
          const lastAction = this.history[this.historyIndex];
          if (lastAction.data) {
            this.ctx?.putImageData(lastAction.data, 0, 0);
          }
        }
      };

      resize();
      new ResizeObserver(resize).observe(container);
    }

    // Set initial canvas state
    if (this.ctx) {
      this.ctx.lineCap = 'round';
      this.ctx.lineJoin = 'round';
      this.applyBrushSettings();

      // Load existing strokes now that ctx is ready
      if (this.activityId) {
        verbose() && console.log(`🎨 DrawingCanvas: Loading strokes for canvas ${this.activityId}`);
        this.loadStrokes();
      } else {
        verbose() && console.log('🎨 DrawingCanvas: No activityId - strokes will not persist');
      }
    }
  }

  private setupEventListeners(): void {
    // Tool buttons
    this.shadowRoot.querySelectorAll('.tool-btn').forEach(btn => {
      btn.addEventListener('click', (e) => {
        const tool = (e.currentTarget as HTMLElement).dataset.tool as DrawingTool;
        this.setTool(tool);
      });
    });

    // Color picker
    const colorPicker = this.shadowRoot.querySelector('.color-picker') as HTMLInputElement;
    colorPicker?.addEventListener('input', (e) => {
      this.brushSettings.color = (e.target as HTMLInputElement).value;
      this.applyBrushSettings();
    });

    // Size slider
    const sizeSlider = this.shadowRoot.querySelector('.size-slider') as HTMLInputElement;
    const sizeLabel = this.shadowRoot.querySelector('.size-label');
    sizeSlider?.addEventListener('input', (e) => {
      const size = parseInt((e.target as HTMLInputElement).value);
      this.brushSettings.size = size;
      if (sizeLabel) sizeLabel.textContent = `${size}px`;
      this.applyBrushSettings();
      this.updateStatusBar();
    });

    // Action buttons
    this.shadowRoot.querySelector('#undo-btn')?.addEventListener('click', () => this.undo());
    this.shadowRoot.querySelector('#redo-btn')?.addEventListener('click', () => this.redo());
    this.shadowRoot.querySelector('#clear-btn')?.addEventListener('click', () => this.clearCanvas());
    this.shadowRoot.querySelector('#capture-btn')?.addEventListener('click', () => this.captureForAI());

    // Canvas drawing events
    if (this.canvas) {
      this.canvas.addEventListener('mousedown', (e) => this.startDrawing(e));
      this.canvas.addEventListener('mousemove', (e) => this.draw(e));
      this.canvas.addEventListener('mouseup', () => this.stopDrawing());
      this.canvas.addEventListener('mouseout', () => this.stopDrawing());

      // Touch support
      this.canvas.addEventListener('touchstart', (e) => {
        e.preventDefault();
        const touch = e.touches[0];
        this.startDrawing(touch as any);
      });
      this.canvas.addEventListener('touchmove', (e) => {
        e.preventDefault();
        const touch = e.touches[0];
        this.draw(touch as any);
      });
      this.canvas.addEventListener('touchend', () => this.stopDrawing());
    }

    // Keyboard shortcuts
    this.shadowRoot.addEventListener('keydown', (event) => {
      const e = event as KeyboardEvent;
      if (e.ctrlKey || e.metaKey) {
        if (e.key === 'z') { e.preventDefault(); this.undo(); }
        if (e.key === 'y') { e.preventDefault(); this.redo(); }
      }
      if (e.key === 'b') this.setTool('brush');
      if (e.key === 'e') this.setTool('eraser');
      if (e.key === 'l') this.setTool('line');
      if (e.key === 'r') this.setTool('rectangle');
      if (e.key === 'c') this.setTool('circle');
      if (e.key === 'a') this.setTool('arrow');
    });
  }

  private setTool(tool: DrawingTool): void {
    this.currentTool = tool;

    // Update UI
    this.shadowRoot.querySelectorAll('.tool-btn').forEach(btn => {
      btn.classList.toggle('active', (btn as HTMLElement).dataset.tool === tool);
    });

    this.updateStatusBar();
  }

  private applyBrushSettings(): void {
    if (!this.ctx) return;

    if (this.currentTool === 'eraser') {
      this.ctx.globalCompositeOperation = 'destination-out';
      this.ctx.strokeStyle = 'rgba(0,0,0,1)';
    } else {
      this.ctx.globalCompositeOperation = 'source-over';
      this.ctx.strokeStyle = this.brushSettings.color;
    }
    this.ctx.lineWidth = this.brushSettings.size;
    this.ctx.globalAlpha = this.brushSettings.opacity;
  }

  private updateStatusBar(): void {
    const statusEl = this.shadowRoot.querySelector('.tool-status');
    if (statusEl) {
      const toolName = this.currentTool.charAt(0).toUpperCase() + this.currentTool.slice(1);
      statusEl.textContent = `Tool: ${toolName} | Size: ${this.brushSettings.size}px`;
    }
  }

  private getCanvasCoords(e: MouseEvent | Touch): { x: number; y: number } {
    if (!this.canvas) return { x: 0, y: 0 };
    const rect = this.canvas.getBoundingClientRect();
    return {
      x: (e as MouseEvent).clientX - rect.left,
      y: (e as MouseEvent).clientY - rect.top
    };
  }

  private startDrawing(e: MouseEvent | Touch): void {
    this.isDrawing = true;
    const coords = this.getCanvasCoords(e);
    this.lastX = coords.x;
    this.lastY = coords.y;
    this.shapeStartX = coords.x;
    this.shapeStartY = coords.y;

    // Start collecting points for this stroke
    this.currentStrokePoints = [[coords.x, coords.y]];

    if (this.currentTool === 'brush' || this.currentTool === 'eraser') {
      this.ctx?.beginPath();
      this.ctx?.moveTo(coords.x, coords.y);
    }
  }

  private draw(e: MouseEvent | Touch): void {
    if (!this.isDrawing || !this.ctx) return;

    const coords = this.getCanvasCoords(e);

    // Collect point for database persistence
    this.currentStrokePoints.push([coords.x, coords.y]);

    if (this.currentTool === 'brush' || this.currentTool === 'eraser') {
      this.applyBrushSettings();
      this.ctx.lineTo(coords.x, coords.y);
      this.ctx.stroke();
    } else {
      // Shape preview on preview canvas
      this.drawShapePreview(coords.x, coords.y);
    }

    this.lastX = coords.x;
    this.lastY = coords.y;
  }

  private drawShapePreview(x: number, y: number): void {
    if (!this.previewCanvas) return;
    const previewCtx = this.previewCanvas.getContext('2d');
    if (!previewCtx) return;

    // Clear preview
    previewCtx.clearRect(0, 0, this.previewCanvas.width, this.previewCanvas.height);

    // Draw shape preview
    previewCtx.strokeStyle = this.brushSettings.color;
    previewCtx.lineWidth = this.brushSettings.size;
    previewCtx.setLineDash([5, 5]);
    previewCtx.beginPath();

    this.drawShapePath(previewCtx, this.shapeStartX, this.shapeStartY, x, y);
    previewCtx.stroke();
    previewCtx.setLineDash([]);
  }

  private drawShapePath(
    ctx: CanvasRenderingContext2D,
    startX: number, startY: number,
    endX: number, endY: number
  ): void {
    const width = endX - startX;
    const height = endY - startY;

    switch (this.currentTool) {
      case 'line':
        ctx.moveTo(startX, startY);
        ctx.lineTo(endX, endY);
        break;

      case 'rectangle':
        ctx.rect(startX, startY, width, height);
        break;

      case 'circle':
        const radius = Math.sqrt(width * width + height * height);
        ctx.arc(startX, startY, radius, 0, Math.PI * 2);
        break;

      case 'arrow':
        // Draw line
        ctx.moveTo(startX, startY);
        ctx.lineTo(endX, endY);

        // Draw arrowhead
        const angle = Math.atan2(height, width);
        const headLength = 15;
        ctx.moveTo(endX, endY);
        ctx.lineTo(
          endX - headLength * Math.cos(angle - Math.PI / 6),
          endY - headLength * Math.sin(angle - Math.PI / 6)
        );
        ctx.moveTo(endX, endY);
        ctx.lineTo(
          endX - headLength * Math.cos(angle + Math.PI / 6),
          endY - headLength * Math.sin(angle + Math.PI / 6)
        );
        break;
    }
  }

  private stopDrawing(): void {
    if (!this.isDrawing) return;
    this.isDrawing = false;

    // For shape tools, commit the shape to main canvas
    if (this.currentTool !== 'brush' && this.currentTool !== 'eraser' && this.ctx) {
      this.applyBrushSettings();
      this.ctx.beginPath();
      this.drawShapePath(this.ctx, this.shapeStartX, this.shapeStartY, this.lastX, this.lastY);
      this.ctx.stroke();

      // Clear preview
      const previewCtx = this.previewCanvas?.getContext('2d');
      if (previewCtx && this.previewCanvas) {
        previewCtx.clearRect(0, 0, this.previewCanvas.width, this.previewCanvas.height);
      }
    }

    // Save to local history
    this.saveToHistory();

    // Save to database for persistence and real-time sync
    this.saveStrokeToDatabase();
  }

  /**
   * Save the current stroke to the database
   */
  private async saveStrokeToDatabase(): Promise<void> {
    // Only save if we have an activity ID and at least one point
    if (!this.activityId || this.currentStrokePoints.length === 0) {
      // Still emit local event for AI awareness even without persistence
      Events.emit(DRAWING_CANVAS_EVENTS.STROKE_ADDED, {
        tool: this.currentTool,
        color: this.brushSettings.color,
        timestamp: Date.now()
      });
      return;
    }

    try {
      // Map DrawingTool to CanvasTool (they should be compatible except 'text')
      const tool = this.currentTool === 'text' ? 'brush' : this.currentTool;

      const result = await Commands.execute<CanvasStrokeAddParams, CanvasStrokeAddResult>(
        'canvas/stroke/add',
        {
          canvasId: this.activityId,
          tool: tool as CanvasTool,
          points: this.currentStrokePoints,
          color: this.brushSettings.color,
          size: this.brushSettings.size,
          opacity: this.brushSettings.opacity,
          compositeOp: this.currentTool === 'eraser' ? 'destination-out' : undefined
        }
      );

      if (result.success && result.strokeId) {
        // Mark as loaded so we don't re-render when event comes back
        this.loadedStrokeIds.add(result.strokeId);
        verbose() && console.log(`🎨 DrawingCanvas: Saved stroke ${result.strokeId}`);

        // Update Positron context so AIs know canvas changed
        this.emitPositronContext();
      } else {
        console.warn('🎨 DrawingCanvas: Failed to save stroke:', result.error);
      }
    } catch (err) {
      console.error('🎨 DrawingCanvas: Error saving stroke:', err);
    }

    // Clear collected points
    this.currentStrokePoints = [];
  }

  private saveToHistory(): void {
    if (!this.ctx || !this.canvas) return;

    // Remove any redo states
    this.history = this.history.slice(0, this.historyIndex + 1);

    // Save current state
    const imageData = this.ctx.getImageData(0, 0, this.canvas.width, this.canvas.height);
    this.history.push({
      type: 'stroke',
      data: imageData,
      timestamp: Date.now()
    });
    this.historyIndex++;

    // Limit history size
    if (this.history.length > 50) {
      this.history.shift();
      this.historyIndex--;
    }
  }

  private undo(): void {
    if (this.historyIndex <= 0 || !this.ctx || !this.canvas) return;

    this.historyIndex--;
    if (this.historyIndex >= 0 && this.history[this.historyIndex].data) {
      this.ctx.putImageData(this.history[this.historyIndex].data!, 0, 0);
    } else {
      this.ctx.clearRect(0, 0, this.canvas.width, this.canvas.height);
    }
  }

  private redo(): void {
    if (this.historyIndex >= this.history.length - 1 || !this.ctx) return;

    this.historyIndex++;
    const action = this.history[this.historyIndex];
    if (action.data) {
      this.ctx.putImageData(action.data, 0, 0);
    }
  }

  private clearCanvas(): void {
    if (!this.ctx || !this.canvas) return;

    this.ctx.clearRect(0, 0, this.canvas.width, this.canvas.height);
    this.saveToHistory();

    Events.emit(DRAWING_CANVAS_EVENTS.CANVAS_CLEARED, {
      timestamp: Date.now()
    });
  }

  /**
   * Capture canvas as base64 image for sending to vision AI
   */
  async captureForAI(): Promise<string | null> {
    if (!this.canvas) return null;

    const dataUrl = this.canvas.toDataURL('image/png');
    const base64 = dataUrl.split(',')[1];

    Events.emit(DRAWING_CANVAS_EVENTS.CANVAS_CAPTURED, {
      base64,
      mimeType: 'image/png',
      width: this.canvas.width,
      height: this.canvas.height,
      timestamp: Date.now()
    });

    verbose() && console.log('📷 DrawingCanvas: Captured for AI vision');
    return base64;
  }

  /**
   * Get canvas as base64 for external use
   */
  getCanvasBase64(): string | null {
    if (!this.canvas) return null;
    return this.canvas.toDataURL('image/png').split(',')[1];
  }

  /**
   * Handle AI draw request (from positron/cursor or direct command)
   */
  private handleAIDrawRequest(data: {
    tool?: DrawingTool;
    startX: number;
    startY: number;
    endX?: number;
    endY?: number;
    color?: string;
    size?: number;
    personaName?: string;
  }): void {
    if (!this.ctx) return;

    const { tool = 'brush', startX, startY, endX, endY, color, size, personaName } = data;

    // Apply settings
    if (color) this.brushSettings.color = color;
    if (size) this.brushSettings.size = size;
    this.applyBrushSettings();

    // Draw
    this.ctx.beginPath();
    if (tool === 'brush' || !endX || !endY) {
      // Single point or brush stroke
      this.ctx.arc(startX, startY, this.brushSettings.size / 2, 0, Math.PI * 2);
      this.ctx.fill();
    } else {
      // Shape
      this.currentTool = tool;
      this.drawShapePath(this.ctx, startX, startY, endX, endY);
      this.ctx.stroke();
    }

    this.saveToHistory();
    verbose() && console.log(`🎨 DrawingCanvas: ${personaName || 'AI'} drew ${tool} at (${startX}, ${startY})`);
  }

  protected async onWidgetCleanup(): Promise<void> {
    // Unsubscribe from stroke events
    if (this.strokeEventUnsubscribe) {
      this.strokeEventUnsubscribe();
      this.strokeEventUnsubscribe = null;
    }

    // Clear loaded stroke IDs
    this.loadedStrokeIds.clear();

    verbose() && console.log('🎨 DrawingCanvas: Cleaned up collaborative canvas');
  }
}
