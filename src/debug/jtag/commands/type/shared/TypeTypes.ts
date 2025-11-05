import { CommandParams, CommandResult, createPayload } from '../../../system/core/types/JTAGTypes';
import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import type { JTAGError } from '../../../system/core/types/ErrorTypes';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';

export interface TypeParams extends CommandParams {
  readonly selector: string;
  readonly text: string;
  readonly clearFirst?: boolean;
  readonly delay?: number;
}

export const createTypeParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    selector?: string;
    text?: string;
    clearFirst?: boolean;
    delay?: number;
  }
): TypeParams => createPayload(context, sessionId, {
  selector: data.selector ?? '',
  text: data.text ?? '',
  clearFirst: data.clearFirst ?? true,
  delay: data.delay ?? 0,
  ...data
});

export interface TypeResult extends CommandResult {
  readonly success: boolean;
  readonly selector: string;
  readonly typed: boolean;
  readonly text: string;
  readonly error?: JTAGError;
  readonly timestamp: string;
}

export const createTypeResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    selector?: string;
    typed?: boolean;
    text?: string;
    error?: JTAGError;
  }
): TypeResult => createPayload(context, sessionId, {
  selector: data.selector ?? '',
  typed: data.typed ?? false,
  text: data.text ?? '',
  timestamp: new Date().toISOString(),
  ...data
});