/**
 * CanvasStrokeEntity - Collaborative canvas stroke data
 *
 * Extends CollaborativeOperationEntity for the drawing canvas domain.
 * Uses activityId from base class as the canvas identifier.
 */

import {
  CollaborativeOperationEntity,
  type OperationMeta
} from './CollaborativeOperationEntity';
import { COLLECTIONS } from '../../shared/Constants';
import { JsonField, NumberField, TextField } from '../decorators/FieldDecorators';

// ============================================================================
// TYPES
// ============================================================================

/**
 * Drawing tool types
 */
export type CanvasTool = 'brush' | 'eraser' | 'line' | 'rectangle' | 'circle' | 'arrow';

/**
 * Point format: [x, y] or [x, y, pressure] for pressure-sensitive input
 */
export type StrokePoint = [number, number] | [number, number, number];

/**
 * Bounding box for hit testing and viewport culling
 */
export interface StrokeBounds {
  minX: number;
  minY: number;
  maxX: number;
  maxY: number;
}

/**
 * Stroke metadata stored in base class meta field
 */
export interface CanvasStrokeMeta extends OperationMeta {
  /** Number of points (for size estimation without loading content) */
  pointCount?: number;
}

// ============================================================================
// ENTITY
// ============================================================================

/**
 * Canvas stroke operation entity
 *
 * Base class provides: activityId (canvas ID), creatorId, creatorName, timestamp, opType
 * This class adds: tool, color, size, points, bounds, opacity, compositeOp
 */
export class CanvasStrokeEntity extends CollaborativeOperationEntity<CanvasStrokeMeta> {
  static readonly collection = COLLECTIONS.CANVAS_STROKES;

  // ============================================================================
  // STROKE-SPECIFIC FIELDS
  // ============================================================================

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
   * Stroke points array: [[x1,y1], [x2,y2], ...] or [[x1,y1,p1], ...]
   */
  @JsonField()
  points!: StrokePoint[];

  /**
   * Bounding box for hit testing and viewport culling
   */
  @JsonField({ nullable: true })
  bounds?: StrokeBounds;

  /**
   * Stroke opacity (0-1)
   */
  @NumberField({ nullable: true })
  opacity?: number;

  /**
   * Composite operation (for blend modes)
   */
  @TextField({ nullable: true })
  compositeOp?: string;

  // ============================================================================
  // CONSTRUCTOR
  // ============================================================================

  constructor() {
    super();
    this.timestamp = new Date();
    this.opType = 'stroke';
    this.tool = 'brush';
    this.color = '#000000';
    this.size = 5;
    this.points = [];
    this.opacity = 1;
    this.meta = {};
  }

  // ============================================================================
  // BASE ENTITY IMPLEMENTATION
  // ============================================================================

  get collection(): string {
    return CanvasStrokeEntity.collection;
  }

  /**
   * Validate stroke data
   */
  validate(): { success: boolean; error?: string } {
    const baseResult = super.validate();
    if (!baseResult.success) return baseResult;

    const validTools: CanvasTool[] = ['brush', 'eraser', 'line', 'rectangle', 'circle', 'arrow'];
    if (!validTools.includes(this.tool)) {
      return {
        success: false,
        error: `Canvas stroke tool must be one of: ${validTools.join(', ')}`
      };
    }

    if (!Array.isArray(this.points)) {
      return { success: false, error: 'Canvas stroke points must be an array' };
    }
    if (this.points.length === 0) {
      return { success: false, error: 'Canvas stroke must have at least one point' };
    }

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
   * Pagination config - oldest first for proper replay order
   */
  static getPaginationConfig(): {
    defaultSortField: string;
    defaultSortDirection: 'asc' | 'desc';
    defaultPageSize: number;
    cursorField: string;
  } {
    return {
      defaultSortField: 'timestamp',
      defaultSortDirection: 'asc',
      defaultPageSize: 100,
      cursorField: 'timestamp'
    };
  }
}
