import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { SecuritySetupParams, SecuritySetupResult, SetupStep } from '../shared/SecuritySetupTypes';
import { execSync } from 'child_process';
import * as path from 'path';
import * as fs from 'fs';

interface InstallationStatus {
    monitorScriptExists: boolean;
    proxyScriptExists: boolean;
    proxyInstalled: boolean;
    proxyInShells: boolean;
    terminalNotifierInstalled: boolean;
    monitorRunning: boolean;
    currentShell: string;
    logDirExists: boolean;
    logDirWritable: boolean;
}

export class SecuritySetupServerCommand extends CommandBase<
    SecuritySetupParams,
    SecuritySetupResult
> {
    constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
        super('security/setup', context, subpath, commander);
    }

    async execute(params: JTAGPayload): Promise<SecuritySetupResult> {
        const setupParams = params as SecuritySetupParams;
        const securityMonitorDir = path.resolve(process.cwd(), '../../../security-monitor');
        const logDir = '/var/log/ares';

        // Check current installation status
        const status = this.checkStatus(securityMonitorDir, logDir);

        if (setupParams.statusOnly) {
            return this.formatStatusResult(setupParams, status);
        }

        // Interactive setup - print instructions, user executes manually
        const component = setupParams.component ?? 'all';
        const steps = this.getSetupSteps(securityMonitorDir, component);

        return this.formatSetupInstructions(setupParams, status, steps);
    }

    private checkStatus(securityMonitorDir: string, logDir: string): InstallationStatus {
        const status: InstallationStatus = {
            monitorScriptExists: fs.existsSync(path.join(securityMonitorDir, 'monitor-screen-watchers.sh')),
            proxyScriptExists: fs.existsSync(path.join(securityMonitorDir, 'transparent-shell-proxy.sh')),
            proxyInstalled: fs.existsSync('/usr/local/bin/safebash'),
            proxyInShells: false,
            terminalNotifierInstalled: false,
            monitorRunning: false,
            currentShell: '',
            logDirExists: fs.existsSync(logDir),
            logDirWritable: false,
        };

        // Check if terminal-notifier is installed
        try {
            execSync('which terminal-notifier', { stdio: 'ignore' });
            status.terminalNotifierInstalled = true;
        } catch {
            // Not installed
        }

        // Check if monitor is running
        try {
            const procs = execSync('pgrep -f "monitor-screen-watchers"', { encoding: 'utf-8' });
            status.monitorRunning = procs.trim().length > 0;
        } catch {
            // Not running
        }

        // Check current shell
        try {
            status.currentShell = process.env.SHELL ?? '';
        } catch {
            // Can't determine
        }

        // Check if proxy is in /etc/shells
        try {
            const shells = fs.readFileSync('/etc/shells', 'utf-8');
            status.proxyInShells = shells.includes('/usr/local/bin/safebash');
        } catch {
            // Can't read
        }

        // Check if log directory is writable
        if (status.logDirExists) {
            try {
                fs.accessSync(logDir, fs.constants.W_OK);
                status.logDirWritable = true;
            } catch {
                // Not writable
            }
        }

        return status;
    }

    private getSetupSteps(securityMonitorDir: string, component: string): SetupStep[] {
        const steps: SetupStep[] = [];

        // Monitor setup
        if (component === 'monitor' || component === 'all') {
            steps.push({
                name: 'Install terminal-notifier',
                description: 'Required for clickable notifications',
                command: 'brew install terminal-notifier',
                requiresSudo: false,
                optional: false,
            });

            steps.push({
                name: 'Create log directory',
                description: 'Create /var/log/ares for security logs',
                command: 'sudo mkdir -p /var/log/ares && sudo chown $(whoami) /var/log/ares',
                requiresSudo: true,
                optional: false,
            });

            steps.push({
                name: 'Start monitor daemon',
                description: 'Start background process monitoring',
                command: `cd ${securityMonitorDir} && ./monitor-screen-watchers.sh &`,
                requiresSudo: false,
                optional: false,
            });
        }

        // Proxy setup
        if (component === 'proxy' || component === 'all') {
            steps.push({
                name: 'Install transparent proxy',
                description: 'Create symlink in /usr/local/bin',
                command: `sudo ln -sf ${securityMonitorDir}/transparent-shell-proxy.sh /usr/local/bin/safebash`,
                requiresSudo: true,
                optional: false,
            });

            steps.push({
                name: 'Add proxy to valid shells',
                description: 'Register safebash as a valid login shell',
                command: 'echo "/usr/local/bin/safebash" | sudo tee -a /etc/shells',
                requiresSudo: true,
                optional: false,
            });

            steps.push({
                name: 'Activate proxy (OPTIONAL)',
                description: 'Switch your shell to safebash (WARNING: Only if you understand what this does)',
                command: 'chsh -s /usr/local/bin/safebash',
                requiresSudo: false,
                optional: true,
            });
        }

        return steps;
    }

    private formatStatusResult(params: SecuritySetupParams, status: InstallationStatus): SecuritySetupResult {
        return transformPayload(params, {
            success: true,
            installed: {
                monitor: status.monitorScriptExists,
                proxy: status.proxyInstalled && status.proxyInShells,
                terminalNotifier: status.terminalNotifierInstalled,
            },
            status: {
                monitorRunning: status.monitorRunning,
                proxyActive: status.currentShell === '/usr/local/bin/safebash',
                logDirectory: status.logDirExists && status.logDirWritable ? '/var/log/ares' : 'NOT CONFIGURED',
            },
            nextSteps: this.generateNextSteps(status),
        });
    }

    private formatSetupInstructions(params: SecuritySetupParams, status: InstallationStatus, steps: SetupStep[]): SecuritySetupResult {
        const manualCommands: string[] = [];
        const nextSteps: string[] = [];

        nextSteps.push('='.repeat(70));
        nextSteps.push('SECURITY MONITOR SETUP - Interactive Installation');
        nextSteps.push('='.repeat(70));
        nextSteps.push('');
        nextSteps.push('⚠️  IMPORTANT: You must run these commands MANUALLY');
        nextSteps.push('   This is a security feature - no auto-sudo, no silent installs.');
        nextSteps.push('');

        for (const step of steps) {
            // Skip already completed steps
            if (this.isStepComplete(step, status)) {
                nextSteps.push(`✅ ${step.name} - Already installed`);
                continue;
            }

            nextSteps.push('');
            nextSteps.push(`📋 ${step.name}${step.optional ? ' (OPTIONAL)' : ''}`);
            nextSteps.push(`   ${step.description}`);
            nextSteps.push('');

            if (step.requiresSudo) {
                nextSteps.push('   ⚠️  This command requires sudo (you\'ll be prompted for password)');
            }

            nextSteps.push(`   $ ${step.command}`);
            nextSteps.push('');

            manualCommands.push(step.command);
        }

        nextSteps.push('='.repeat(70));
        nextSteps.push('After running these commands, check status with:');
        nextSteps.push('   $ ./jtag security/setup --statusOnly');
        nextSteps.push('='.repeat(70));

        return transformPayload(params, {
            success: true,
            installed: {
                monitor: status.monitorScriptExists && status.terminalNotifierInstalled,
                proxy: status.proxyInstalled && status.proxyInShells,
                terminalNotifier: status.terminalNotifierInstalled,
            },
            status: {
                monitorRunning: status.monitorRunning,
                proxyActive: status.currentShell === '/usr/local/bin/safebash',
                logDirectory: status.logDirExists && status.logDirWritable ? '/var/log/ares' : 'NOT CONFIGURED',
            },
            nextSteps,
            manualCommands,
        });
    }

    private isStepComplete(step: SetupStep, status: InstallationStatus): boolean {
        if (step.name.includes('terminal-notifier')) {
            return status.terminalNotifierInstalled;
        }
        if (step.name.includes('log directory')) {
            return status.logDirExists && status.logDirWritable;
        }
        if (step.name.includes('monitor daemon')) {
            return status.monitorRunning;
        }
        if (step.name.includes('transparent proxy')) {
            return status.proxyInstalled;
        }
        if (step.name.includes('valid shells')) {
            return status.proxyInShells;
        }
        if (step.name.includes('Activate proxy')) {
            return status.currentShell === '/usr/local/bin/safebash';
        }
        return false;
    }

    private generateNextSteps(status: InstallationStatus): string[] {
        const steps: string[] = [];

        if (!status.terminalNotifierInstalled) {
            steps.push('Install terminal-notifier: brew install terminal-notifier');
        }

        if (!status.logDirExists || !status.logDirWritable) {
            steps.push('Create log directory: sudo mkdir -p /var/log/ares && sudo chown $(whoami) /var/log/ares');
        }

        if (!status.monitorRunning) {
            steps.push('Start monitor: cd security-monitor && ./monitor-screen-watchers.sh &');
        }

        if (!status.proxyInstalled || !status.proxyInShells) {
            steps.push('Run: ./jtag security/setup --component=proxy (for installation instructions)');
        }

        if (steps.length === 0) {
            steps.push('✅ All components installed and running!');
            steps.push('View live report: cd security-monitor && ./view-report.sh');
        }

        return steps;
    }
}
