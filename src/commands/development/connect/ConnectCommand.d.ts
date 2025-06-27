/**
 * Connect Command - Standard Connection Protocol
 * ==============================================
 * Implements the standard AI connection workflow:
 * 1. Session detection/management (existing browser or DevTools)
 * 2. Browser reload trigger
 * 3. Log connection (JTAG unit)
 * 4. Session sandbox setup
 * 5. SelfTest validation (like git hook)
 *
 * Any AI can call this via: continuum.connect() or --cmd connect
 */
import { CommandDefinition } from '../../core/BaseCommand';
export interface ConnectParams {
    mode?: 'auto' | 'existing' | 'devtools';
    reload?: boolean;
    selftest?: boolean;
    logs?: boolean;
    sandbox?: boolean;
}
export interface ConnectResult {
    success: boolean;
    message: string;
    session: {
        type: 'existing' | 'devtools';
        connected: boolean;
        tabs?: number;
    };
    steps: {
        sessionManagement: boolean;
        browserReload: boolean;
        logConnection: boolean;
        sessionSandbox: boolean;
        selftestValidation: boolean;
    };
    error?: string;
}
export declare const ConnectCommand: CommandDefinition<ConnectParams, ConnectResult>;
export default ConnectCommand;
//# sourceMappingURL=ConnectCommand.d.ts.map