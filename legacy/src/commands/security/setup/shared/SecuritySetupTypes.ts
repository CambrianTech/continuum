import { Commands } from '../../../../system/core/shared/Commands';
import type { CommandParams, CommandResult, CommandInput} from '../../../../system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/** Install and configure security components (network monitor, proxy) and report their current status. */
export interface SecuritySetupParams extends CommandParams {
    /** Skip interactive prompts and show status only */
    statusOnly?: boolean;

    /** Component to setup: 'monitor' | 'proxy' | 'all' */
    component?: 'monitor' | 'proxy' | 'all';
}

export interface SecuritySetupResult extends CommandResult {
    success: boolean;
    installed: {
        monitor: boolean;
        proxy: boolean;
        terminalNotifier: boolean;
    };

    status: {
        monitorRunning: boolean;
        proxyActive: boolean;
        logDirectory: string;
    };

    nextSteps: string[];
    manualCommands?: string[];
}

export interface SetupStep {
    name: string;
    description: string;
    command: string;
    requiresSudo: boolean;
    optional: boolean;
}

/**
 * SecuritySetup — Type-safe command executor
 *
 * Usage:
 *   import { SecuritySetup } from '...shared/SecuritySetupTypes';
 *   const result = await SecuritySetup.execute({ ... });
 */
export const SecuritySetup = {
  execute(params: CommandInput<SecuritySetupParams>): Promise<SecuritySetupResult> {
    return Commands.execute<SecuritySetupParams, SecuritySetupResult>('security/setup', params as Partial<SecuritySetupParams>);
  },
  commandName: 'security/setup' as const,
} as const;

/**
 * Factory function for creating SecuritySetupParams
 */
export const createSecuritySetupParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<SecuritySetupParams, 'context' | 'sessionId' | 'userId'>
): SecuritySetupParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data
});

/**
 * Factory function for creating SecuritySetupResult with defaults
 */
export const createSecuritySetupResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<SecuritySetupResult, 'context' | 'sessionId' | 'userId'>
): SecuritySetupResult => createPayload(context, sessionId, {
  ...data
});

/**
 * Smart security/setup-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createSecuritySetupResultFromParams = (
  params: SecuritySetupParams,
  differences: Omit<SecuritySetupResult, 'context' | 'sessionId' | 'userId'>
): SecuritySetupResult => transformPayload(params, differences);

