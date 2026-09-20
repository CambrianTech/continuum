import { CommandParams, CommandResult, createPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { JTAGContext, CommandInput} from '@system/core/types/JTAGTypes';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import { Commands } from '../../../../system/core/shared/Commands';

/** Wait for a DOM element to appear, matching a CSS selector. */
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
    selector: string;  // Required, not optional!
    timeout?: number;
    visible?: boolean;
    interval?: number;
  }
): WaitForElementParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
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
    selector: string;  // Required, not optional!
    found?: boolean;
    visible?: boolean;
    timeout?: number;
    waitTime?: number;
    error?: JTAGError;
  }
): WaitForElementResult => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  found: data.found ?? false,
  visible: data.visible ?? false,
  timeout: data.timeout ?? 30000,
  waitTime: data.waitTime ?? 0,
  timestamp: new Date().toISOString(),
  ...data
});
/**
 * WaitForElement — Type-safe command executor
 *
 * Usage:
 *   import { WaitForElement } from '...shared/WaitForElementTypes';
 *   const result = await WaitForElement.execute({ ... });
 */
export const WaitForElement = {
  execute(params: CommandInput<WaitForElementParams>): Promise<WaitForElementResult> {
    return Commands.execute<WaitForElementParams, WaitForElementResult>('interface/wait-for-element', params as Partial<WaitForElementParams>);
  },
  commandName: 'interface/wait-for-element' as const,
} as const;
