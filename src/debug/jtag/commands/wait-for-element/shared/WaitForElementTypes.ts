import { CommandParams, CommandResult, createPayload } from '@shared/JTAGTypes';
import type { JTAGContext } from '@shared/JTAGTypes';
import type { JTAGError } from '@shared/ErrorTypes';
import { UUID } from '@shared/CrossPlatformUUID';

export interface WaitForElementParams extends CommandParams {
  readonly selector: string;
  readonly timeout?: number;
  readonly visible?: boolean;
  readonly interval?: number;
}

export const createWaitForElementParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    selector?: string;
    timeout?: number;
    visible?: boolean;
    interval?: number;
  }
): WaitForElementParams => createPayload(context, sessionId, {
  selector: data.selector ?? 'body',
  timeout: data.timeout ?? 30000,
  visible: data.visible ?? true,
  interval: data.interval ?? 100,
  ...data
});

export interface WaitForElementResult extends CommandResult {
  readonly success: boolean;
  readonly selector: string;
  readonly found: boolean;
  readonly visible: boolean;
  readonly timeout: number;
  readonly waitTime: number;
  readonly error?: JTAGError;
  readonly timestamp: string;
}

export const createWaitForElementResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    selector?: string;
    found?: boolean;
    visible?: boolean;
    timeout?: number;
    waitTime?: number;
    error?: JTAGError;
  }
): WaitForElementResult => createPayload(context, sessionId, {
  selector: data.selector ?? '',
  found: data.found ?? false,
  visible: data.visible ?? false,
  timeout: data.timeout ?? 30000,
  waitTime: data.waitTime ?? 0,
  timestamp: new Date().toISOString(),
  ...data
});