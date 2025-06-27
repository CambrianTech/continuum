/**
 * WebSocket Server Fix Verification Test
 * Modular, dependency-injected test for CommandSystemBridge fix
 */
import { BaseCommand } from '../../core/BaseCommand.js';
import { CommandDefinition, CommandContext, CommandResult } from '../../core/BaseCommand.js';
interface WebSocketTestParams {
    timeout?: number;
    verbose?: boolean;
}
interface WebSocketTestResult extends CommandResult {
    markers: {
        bridgeInitialized: boolean;
        commandsMapPopulated: boolean;
        webSocketReady: boolean;
        crashDetected: boolean;
    };
    output?: string;
}
export declare class WebSocketServerTest extends BaseCommand {
    private static readonly VERIFICATION_MARKERS;
    static getDefinition(): CommandDefinition;
    static execute(params: WebSocketTestParams, context?: CommandContext): Promise<WebSocketTestResult>;
    private static analyzeOutput;
    private static hasAnyMarker;
    private static logResults;
    private static getFailureMessage;
}
export {};
//# sourceMappingURL=WebSocketServerTest.d.ts.map