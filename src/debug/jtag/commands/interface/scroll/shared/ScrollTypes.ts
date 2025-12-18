import { CommandParams, CommandResult, createPayload } from '@system/core/types/JTAGTypes';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

export interface ScrollParams extends CommandParams {
  readonly x?: number;
  readonly y?: number;
  readonly selector?: string;
  readonly behavior?: 'auto' | 'smooth' | 'instant';
}

export const createScrollParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    x?: number;
    y?: number;
    selector?: string;
    behavior?: 'auto' | 'smooth' | 'instant';
  }
): ScrollParams => createPayload(context, sessionId, {
  x: data.x ?? 0,
  y: data.y ?? 0,
  selector: data.selector,
  behavior: data.behavior ?? 'smooth',
  ...data
});

export interface ScrollResult extends CommandResult {
  readonly success: boolean;
  readonly scrollX: number;
  readonly scrollY: number;
  readonly selector?: string;
  readonly scrolled: boolean;
  readonly error?: JTAGError;
  readonly timestamp: string;
}

export const createScrollResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    scrollX?: number;
    scrollY?: number;
    selector?: string;
    scrolled?: boolean;
    error?: JTAGError;
  }
): ScrollResult => createPayload(context, sessionId, {
  scrollX: data.scrollX ?? 0,
  scrollY: data.scrollY ?? 0,
  selector: data.selector,
  scrolled: data.scrolled ?? false,
  timestamp: new Date().toISOString(),
  ...data
});