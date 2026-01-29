import { CommandParams, CommandResult, createPayload } from '@system/core/types/JTAGTypes';
import type { JTAGContext, CommandInput} from '@system/core/types/JTAGTypes';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import { Commands } from '../../../../system/core/shared/Commands';

export interface GetTextParams extends CommandParams {
  readonly selector: string;
  readonly trim?: boolean;
  readonly innerText?: boolean; // vs textContent
}

export const createGetTextParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    selector: string;  // Required, not optional!
    trim?: boolean;
    innerText?: boolean;
  }
): GetTextParams => createPayload(context, sessionId, {
  trim: data.trim ?? true,
  innerText: data.innerText ?? true,
  ...data  // selector is required, so it's in data
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
/**
 * GetText — Type-safe command executor
 *
 * Usage:
 *   import { GetText } from '...shared/GetTextTypes';
 *   const result = await GetText.execute({ ... });
 */
export const GetText = {
  execute(params: CommandInput<GetTextParams>): Promise<GetTextResult> {
    return Commands.execute<GetTextParams, GetTextResult>('interface/get-text', params as Partial<GetTextParams>);
  },
  commandName: 'interface/get-text' as const,
} as const;
