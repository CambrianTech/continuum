/**
 * WebSocket Server Fix Verification Test
 * Modular, dependency-injected test for CommandSystemBridge fix
 */
import { BaseCommand } from '../../core/BaseCommand.js';
class DefaultProcessRunner {
    async spawn(command, args, options = {}) {
        const { spawn } = await import('child_process');
        return new Promise((resolve) => {
            let stdout = '';
            let stderr = '';
            const process = spawn(command, args, { stdio: 'pipe', ...options });
            process.stdout?.on('data', (data) => stdout += data.toString());
            process.stderr?.on('data', (data) => stderr += data.toString());
            process.on('close', (code) => {
                resolve({
                    code: code || 0,
                    stdout,
                    stderr,
                    success: code === 0 || code === 143 // 143 = SIGTERM (normal for timeout)
                });
            });
            // Auto-timeout
            setTimeout(() => process.kill('SIGTERM'), options.timeout || 8000);
        });
    }
}
export class WebSocketServerTest extends BaseCommand {
    static getDefinition() {
        return {
            name: 'websocket-test',
            description: 'Verify WebSocket server CommandSystemBridge fix',
            category: 'development',
            params: 'timeout?: number, verbose?: boolean',
            examples: [
                'websocket-test',
                'websocket-test timeout:10 verbose:true'
            ],
            icon: '🔌'
        };
    }
    static async execute(params, context) {
        const config = {
            timeout: params.timeout || 8000,
            verbose: params.verbose || false,
            processRunner: new DefaultProcessRunner(),
            serverCommand: 'node',
            serverArgs: ['src/core/continuum-core.cjs']
        };
        console.log('🧪 Testing WebSocket server fix...');
        const result = await config.processRunner.spawn(config.serverCommand, config.serverArgs, { timeout: config.timeout });
        const markers = this.analyzeOutput(result.stdout + result.stderr);
        if (config.verbose) {
            console.log('📋 Server output:', result.stdout.slice(-500));
            if (result.stderr) {
                console.log('⚠️  Server errors:', result.stderr.slice(-300));
            }
        }
        const success = markers.bridgeInitialized &&
            markers.commandsMapPopulated &&
            !markers.crashDetected;
        this.logResults(markers, success);
        return {
            success,
            message: success ?
                'WebSocket server fix verified successfully' :
                this.getFailureMessage(markers),
            data: { markers },
            markers,
            output: config.verbose ? result.stdout + result.stderr : undefined
        };
    }
    static analyzeOutput(output) {
        return {
            bridgeInitialized: this.hasAnyMarker(output, this.VERIFICATION_MARKERS.bridgeInit),
            commandsMapPopulated: this.hasAnyMarker(output, this.VERIFICATION_MARKERS.commandsMap),
            webSocketReady: this.hasAnyMarker(output, this.VERIFICATION_MARKERS.webSocketReady),
            crashDetected: this.hasAnyMarker(output, this.VERIFICATION_MARKERS.crashIndicators)
        };
    }
    static hasAnyMarker(output, markers) {
        return markers.some(marker => {
            const regex = new RegExp(marker, 'i');
            return regex.test(output);
        });
    }
    static logResults(markers, success) {
        console.log('\n📊 Verification Results:');
        console.log(`   Bridge Initialized: ${markers.bridgeInitialized ? '✅' : '❌'}`);
        console.log(`   Commands Map Populated: ${markers.commandsMapPopulated ? '✅' : '❌'}`);
        console.log(`   WebSocket Ready: ${markers.webSocketReady ? '✅' : '❌'}`);
        console.log(`   Crash Detected: ${markers.crashDetected ? '❌' : '✅'}`);
        console.log(`   Overall: ${success ? '✅ PASS' : '❌ FAIL'}`);
    }
    static getFailureMessage(markers) {
        if (markers.crashDetected) {
            return 'WebSocket server crashed - .commands.entries() undefined error detected';
        }
        if (!markers.bridgeInitialized) {
            return 'CommandSystemBridge failed to initialize';
        }
        if (!markers.commandsMapPopulated) {
            return 'Commands map was not populated - WebSocket server may still crash';
        }
        return 'Unknown verification failure';
    }
}
WebSocketServerTest.VERIFICATION_MARKERS = {
    bridgeInit: [
        'CommandSystemBridge: Initializing',
        'dual-system support'
    ],
    commandsMap: [
        'Updated commands map with',
        'commands map'
    ],
    webSocketReady: [
        'WebSocketServer',
        'WebSocket',
        'User connected'
    ],
    crashIndicators: [
        'commands.entries() undefined',
        'commands.entries is not a function',
        'Cannot read property \'entries\'',
        'TypeError.*entries'
    ]
};
//# sourceMappingURL=WebSocketServerTest.js.map