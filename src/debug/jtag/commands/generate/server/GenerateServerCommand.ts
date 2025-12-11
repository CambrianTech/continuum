/**
 * Generate Command - Server Implementation
 *
 * Generate a new command from a CommandSpec JSON definition
 */

import { CommandBase, type ICommandDaemon } from '../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import type { GenerateParams, GenerateResult } from '../shared/GenerateTypes';
import { createGenerateResultFromParams } from '../shared/GenerateTypes';
import { CommandGenerator } from '../../../generator/CommandGenerator';
import type { CommandSpec } from '../../../generator/CommandNaming';
import * as path from 'path';
import * as fs from 'fs';

export class GenerateServerCommand extends CommandBase<GenerateParams, GenerateResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('Generate', context, subpath, commander);
  }

  async execute(params: GenerateParams): Promise<GenerateResult> {
    try {
      console.log('🔧 SERVER: Executing Generate', params);

      // If template mode, return example CommandSpec
      if (params.template) {
        const templateSpec = this.generateExampleSpec();
        return createGenerateResultFromParams(params, {
          success: true,
          templateSpec,
          filesCreated: [],
          commandPath: ''
        });
      }

      // Validate spec parameter
      if (!params.spec) {
        throw new Error('spec parameter is required when template=false');
      }

      // Parse spec parameter - can be a file path, "-" for stdin, or a JSON string
      let commandSpec: CommandSpec;

      if (typeof params.spec === 'string') {
        // It's a file path or JSON string
        if (params.spec === '-') {
          // Read from stdin
          const stdin = fs.readFileSync(0, 'utf-8');
          commandSpec = JSON.parse(stdin);
        } else if (params.spec.startsWith('{')) {
          // It's inline JSON
          commandSpec = JSON.parse(params.spec);
        } else {
          // It's a file path
          if (!fs.existsSync(params.spec)) {
            throw new Error(`Spec file not found: ${params.spec}`);
          }
          const fileContent = fs.readFileSync(params.spec, 'utf-8');
          commandSpec = JSON.parse(fileContent);
        }
      } else {
        // It's already an object (passed from TypeScript code)
        commandSpec = params.spec as CommandSpec;
      }

      // Validate required fields
      if (!commandSpec.name) {
        throw new Error('CommandSpec must have a "name" field');
      }
      if (!commandSpec.description) {
        throw new Error('CommandSpec must have a "description" field');
      }

      // Determine root path (3 levels up from this file)
      const rootPath = path.resolve(__dirname, '../../..');

      // Create generator instance
      const generator = new CommandGenerator(rootPath);

      // Determine output directory
      const commandPath = path.join(rootPath, 'commands', commandSpec.name);

      // Check if command already exists
      if (fs.existsSync(commandPath)) {
        throw new Error(`Command already exists at: ${commandPath}\nDelete it first if you want to regenerate.`);
      }

      // Generate command files
      const filesCreated: string[] = [];

      // Capture console output to track created files
      const originalLog = console.log;
      console.log = (message: string, ...args: any[]) => {
        if (message.startsWith('✅ Created:')) {
          const filePath = message.replace('✅ Created:', '').trim();
          filesCreated.push(filePath);
        }
        originalLog(message, ...args);
      };

      try {
        generator.generate(commandSpec);
      } finally {
        console.log = originalLog;
      }

      console.log(`\n🎉 Generated ${filesCreated.length} files for command: ${commandSpec.name}`);

      return createGenerateResultFromParams(params, {
        success: true,
        filesCreated,
        commandPath
      });

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.error('❌ SERVER: Generate error:', errorMessage);

      return createGenerateResultFromParams(params, {
        success: false,
        error: errorMessage
      });
    }
  }

  /**
   * Generate example CommandSpec template
   */
  private generateExampleSpec(): CommandSpec {
    return {
      name: 'echo',
      description: 'Echo back a message with timestamp and context information',
      params: [
        {
          name: 'message',
          type: 'string',
          optional: false,
          description: 'The message to echo back'
        },
        {
          name: 'uppercase',
          type: 'boolean',
          optional: true,
          description: 'Convert message to uppercase'
        }
      ],
      results: [
        {
          name: 'echoedMessage',
          type: 'string',
          description: 'The echoed message (possibly transformed)'
        },
        {
          name: 'timestamp',
          type: 'number',
          description: 'Unix timestamp when command executed'
        },
        {
          name: 'sessionId',
          type: 'string',
          description: 'Session ID that executed the command'
        }
      ],
      examples: [
        {
          description: 'Basic echo',
          command: './jtag echo --message="Hello World"',
          expectedResult: '{ echoedMessage: "Hello World", timestamp: 1733520000000, sessionId: "..." }'
        },
        {
          description: 'Echo with uppercase transformation',
          command: './jtag echo --message="Hello World" --uppercase=true',
          expectedResult: '{ echoedMessage: "HELLO WORLD", timestamp: 1733520000000, sessionId: "..." }'
        }
      ],
      accessLevel: 'ai-safe'
    };
  }
}
