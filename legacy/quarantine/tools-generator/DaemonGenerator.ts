/**
 * Daemon Generator
 *
 * Generates daemon implementations from DaemonSpec.
 * Follows the three-layer pattern: shared/browser/server
 *
 * Features:
 * - Concurrency helpers (RateLimiter, AsyncQueue, Semaphore)
 * - Type-safe job methods
 * - Event emission support
 * - Lifecycle hooks (onStart, onStop)
 * - Metrics collection
 */

import * as fs from 'fs';
import * as path from 'path';
import type { DaemonSpec, DaemonJob, DaemonEvent } from './DaemonTypes';

interface GenerateOptions {
  force?: boolean; // Overwrite existing files
}

export class DaemonGenerator {
  private readonly rootPath: string;

  constructor(rootPath: string) {
    this.rootPath = rootPath;
  }

  /**
   * Generate daemon from spec
   */
  generate(spec: DaemonSpec, outputDir: string, options: GenerateOptions = {}): void {
    console.log(`\n🏗️  Generating daemon: ${spec.name}`);
    console.log(`   Output: ${outputDir}`);

    // Create directory structure
    this.createDirectoryStructure(outputDir);

    // Generate files
    this.generateSharedFile(spec, outputDir, options);
    this.generateBrowserFile(spec, outputDir, options);
    this.generateServerFile(spec, outputDir, options);

    console.log(`✅ Daemon generated successfully\n`);
  }

  private createDirectoryStructure(outputDir: string): void {
    const dirs = [
      outputDir,
      path.join(outputDir, 'shared'),
      path.join(outputDir, 'browser'),
      path.join(outputDir, 'server')
    ];

    for (const dir of dirs) {
      if (!fs.existsSync(dir)) {
        fs.mkdirSync(dir, { recursive: true });
      }
    }
  }

  private generateSharedFile(spec: DaemonSpec, outputDir: string, options: GenerateOptions): void {
    const className = this.pascalCase(spec.name);
    const filePath = path.join(outputDir, 'shared', `${className}.ts`);

    if (fs.existsSync(filePath) && !options.force) {
      console.log(`   ⏭️  Skipping ${className}.ts (already exists)`);
      return;
    }

    const content = this.generateSharedContent(spec);
    fs.writeFileSync(filePath, content);
    console.log(`   ✅ Generated shared/${className}.ts`);
  }

  private generateBrowserFile(spec: DaemonSpec, outputDir: string, options: GenerateOptions): void {
    const className = this.pascalCase(spec.name);
    const filePath = path.join(outputDir, 'browser', `${className}Browser.ts`);

    if (fs.existsSync(filePath) && !options.force) {
      console.log(`   ⏭️  Skipping ${className}Browser.ts (already exists)`);
      return;
    }

    const content = this.generateBrowserContent(spec);
    fs.writeFileSync(filePath, content);
    console.log(`   ✅ Generated browser/${className}Browser.ts`);
  }

  private generateServerFile(spec: DaemonSpec, outputDir: string, options: GenerateOptions): void {
    const className = this.pascalCase(spec.name);
    const filePath = path.join(outputDir, 'server', `${className}Server.ts`);

    if (fs.existsSync(filePath) && !options.force) {
      console.log(`   ⏭️  Skipping ${className}Server.ts (already exists)`);
      return;
    }

    const content = this.generateServerContent(spec);
    fs.writeFileSync(filePath, content);
    console.log(`   ✅ Generated server/${className}Server.ts`);
  }

  private generateSharedContent(spec: DaemonSpec): string {
    const className = this.pascalCase(spec.name);
    const jobMethods = spec.jobs.map(job => this.generateJobMethod(job)).join('\n\n');
    const jobHandlers = spec.jobs.map(job => this.generateJobHandler(job)).join('\n      ');

    return `/**
 * ${className} - ${spec.description}
 *
 * GENERATED FILE - DO NOT EDIT MANUALLY
 * Generated from DaemonSpec by DaemonGenerator
 */

import { DaemonBase } from '../../../daemons/command-daemon/shared/DaemonBase';
import type { JTAGContext, JTAGMessage, JTAGPayload } from '../../../system/core/types/JTAGTypes';
import { createPayload } from '../../../system/core/types/JTAGTypes';
import { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import { type BaseResponsePayload, createConsoleSuccessResponse, createConsoleErrorResponse } from '../../../system/core/types/ResponseTypes';

/**
 * ${className} Payload
 */
export interface ${className}Payload extends JTAGPayload {
  readonly type: '${spec.jobs.map(j => j.name).join("' | '")}';
  readonly params?: Record<string, unknown>;
}

/**
 * ${className} - Shared base class
 */
export abstract class ${className} extends DaemonBase {
  public readonly subpath: string = '${this.kebabCase(spec.name)}';

  constructor(context: JTAGContext, router: JTAGRouter) {
    super('${this.kebabCase(spec.name)}', context, router);
  }

  /**
   * Initialize daemon
   */
  protected async initialize(): Promise<void> {
    this.log.info(\`💾 \${this.toString()}: ${className} initialized\`);
    ${spec.lifecycle?.onStart ? `await this.onStart();` : ''}
  }

  /**
   * Handle incoming messages
   */
  async handleMessage(message: JTAGMessage): Promise<BaseResponsePayload> {
    const payload = message.payload as ${className}Payload;

    try {
      let result: BaseResponsePayload;

      switch (payload.type) {
${jobHandlers}
        default:
          result = createConsoleErrorResponse(
            \`Unknown job type: \${payload.type}\`,
            payload.context,
            payload.sessionId
          );
      }

      return result;
    } catch (error: unknown) {
      const errorMessage = error instanceof Error ? error.message : 'Unknown error';
      return createConsoleErrorResponse(errorMessage, payload.context, payload.sessionId);
    }
  }

  /**
   * Job methods (implement in subclass or override)
   */
${jobMethods}

  ${spec.lifecycle?.onStart ? `
  /**
   * Lifecycle: Start
   * ${spec.lifecycle.onStart}
   */
  protected async onStart(): Promise<void> {
    // TODO: Implement onStart logic
  }
  ` : ''}

  ${spec.lifecycle?.onStop ? `
  /**
   * Lifecycle: Stop
   * ${spec.lifecycle.onStop}
   */
  async shutdown(): Promise<void> {
    await super.shutdown();
    await this.onStop();
  }

  protected async onStop(): Promise<void> {
    // TODO: Implement onStop logic
  }
  ` : ''}
}
`;
  }

  private generateJobMethod(job: DaemonJob): string {
    const params = job.params.map(p => `${p.name}: ${p.type}`).join(', ');
    const returnType = job.async ? `Promise<${job.returns}>` : job.returns;

    return `  /**
   * ${job.description || `Job: ${job.name}`}
   */
  protected ${job.async ? 'async ' : ''}${job.name}(${params}): ${returnType} {
    // TODO: Implement ${job.name}
    throw new Error('${job.name} not implemented');
  }`;
  }

  private generateJobHandler(job: DaemonJob): string {
    const paramExtraction = job.params
      .map(p => `            const ${job.name}_${p.name} = payload.params?.${p.name} as ${p.type};`)
      .join('\n');

    const paramList = job.params.map(p => `${job.name}_${p.name}`).join(', ');

    return `          case '${job.name}':
${paramExtraction}
            result = createConsoleSuccessResponse(
              await this.${job.name}(${paramList}),
              payload.context,
              payload.sessionId
            );
            break;`;
  }

  private generateBrowserContent(spec: DaemonSpec): string {
    const className = this.pascalCase(spec.name);

    return `/**
 * ${className} Browser - Browser-specific implementation
 *
 * GENERATED FILE - DO NOT EDIT MANUALLY
 */

import { ${className} } from '../shared/${className}';
import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';

export class ${className}Browser extends ${className} {
  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);
  }

  // Browser-specific overrides go here
}
`;
  }

  private generateServerContent(spec: DaemonSpec): string {
    const className = this.pascalCase(spec.name);

    return `/**
 * ${className} Server - Server-specific implementation
 *
 * GENERATED FILE - DO NOT EDIT MANUALLY
 */

import { ${className} } from '../shared/${className}';
import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';

export class ${className}Server extends ${className} {
  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);
    // Logging automatically set up by DaemonBase
  }

  // Server-specific overrides go here
}
`;
  }

  private pascalCase(str: string): string {
    return str
      .split('-')
      .map(word => word.charAt(0).toUpperCase() + word.slice(1))
      .join('');
  }

  private kebabCase(str: string): string {
    return str.toLowerCase().replace(/\s+/g, '-');
  }
}
