import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';

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
