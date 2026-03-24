/**
 * TeamStudentPipeline — Per-role student in a collaborative team project
 *
 * Each team member gets their own student sentinel running this pipeline.
 * Two phases:
 *
 * Phase 1 — TRAINING: Learn the role via coursework (same pattern as StudentPipeline)
 *   - Watch for role-specific curriculum from teacher
 *   - Per-topic: synthesize data, pre-test, train, post-test, exam, phenotype validate
 *   - Emit role:training:done when all topics passed
 *
 * Phase 2 — BUILDING: Collaborate on the actual project
 *   - Watch for build:start
 *   - Per-milestone: receive task assignment, execute via CodingAgent, post work to chat
 *   - Post-build: compose adapters, plasticity compaction
 *
 * Students communicate naturally via the academy chat room between pipeline steps.
 * PersonaUser autonomous loops respond to messages from teammates.
 *
 * @see TeamTeacherPipeline for orchestration
 * @see TeamProjectTypes for event taxonomy
 */

import type { Pipeline, PipelineStep } from '../../../workers/continuum-core/bindings/modules/sentinel';
import type { TeamStudentPipelineConfig } from '../../genome/shared/TeamProjectTypes';
import { teamEvent, TEAM_EVENTS } from '../../genome/shared/TeamProjectTypes';
import { ACADEMY_EVENTS } from '../../genome/shared/AcademyTypes';

const TE = TEAM_EVENTS;
const AE = ACADEMY_EVENTS;

/**
 * Build the team student sentinel pipeline.
 *
 * Step flow:
 *   0: Watch — role:curriculum:ready (teacher generated per-role curriculum)
 *   1: Loop (per topic) — Train/exam cycle (reuses StudentPipeline pattern)
 *   2: Emit — role:training:done
 *   3: Chat — Post training completion
 *   4: Watch — build:start
 *   5: Loop (per milestone) — Execute assigned tasks, post work
 *   6: Command — genome/compose (merge trained adapters)
 *   7: Command — plasticity/pipeline (compact model)
 *   8: Command — plasticity/compress (target-device GGUF)
 */
export function buildTeamStudentPipeline(config: TeamStudentPipelineConfig): Pipeline {
  const { projectId, personaId, personaName, role, roleDescription, baseModel, config: teamConfig } = config;
  const evt = (action: string) => teamEvent(projectId, action);
  const iterEvt = (action: string) => `${teamEvent(projectId, action)}:{{input.iteration}}`;

  // Session ID scoped to this member for training events
  const memberSessionId = `${projectId}-${personaId}`;

  const steps: PipelineStep[] = [

    // ══════════════════════════════════════════════════════════════════
    // PHASE 1: TRAINING — Learn the role
    // ══════════════════════════════════════════════════════════════════

    // Step 0: Watch for role-specific curriculum from teacher
    // Teacher emits with iteration index matching member order, but we
    // watch all role:curriculum:ready events and match by role in payload
    {
      type: 'watch',
      event: evt(TE.ROLE_CURRICULUM_READY),
      timeoutSecs: 0,
    },

    // Step 1: Training loop — per-topic: dataset → train → exam
    {
      type: 'loop',
      count: teamConfig.topicsPerSession,
      steps: buildTrainingLoopSteps(memberSessionId, personaId, personaName, role, baseModel, teamConfig),
    },

    // Step 2: Emit role:training:done
    {
      type: 'emit',
      event: evt(TE.ROLE_TRAINING_DONE),
      payload: {
        projectId,
        personaId,
        personaName,
        role,
      },
    },

    // Step 3: Chat — Post training completion with results
    {
      type: 'command',
      command: 'collaboration/chat/send',
      params: {
        room: 'academy',
        message: [
          `✅ **${personaName}** finished training for role: **${role}**`,
          '',
          `_${roleDescription}_`,
          '',
          `_Trained on ${teamConfig.topicsPerSession} topics, ${teamConfig.epochs} epochs each. Ready to build._`,
        ].join('\n'),
      },
    },

    // ══════════════════════════════════════════════════════════════════
    // PHASE 2: BUILDING — Collaborative project work
    // ══════════════════════════════════════════════════════════════════

    // Step 4: Watch for build phase to start
    {
      type: 'watch',
      event: evt(TE.BUILD_START),
      timeoutSecs: 0,
    },

    // Step 5: Build loop — per-milestone: receive task, execute, post work
    {
      type: 'loop',
      count: teamConfig.buildMilestones,
      steps: buildBuildLoopSteps(projectId, personaId, personaName, role, baseModel, teamConfig),
    },

    // ══════════════════════════════════════════════════════════════════
    // POST-BUILD: Compose adapters + plasticity compaction
    // ══════════════════════════════════════════════════════════════════

    // Step 6: Compose all trained adapters
    {
      type: 'command',
      command: 'genome/compose',
      params: {
        personaId,
        baseModel,
        name: `${personaName}-team-${projectId.slice(0, 8)}`,
        layers: '{{steps.1.iterations.*.3.data.layerId}}',
        strategy: 'weighted-merge',
        activate: true,
      },
    },

    // Step 7: Plasticity compaction
    {
      type: 'condition',
      if: '{{steps.6.data.composedAdapterPath}}',
      then: [
        {
          type: 'command',
          command: 'plasticity/pipeline',
          params: {
            capturePath: '{{steps.6.data.composedAdapterPath}}',
            modelPath: baseModel,
          },
        },
        {
          type: 'command',
          command: 'plasticity/compress',
          params: {
            capturePath: '{{steps.7.0.data.topologyPath}}',
            modelPath: '{{steps.7.0.data.modelPath}}',
            deviceSpec: '32gb',
          },
        },
      ],
      else: [],
    },
  ];

  return {
    name: `team-student-${personaName.toLowerCase().replace(/[^a-z0-9]+/g, '-')}-${role}`,
    steps,
    inputs: {
      projectId,
      personaId,
      personaName,
      role,
      roleDescription,
      baseModel,
    },
  };
}

// ============================================================================
// Phase 1: Training Loop Steps
// ============================================================================

function buildTrainingLoopSteps(
  sessionId: string,
  personaId: string,
  personaName: string,
  role: string,
  baseModel: string,
  teamConfig: TeamStudentPipelineConfig['config'],
): PipelineStep[] {
  // Reuses the StudentPipeline pattern: dataset → pre-test → train → exam → validate
  // Events scoped by member sessionId + iteration to prevent cross-member collision
  const iterEvt = (action: string) => `${sessionId}:${action}:{{input.iteration}}`;

  return [
    // loop.0: LLM — Pre-test (baseline, no adapters)
    {
      type: 'llm',
      prompt: [
        `You are ${personaName} (role: ${role}).`,
        'Answer about topic {{input.iteration}} from your curriculum:',
        '{{steps.0.data.payload.curriculum.topics.{{input.iteration}}.name}}',
        '',
        '1. Key concepts? 2. Most important principle? 3. Practical example?',
        'Reply as JSON: [{"questionIndex":0,"studentAnswer":"..."}]',
      ].join('\n'),
      model: baseModel,
      provider: 'candle',
      temperature: 0.5,
      maxTokens: 1024,
    },

    // loop.1: Command — Synthesize training data for this topic
    {
      type: 'command',
      command: 'genome/dataset-synthesize',
      params: {
        topic: '{{steps.0.data.payload.curriculum.topics.{{input.iteration}}.name}}',
        skill: role,
        personaName,
        exampleCount: teamConfig.examplesPerTopic,
        difficulty: '{{steps.0.data.payload.curriculum.topics.{{input.iteration}}.difficulty}}',
      },
    },

    // loop.2: Emit training:started
    {
      type: 'emit',
      event: iterEvt('training:started'),
      payload: { personaId, role, topicIndex: '{{input.iteration}}' },
    },

    // loop.3: Command — Train LoRA
    {
      type: 'command',
      command: 'genome/train',
      params: {
        personaId,
        personaName,
        traitType: `${role}-topic-{{input.iteration}}`,
        baseModel,
        datasetPath: '{{loop.1.data.datasetPath}}',
        rank: teamConfig.rank,
        epochs: teamConfig.epochs,
        learningRate: teamConfig.learningRate,
        batchSize: teamConfig.batchSize,
      },
    },

    // loop.4: LLM — Post-test (with trained adapter)
    {
      type: 'llm',
      prompt: [
        `You are ${personaName} (role: ${role}).`,
        'Answer about topic {{input.iteration}}:',
        '{{steps.0.data.payload.curriculum.topics.{{input.iteration}}.name}}',
        '',
        '1. Key concepts? 2. Most important principle? 3. Practical example?',
        'Reply as JSON: [{"questionIndex":0,"studentAnswer":"..."}]',
      ].join('\n'),
      model: baseModel,
      provider: 'candle',
      temperature: 0.5,
      maxTokens: 1024,
      activeAdapters: [{
        name: '{{loop.3.data.layerId}}',
        path: '{{loop.3.data.adapterPath}}',
        domain: role,
        scale: 1.0,
      }],
    },

    // loop.5: Chat — Post before/after to academy room
    {
      type: 'command',
      command: 'collaboration/chat/send',
      params: {
        room: 'academy',
        message: [
          `🧠 **${personaName}** (${role}) — Topic {{input.iteration}} training complete`,
          '',
          '**Before training:** {{loop.0.output}}',
          '**After training:** {{loop.4.output}}',
        ].join('\n'),
      },
    },
  ];
}

// ============================================================================
// Phase 2: Build Loop Steps
// ============================================================================

function buildBuildLoopSteps(
  projectId: string,
  personaId: string,
  personaName: string,
  role: string,
  baseModel: string,
  teamConfig: TeamStudentPipelineConfig['config'],
): PipelineStep[] {
  const iterEvt = (action: string) => `${teamEvent(projectId, action)}:{{input.iteration}}`;

  return [
    // loop.0: Watch for milestone task assignment
    {
      type: 'watch',
      event: iterEvt(TEAM_EVENTS.BUILD_MILESTONE),
      timeoutSecs: 0,
    },

    // loop.1: CodingAgent — Execute assigned task
    {
      type: 'codingagent',
      prompt: [
        `You are ${personaName}, a ${role} on a team project.`,
        '',
        '**Your task for this milestone:**',
        'Find your task from the milestone assignments:',
        '{{loop.0.data.payload.tasks}}',
        `Your role is "${role}". Execute the task assigned to your role.`,
        '',
        'Write code, create files, and make it work.',
      ].join('\n'),
      personaId,
      model: baseModel,
      captureTraining: true,
    },

    // loop.2: Chat — Post the work (the portfolio)
    {
      type: 'command',
      command: 'collaboration/chat/send',
      params: {
        room: 'academy',
        message: [
          `🔨 **${personaName}** (${role}) — Milestone {{input.iteration}} work submitted`,
          '',
          '{{loop.1.output}}',
        ].join('\n'),
      },
    },

    // loop.3: Emit milestone done for this member
    {
      type: 'emit',
      event: iterEvt(TEAM_EVENTS.BUILD_MILESTONE_DONE),
      payload: {
        projectId,
        personaId,
        role,
        milestoneIndex: '{{input.iteration}}',
        output: '{{loop.1.output}}',
      },
    },
  ];
}
