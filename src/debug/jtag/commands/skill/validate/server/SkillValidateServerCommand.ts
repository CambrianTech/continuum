/**
 * Skill Validate Command - Server Implementation
 *
 * Validates a generated skill by running TypeScript compilation and tests
 * in an ExecutionSandbox. Updates SkillEntity with validation results.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { SkillValidateParams, SkillValidateResult } from '../shared/SkillValidateTypes';
import { createSkillValidateResultFromParams } from '../shared/SkillValidateTypes';
import { SkillEntity } from '@system/data/entities/SkillEntity';
import type { SkillValidationResults } from '@system/data/entities/SkillEntity';
import { DataDaemon } from '@daemons/data-daemon/shared/DataDaemon';
import { COLLECTIONS } from '@system/shared/Constants';
import { ExecutionSandbox } from '@system/code/server/ExecutionSandbox';
import type { SandboxConfig } from '@system/code/server/ExecutionSandbox';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

export class SkillValidateServerCommand extends CommandBase<SkillValidateParams, SkillValidateResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('skill/validate', context, subpath, commander);
  }

  async execute(params: SkillValidateParams): Promise<SkillValidateResult> {
    const { skillId } = params;

    if (!skillId?.trim()) {
      throw new ValidationError('skillId', "Missing required parameter 'skillId'.");
    }

    // Load skill entity
    const readResult = await DataDaemon.read<SkillEntity>(COLLECTIONS.SKILLS, skillId as UUID);
    if (!readResult.success || !readResult.data) {
      throw new ValidationError('skillId', `Skill not found: ${skillId}`);
    }
    const skill = readResult.data.data as SkillEntity;

    if (skill.status !== 'generated') {
      throw new ValidationError('skillId',
        `Skill '${skill.name}' cannot be validated in status '${skill.status}'. Must be 'generated' first.`);
    }

    if (!skill.outputDir) {
      throw new ValidationError('skillId', `Skill '${skill.name}' has no outputDir — was it generated?`);
    }

    const sandbox = new ExecutionSandbox();
    const errors: string[] = [];
    const startTime = Date.now();

    // Step 1: TypeScript compilation check
    const compileConfig: SandboxConfig = {
      command: 'npx',
      args: ['tsc', '--noEmit', '--pretty', '--project', 'tsconfig.json'],
      cwd: skill.outputDir,
      timeoutMs: 30_000,
      maxOutputBytes: 100_000,
      personaId: skill.createdById,
    };

    let compiled = false;
    try {
      const compileResult = await sandbox.execute(compileConfig);
      compiled = compileResult.exitCode === 0;
      if (!compiled) {
        errors.push(`Compilation failed (exit ${compileResult.exitCode}): ${compileResult.stderr || compileResult.stdout}`);
      }
    } catch (e) {
      errors.push(`Compilation error: ${e instanceof Error ? e.message : String(e)}`);
    }

    // Step 2: Run tests (only if compilation passed)
    let testsRun = 0;
    let testsPassed = 0;

    if (compiled) {
      const testConfig: SandboxConfig = {
        command: 'npx',
        args: ['vitest', 'run', '--reporter=json'],
        cwd: skill.outputDir,
        timeoutMs: 60_000,
        maxOutputBytes: 100_000,
        personaId: skill.createdById,
      };

      try {
        const testResult = await sandbox.execute(testConfig);
        // Parse vitest JSON output
        try {
          const output = testResult.stdout;
          const jsonMatch = output.match(/\{[\s\S]*"numTotalTests"[\s\S]*\}/);
          if (jsonMatch) {
            const parsed = JSON.parse(jsonMatch[0]);
            testsRun = parsed.numTotalTests ?? 0;
            testsPassed = parsed.numPassedTests ?? 0;
          }
        } catch {
          // If JSON parsing fails, count from exit code
          testsRun = testResult.exitCode === 0 ? 1 : 0;
          testsPassed = testResult.exitCode === 0 ? 1 : 0;
        }

        if (testResult.exitCode !== 0) {
          errors.push(`Tests failed (exit ${testResult.exitCode}): ${testResult.stderr || testResult.stdout}`);
        }
      } catch (e) {
        errors.push(`Test execution error: ${e instanceof Error ? e.message : String(e)}`);
      }
    }

    const durationMs = Date.now() - startTime;
    const passed = compiled && errors.length === 0;

    // Build validation results
    const validationResults: SkillValidationResults = {
      compiled,
      testsRun,
      testsPassed,
      errors,
      durationMs,
    };

    // Update entity
    const updateData: Partial<SkillEntity> = {
      validationResults,
      status: passed ? 'validated' : 'failed',
    };
    if (!passed) {
      updateData.failureReason = errors.join('; ');
    }
    await DataDaemon.update<SkillEntity>(
      COLLECTIONS.SKILLS,
      skill.id as UUID,
      updateData,
    );

    return createSkillValidateResultFromParams(params, {
      success: passed,
      skillId: skill.id,
      name: skill.name,
      status: passed ? 'validated' : 'failed',
      compiled,
      testsRun,
      testsPassed,
      errors,
      message: passed
        ? `Skill '${skill.name}' validated: compiled + ${testsPassed}/${testsRun} tests passed (${durationMs}ms)`
        : `Skill '${skill.name}' validation failed: ${errors[0] ?? 'unknown error'}`,
    });
  }
}
