/**
 * Genome Demo Run Command - Server Implementation
 *
 * Loads a project spec, builds a demo pipeline via buildDemoPipeline(),
 * and dispatches it to the Rust sentinel executor via sentinel/run.
 *
 * The pipeline has Claude Code build real software milestone by milestone,
 * capturing every interaction for LoRA training. After all milestones:
 * genome/train produces a LoRA adapter, genome/phenotype-validate proves improvement.
 */

import * as fs from 'fs';
import * as path from 'path';
import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { GenomeDemoRunParams, GenomeDemoRunResult } from '../shared/GenomeDemoRunTypes';
import { createGenomeDemoRunResultFromParams } from '../shared/GenomeDemoRunTypes';
import { Commands } from '@system/core/shared/Commands';
import type { SentinelStep } from '@system/sentinel/SentinelDefinition';
import type { PipelineSentinelParams, SentinelRunResult } from '@commands/sentinel/run/shared/SentinelRunTypes';
import type { ProjectSpec } from '@system/genome/shared/AcademyTypes';
import { buildDemoPipeline } from '@system/sentinel/pipelines/DemoPipeline';
import { DEFAULT_DEMO_TRAINING_CONFIG, DEMO_DEFAULTS } from '@system/sentinel/pipelines/DemoTypes';
import type { DemoPipelineConfig } from '@system/sentinel/pipelines/DemoTypes';
import { LOCAL_MODELS } from '@system/shared/Constants';

export class GenomeDemoRunServerCommand extends CommandBase<GenomeDemoRunParams, GenomeDemoRunResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome/demo-run', context, subpath, commander);
  }

  async execute(params: GenomeDemoRunParams): Promise<GenomeDemoRunResult> {
    const { project, personaId } = params;

    if (!project) {
      throw new ValidationError('project', 'Required. Project name (e.g., "task-tracker").');
    }
    if (!personaId) {
      throw new ValidationError('personaId', 'Required. Target persona ID for training.');
    }

    // Resolve project directory
    const srcRoot = path.resolve(__dirname, '../../../../');
    const projectDir = path.join(srcRoot, 'projects', project);
    const projectJsonPath = path.join(projectDir, 'project.json');

    if (!fs.existsSync(projectJsonPath)) {
      return createGenomeDemoRunResultFromParams(params, {
        success: false,
        handle: '',
        projectName: project,
        milestoneCount: 0,
        error: `Project not found: ${projectJsonPath}. Available projects are in src/projects/.`,
      });
    }

    // Parse project spec
    let projectSpec: ProjectSpec;
    try {
      projectSpec = JSON.parse(fs.readFileSync(projectJsonPath, 'utf8'));
    } catch (err) {
      return createGenomeDemoRunResultFromParams(params, {
        success: false,
        handle: '',
        projectName: project,
        milestoneCount: 0,
        error: `Failed to parse project.json: ${err}`,
      });
    }

    // Validate scaffold exists
    const scaffoldDir = path.join(projectDir, 'scaffold');
    if (!fs.existsSync(path.join(scaffoldDir, 'package.json'))) {
      return createGenomeDemoRunResultFromParams(params, {
        success: false,
        handle: '',
        projectName: projectSpec.name,
        milestoneCount: projectSpec.milestones.length,
        error: `Missing scaffold/package.json in ${projectDir}`,
      });
    }

    // Validate test files exist
    for (const milestone of projectSpec.milestones) {
      const testPath = path.join(projectDir, milestone.testFile);
      if (!fs.existsSync(testPath)) {
        return createGenomeDemoRunResultFromParams(params, {
          success: false,
          handle: '',
          projectName: projectSpec.name,
          milestoneCount: projectSpec.milestones.length,
          error: `Missing test file: ${milestone.testFile} (milestone ${milestone.index}: ${milestone.name})`,
        });
      }
    }

    const personaName = params.personaName ?? 'demo-persona';
    const baseModel = params.baseModel ?? LOCAL_MODELS.DEFAULT;

    console.log(`\u{1F680} DEMO RUN: project="${projectSpec.name}", persona="${personaName}", milestones=${projectSpec.milestones.length}`);

    // Build demo pipeline config
    const config: DemoPipelineConfig = {
      projectDir,
      project: projectSpec,
      personaId,
      personaName,
      baseModel,
      maxRetries: params.maxRetries ?? DEMO_DEFAULTS.maxRetries,
      maxBudgetPerMilestone: params.maxBudget ?? DEMO_DEFAULTS.maxBudgetPerMilestone,
      maxTurnsPerMilestone: params.maxTurns ?? DEMO_DEFAULTS.maxTurnsPerMilestone,
      provider: params.provider ?? DEMO_DEFAULTS.provider,
      training: {
        ...DEFAULT_DEMO_TRAINING_CONFIG,
        ...(params.epochs !== undefined && { epochs: params.epochs }),
        ...(params.rank !== undefined && { rank: params.rank }),
      },
    };

    // Build the pipeline
    const pipeline = buildDemoPipeline(config);

    console.log(`   Pipeline: ${pipeline.name}, ${pipeline.steps.length} top-level steps`);

    // Dispatch to Rust sentinel executor
    // PipelineStep[] (Rust bindings) → SentinelStep[] (TS definitions) — structurally compatible wire types
    const pipelineSteps = pipeline.steps as unknown as SentinelStep[];

    const sentinelResult = await Commands.execute<PipelineSentinelParams, SentinelRunResult>('sentinel/run', {
      type: 'pipeline',
      definition: {
        type: 'pipeline',
        name: pipeline.name ?? `demo-${project}`,
        description: `Demo pipeline: Claude Code builds ${projectSpec.name} (${projectSpec.milestones.length} milestones), captures training data for ${personaName}`,
        version: '1.0',
        steps: pipelineSteps,
        loop: { type: 'once' },
        tags: ['demo', project, projectSpec.skill],
      },
      parentPersonaId: personaId,
      sentinelName: pipeline.name ?? `demo-${project}`,
    });

    const handle = sentinelResult.handle ?? '';
    console.log(`\u2705 DEMO RUN: Pipeline dispatched, handle=${handle}`);

    return createGenomeDemoRunResultFromParams(params, {
      success: true,
      handle,
      projectName: projectSpec.name,
      milestoneCount: projectSpec.milestones.length,
    });
  }
}
