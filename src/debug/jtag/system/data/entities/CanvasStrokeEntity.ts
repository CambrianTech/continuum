/**
 * CanvasStrokeEntity - Collaborative canvas stroke data
 *
 * Stores individual strokes for collaborative canvas activities.
 * Each stroke is atomic - created by one user (human or AI),
 * and synchronized to all participants in real-time via Events.
 *
 * Design:
 * - canvasId links to ActivityEntity (canvas is an activity type)
 * - Points stored as flat array for efficient storage/transfer
 * - Full undo/redo support via stroke ordering by timestamp
 * - AI can draw strokes same as humans (tool parity)
 */

import { BaseEntity } from './BaseEntity';
import { COLLECTIONS } from '../../shared/Constants';
import type { UUID } from '../../core/types/CrossPlatformUUID';
import { TextField, NumberField, JsonField, DateField } from '../decorators/FieldDecorators';

/**
 * Drawing tool types
 */
export type CanvasTool = 'brush' | 'eraser' | 'line' | 'rectangle' | 'circle' | 'arrow';

/**
 * Point format: [x, y] or [x, y, pressure] for pressure-sensitive input
 */
export type StrokePoint = [number, number] | [number, number, number];

export class CanvasStrokeEntity extends BaseEntity {
  static readonly collection = COLLECTIONS.CANVAS_STROKES;

  // ============================================================================
  // SUMMARY FIELDS (included in data/list by default)
  // ============================================================================

  /**
   * Activity ID (canvas instance) this stroke belongs to
   */
  @TextField({ index: true, summary: true })
  canvasId!: UUID;

  /**
   * Drawing tool used
   */
  @TextField({ index: true, summary: true })
  tool!: CanvasTool;

  /**
   * Stroke color (CSS color string)
   */
  @TextField({ summary: true })
  color!: string;

  /**
   * Brush/tool size in pixels
   */
  @NumberField({ summary: true })
  size!: number;

  /**
   * User who created this stroke (human or AI)
   */
  @TextField({ index: true, summary: true })
  creatorId!: UUID;

  /**
   * Display name of creator (for real-time collaboration display)
   */
  @TextField({ summary: true })
  creatorName!: string;

  /**
   * Timestamp when stroke was created (for ordering)
   */
  @DateField({ index: true, summary: true })
  timestamp!: Date;

  // ============================================================================
  // DETAIL FIELDS (NOT in summary - require explicit data/read)
  // ============================================================================

  /**
   * Stroke points array: [[x1,y1], [x2,y2], ...] or [[x1,y1,p1], ...]
   * Stored as JSON for flexibility with pressure data
   */
  @JsonField()
  points!: StrokePoint[];

  /**
   * Optional: Bounding box for efficient hit testing and viewport culling
   * Format: { minX, minY, maxX, maxY }
   */
  @JsonField({ nullable: true })
  bounds?: {
    minX: number;
    minY: number;
    maxX: number;
    maxY: number;
  };

  /**
   * Optional: Stroke opacity (0-1)
   */
  @NumberField({ nullable: true })
  opacity?: number;

  /**
   * Optional: Composite operation (for blend modes)
   */
  @TextField({ nullable: true })
  compositeOp?: string;

  // ============================================================================
  // CONSTRUCTOR
  // ============================================================================

  constructor() {
    super();
    this.timestamp = new Date();
    this.points = [];
    this.size = 5;
    this.color = '#000000';
    this.tool = 'brush';
    this.opacity = 1;
  }

  // ============================================================================
  // BASE ENTITY IMPLEMENTATION
  // ============================================================================

  /**
   * Implement BaseEntity abstract method
   */
  get collection(): string {
    return CanvasStrokeEntity.collection;
  }

  /**
   * Implement BaseEntity abstract method - validate stroke data
   */
  validate(): { success: boolean; error?: string } {
    // Required fields
    if (!this.canvasId) {
      return { success: false, error: 'Canvas stroke canvasId is required' };
    }
    if (!this.creatorId) {
      return { success: false, error: 'Canvas stroke creatorId is required' };
    }
    if (!this.creatorName?.trim()) {
      return { success: false, error: 'Canvas stroke creatorName is required' };
    }

    // Validate tool type
    const validTools: CanvasTool[] = ['brush', 'eraser', 'line', 'rectangle', 'circle', 'arrow'];
    if (!validTools.includes(this.tool)) {
      return {
        success: false,
        error: `Canvas stroke tool must be one of: ${validTools.join(', ')}`
      };
    }

    // Validate points
    if (!Array.isArray(this.points)) {
      return { success: false, error: 'Canvas stroke points must be an array' };
    }
    if (this.points.length === 0) {
      return { success: false, error: 'Canvas stroke must have at least one point' };
    }

    // Validate each point
    for (let i = 0; i < this.points.length; i++) {
      const point = this.points[i];
      if (!Array.isArray(point) || point.length < 2 || point.length > 3) {
        return {
          success: false,
          error: `Canvas stroke point ${i} must be [x, y] or [x, y, pressure]`
        };
      }
      if (typeof point[0] !== 'number' || typeof point[1] !== 'number') {
        return {
          success: false,
          error: `Canvas stroke point ${i} coordinates must be numbers`
        };
      }
    }

    // Validate size
    if (typeof this.size !== 'number' || this.size < 1 || this.size > 200) {
      return { success: false, error: 'Canvas stroke size must be between 1 and 200' };
    }

    return { success: true };
  }

  // ============================================================================
  // UTILITY METHODS
  // ============================================================================

  /**
   * Calculate bounding box from points
   */
  calculateBounds(): void {
    if (this.points.length === 0) {
      this.bounds = undefined;
      return;
    }

    let minX = Infinity, minY = Infinity;
    let maxX = -Infinity, maxY = -Infinity;

    for (const point of this.points) {
      const [x, y] = point;
      minX = Math.min(minX, x);
      minY = Math.min(minY, y);
      maxX = Math.max(maxX, x);
      maxY = Math.max(maxY, y);
    }

    // Expand by half stroke size for accurate hit testing
    const halfSize = this.size / 2;
    this.bounds = {
      minX: minX - halfSize,
      minY: minY - halfSize,
      maxX: maxX + halfSize,
      maxY: maxY + halfSize
    };
  }

  /**
   * Check if stroke intersects with a rectangle (for viewport culling)
   */
  intersectsRect(x: number, y: number, width: number, height: number): boolean {
    if (!this.bounds) {
      this.calculateBounds();
    }
    if (!this.bounds) return false;

    return !(
      this.bounds.maxX < x ||
      this.bounds.minX > x + width ||
      this.bounds.maxY < y ||
      this.bounds.minY > y + height
    );
  }

  /**
   * Pagination config - show strokes in creation order (oldest first for replay)
   */
  static getPaginationConfig(): {
    defaultSortField: string;
    defaultSortDirection: 'asc' | 'desc';
    defaultPageSize: number;
    cursorField: string;
  } {
    return {
      defaultSortField: 'timestamp',
      defaultSortDirection: 'asc',  // Oldest first for proper replay order
      defaultPageSize: 100,
      cursorField: 'timestamp'
    };
  }
}
