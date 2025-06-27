/**
 * Probe Command - TypeScript Implementation
 * Validation and feedback probes for debugging the Continuum ecosystem
 */
import { BaseCommand } from '../../core/BaseCommand';
interface ProbeParams {
    type?: 'connection' | 'feedback' | 'trace' | 'system';
    level?: 'error' | 'warn' | 'info' | 'debug';
    target?: string;
    duration?: number;
}
interface ProbeResult {
    success: boolean;
    traceId: string;
    probeType: string;
    timestamp: string;
    data?: any;
    error?: string;
}
export declare class ProbeCommand extends BaseCommand {
    static getDefinition(): {
        name: string;
        description: string;
        category: string;
        parameters: {
            type: {
                type: string;
                description: string;
                default: string;
            };
            level: {
                type: string;
                description: string;
                default: string;
            };
            target: {
                type: string;
                description: string;
                optional: boolean;
            };
            duration: {
                type: string;
                description: string;
                default: number;
            };
        };
        examples: string[];
    };
    static execute(params: ProbeParams): Promise<ProbeResult>;
    private static runConnectionProbe;
    private static runFeedbackProbe;
    private static runTraceProbe;
    private static runSystemProbe;
}
export default ProbeCommand;
//# sourceMappingURL=ProbeCommand.d.ts.map