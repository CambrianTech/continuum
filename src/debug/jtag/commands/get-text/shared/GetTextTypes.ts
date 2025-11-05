import { CommandParams, CommandResult, createPayload } from '../../../system/core/types/JTAGTypes';
import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import type { JTAGError } from '../../../system/core/types/ErrorTypes';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';

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
  readonly shadowDOMData?: any; // Shadow DOM query results
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
    shadowDOMData?: any;
  }
): GetTextResult => createPayload(context, sessionId, {
  selector: data.selector ?? '',
  text: data.text ?? '',
  found: data.found ?? false,
  timestamp: new Date().toISOString(),
  ...data
});