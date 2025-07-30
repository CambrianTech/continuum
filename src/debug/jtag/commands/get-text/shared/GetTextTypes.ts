import { CommandParams, CommandResult, createPayload } from '@shared/JTAGTypes';
import type { JTAGContext } from '@shared/JTAGTypes';
import type { JTAGError } from '@shared/ErrorTypes';
import { UUID } from '@shared/CrossPlatformUUID';

export interface GetTextParams extends CommandParams {
  readonly selector: string;
  readonly trim?: boolean;
  readonly innerText?: boolean; // vs textContent
}

export const createGetTextParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    selector?: string;
    trim?: boolean;
    innerText?: boolean;
  }
): GetTextParams => createPayload(context, sessionId, {
  selector: data.selector ?? 'body',
  trim: data.trim ?? true,
  innerText: data.innerText ?? true,
  ...data
});

export interface GetTextResult extends CommandResult {
  readonly success: boolean;
  readonly selector: string;
  readonly text: string;
  readonly found: boolean;
  readonly error?: JTAGError;
  readonly timestamp: string;
}

export const createGetTextResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    selector?: string;
    text?: string;
    found?: boolean;
    error?: JTAGError;
  }
): GetTextResult => createPayload(context, sessionId, {
  selector: data.selector ?? '',
  text: data.text ?? '',
  found: data.found ?? false,
  timestamp: new Date().toISOString(),
  ...data
});