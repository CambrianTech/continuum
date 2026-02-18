/**
 * Pipe Chain Server Command
 * Enables Unix-style command chaining: cmd1 | cmd2 | cmd3
 */

import { CommandBase } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import { PipeChainParams, PipeChainResult, PipeStepResult, createPipeChainResult } from '../shared/PipeChainTypes';
import { execSync } from 'child_process';

export class PipeChainServerCommand extends CommandBase<PipeChainParams, PipeChainResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('utilities/pipe/chain', context, subpath, commander);
  }

  async execute(params: PipeChainParams): Promise<PipeChainResult> {
    const startTime = Date.now();
    const steps: PipeStepResult[] = [];

    try {
      // Parse commands from pipe-separated string
      const commands = params.commands.split('|').map(cmd => cmd.trim());

      if (commands.length === 0) {
        throw new Error('No commands provided');
      }

      let currentInput: string | undefined;
      let finalOutput: any;

      // Execute commands in sequence, piping output to next command
      for (let i = 0; i < commands.length; i++) {
        const command = commands[i];
        const stepStartTime = Date.now();

        try {
          // Build full command with input piping if needed
          let fullCommand = `./jtag ${command}`;

          // If we have input from previous command, pipe it
          if (currentInput && i > 0) {
            // For JSON input, we can pass it as stdin or as a parameter
            if (this.isJSONString(currentInput)) {
              // Try to pipe JSON data to next command
              fullCommand = `echo '${currentInput.replace(/'/g, "'")}' | ${fullCommand}`;
            } else {
              // For text data, pipe directly
              fullCommand = `echo "${currentInput}" | ${fullCommand}`;
            }
          }

          console.log(`🔗 Executing step ${i + 1}/${commands.length}: ${command}`);

          // Execute command
          const output = execSync(fullCommand, {
            encoding: 'utf8',
            cwd: process.cwd(),
            timeout: params.timeout || 30000,
            maxBuffer: 1024 * 1024 * 10 // 10MB buffer
          });

          const stepResult: PipeStepResult = {
            command,
            success: true,
            output: this.parseOutput(output),
            executionTime: Date.now() - stepStartTime
          };

          steps.push(stepResult);
          currentInput = output;
          finalOutput = stepResult.output;

          if (params.showIntermediate) {
            console.log(`✅ Step ${i + 1} completed in ${stepResult.executionTime}ms`);
          }

        } catch (stepError) {
          const stepResult: PipeStepResult = {
            command,
            success: false,
            output: null,
            executionTime: Date.now() - stepStartTime,
            error: stepError instanceof Error ? stepError.message : String(stepError)
          };

          steps.push(stepResult);

          // Handle error based on errorHandling strategy
          if (params.errorHandling === 'stop' || !params.errorHandling) {
            throw new Error(`Command failed at step ${i + 1}: ${stepResult.error}`);
          } else if (params.errorHandling === 'continue') {
            console.warn(`⚠️  Step ${i + 1} failed, continuing: ${stepResult.error}`);
            currentInput = ''; // Reset input for next command
          }
          // For 'collect', we just continue and collect the error
        }
      }

      return createPipeChainResult(params, {
        success: true,
        steps,
        finalOutput,
        totalExecutionTime: Date.now() - startTime
      });

    } catch (error) {
      return createPipeChainResult(params, {
        success: false,
        steps,
        finalOutput: null,
        totalExecutionTime: Date.now() - startTime,
        error: error instanceof Error ? error.message : String(error)
      });
    }
  }

  private isJSONString(str: string): boolean {
    try {
      JSON.parse(str);
      return true;
    } catch {
      return false;
    }
  }

  private parseOutput(output: string): any {
    // Try to parse as JSON first
    try {
      return JSON.parse(output);
    } catch {
      // If not JSON, return as string
      return output.trim();
    }
  }

  getCommandName(): string {
    return 'pipe/chain';
  }

  getDescription(): string {
    return 'Chain JTAG commands together with Unix-style piping';
  }

  getUsageExamples(): string[] {
    return [
      'pipe/chain --commands="debug/widget-state --widgetSelector=chat-widget | filter/json --path=messages"',
      'pipe/chain --commands="test/run/suite --profile=chat | filter/json --path=failures" --format=json',
      'pipe/chain --commands="data/list --collection=User | tree/json --compact" --show-intermediate'
    ];
  }
}