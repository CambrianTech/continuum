import { CommandParams, CommandResult, createPayload } from '@system/core/types/JTAGTypes';
import type { JTAGContext, CommandInput} from '@system/core/types/JTAGTypes';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import { Commands } from '../../../../system/core/shared/Commands';

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
    selector: string;  // Required, not optional!
    text?: string;
    clearFirst?: boolean;
    delay?: number;
  }
): TypeParams => createPayload(context, sessionId, {
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
    selector: string;  // Required, not optional!
    typed?: boolean;
    text?: string;
    error?: JTAGError;
  }
): TypeResult => createPayload(context, sessionId, {
  typed: data.typed ?? false,
  text: data.text ?? '',
  timestamp: new Date().toISOString(),
  ...data
});
/**
 * Type — Type-safe command executor
 *
 * Usage:
 *   import { Type } from '...shared/TypeTypes';
 *   const result = await Type.execute({ ... });
 */
export const Type = {
  execute(params: CommandInput<TypeParams>): Promise<TypeResult> {
    return Commands.execute<TypeParams, TypeResult>('interface/type', params as Partial<TypeParams>);
  },
  commandName: 'interface/type' as const,
} as const;
