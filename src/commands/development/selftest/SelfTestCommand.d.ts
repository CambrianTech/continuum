/**
 * SelfTest Command - TypeScript Implementation
 * JTAG System Verification with Browser/DevTools Testing
 */
import { CommandDefinition } from '../../core/BaseCommand';
export interface SelfTestParams {
    verbose?: boolean;
    mode?: 'simple' | 'browser' | 'devtools' | 'full';
}
export interface SelfTestResult {
    success: boolean;
    message: string;
    tests: TestResults;
    mode: string;
    error?: string;
}
export interface TestResults {
    simple: boolean;
    browser: boolean;
    devtools: boolean;
    screenshot: boolean;
}
export interface SelfTestContext {
    continuum?: any;
}
export declare class SelfTestCommand {
    private static readonly DEFAULT_MODE;
    private static readonly DEFAULT_VERBOSE;
    static getDefinition(): CommandDefinition;
    static execute(params: SelfTestParams, context?: SelfTestContext): Promise<SelfTestResult>;
    private static testSimpleSystem;
    private static testBrowserManagement;
    private static testDevToolsIntegration;
    private static testScreenshotCapability;
    private static createErrorResult;
    private static isValidMode;
    protected static parseParams<T = any>(params: any): T;
}
export default SelfTestCommand;
//# sourceMappingURL=SelfTestCommand.d.ts.map