/**
 * Probe Command - TypeScript Implementation
 * Validation and feedback probes for debugging the Continuum ecosystem
 */
import { BaseCommand } from '../../core/BaseCommand';
export class ProbeCommand extends BaseCommand {
    static getDefinition() {
        return {
            name: 'probe',
            description: 'Validation and feedback probes for debugging',
            category: 'development',
            parameters: {
                type: {
                    type: 'string',
                    description: 'Type of probe: connection, feedback, trace, system',
                    default: 'feedback'
                },
                level: {
                    type: 'string',
                    description: 'Log level for feedback: error, warn, info, debug',
                    default: 'info'
                },
                target: {
                    type: 'string',
                    description: 'Target for probe (optional)',
                    optional: true
                },
                duration: {
                    type: 'number',
                    description: 'Duration for timed probes (ms)',
                    default: 1000
                }
            },
            examples: [
                'probe --type feedback --level info',
                'probe --type connection --target browser',
                'probe --type trace --duration 5000'
            ]
        };
    }
    static async execute(params) {
        const traceId = `probe-${Date.now()}-${Math.random().toString(36).substring(7)}`;
        const timestamp = new Date().toISOString();
        const probeType = params.type || 'feedback';
        try {
            console.log(`🔬 PROBE STARTING: ${traceId} (type: ${probeType})`);
            let probeData = {};
            switch (probeType) {
                case 'connection':
                    probeData = await this.runConnectionProbe(traceId, params);
                    break;
                case 'feedback':
                    probeData = await this.runFeedbackProbe(traceId, params);
                    break;
                case 'trace':
                    probeData = await this.runTraceProbe(traceId, params);
                    break;
                case 'system':
                    probeData = await this.runSystemProbe(traceId, params);
                    break;
                default:
                    throw new Error(`Unknown probe type: ${probeType}`);
            }
            console.log(`✅ PROBE COMPLETED: ${traceId}`, probeData);
            return {
                success: true,
                traceId,
                probeType,
                timestamp,
                data: probeData
            };
        }
        catch (error) {
            const errorMessage = error instanceof Error ? error.message : String(error);
            console.error(`❌ PROBE FAILED: ${traceId}`, errorMessage);
            return {
                success: false,
                traceId,
                probeType,
                timestamp,
                error: errorMessage
            };
        }
    }
    static async runConnectionProbe(traceId, params) {
        return {
            type: 'connection',
            traceId,
            websocketStatus: 'checking...',
            browserStatus: 'checking...',
            serverStatus: 'checking...'
        };
    }
    static async runFeedbackProbe(traceId, params) {
        const level = params.level || 'info';
        // Send different log levels to test feedback
        switch (level) {
            case 'error':
                console.error(`🚨 PROBE ERROR TEST: ${traceId}`);
                break;
            case 'warn':
                console.warn(`⚠️ PROBE WARN TEST: ${traceId}`);
                break;
            case 'debug':
                console.debug(`🐛 PROBE DEBUG TEST: ${traceId}`);
                break;
            default:
                console.log(`📊 PROBE INFO TEST: ${traceId}`);
        }
        return {
            type: 'feedback',
            traceId,
            level,
            message: `Feedback probe executed at ${level} level`
        };
    }
    static async runTraceProbe(traceId, params) {
        const duration = params.duration || 1000;
        const startTime = Date.now();
        console.log(`🎯 TRACE PROBE START: ${traceId} (duration: ${duration}ms)`);
        // Simulate some async work
        await new Promise(resolve => setTimeout(resolve, Math.min(duration, 100)));
        const endTime = Date.now();
        const actualDuration = endTime - startTime;
        console.log(`🏁 TRACE PROBE END: ${traceId} (actual: ${actualDuration}ms)`);
        return {
            type: 'trace',
            traceId,
            requestedDuration: duration,
            actualDuration,
            startTime,
            endTime
        };
    }
    static async runSystemProbe(traceId, params) {
        console.log(`🖥️ SYSTEM PROBE: ${traceId}`);
        return {
            type: 'system',
            traceId,
            timestamp: Date.now(),
            userAgent: typeof navigator !== 'undefined' ? navigator.userAgent : 'server-side',
            url: typeof window !== 'undefined' ? window.location.href : 'server-side',
            performance: typeof performance !== 'undefined' ? performance.now() : 'unavailable'
        };
    }
}
export default ProbeCommand;
//# sourceMappingURL=ProbeCommand.js.map