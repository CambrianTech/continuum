import { Commands } from '../../../../system/core/shared/Commands';
import type { CommandParams, CommandResult, CommandInput} from '../../../../system/core/types/JTAGTypes';

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
