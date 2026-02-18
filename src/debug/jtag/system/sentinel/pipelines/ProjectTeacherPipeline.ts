/**
 * ProjectTeacherPipeline — Sentinel pipeline for multi-milestone project-based Academy training
 *
 * The project teacher orchestrates a cold-then-warm learning loop per milestone:
 * 1. Read project.json, scaffold working directory, npm install
 * 2. For each milestone:
 *    a. Read milestone test file, emit milestone:ready
 *    b. Watch for student's COLD attempt (baseline, no pre-training)
 *    c. Analyze attempt with agentMode (cloud LLM reads actual code, diagnoses gaps)
 *    d. Synthesize training data grounded in the student's real mistakes
 *    e. Wait for student to train LoRA
 *    f. Emit milestone:retry with feedback + hints
 *    g. Watch for student's WARM attempt (trained adapter)
 *    h. Evaluate test output — pass → next milestone, fail → retry loop
 * 3. Emit session:complete
 *
 * Key insight: the teacher uses agentMode for analysis so it can read files,
 * run diagnostics, and optionally clean up non-pedagogical issues like a
 * tutor who erases the whiteboard before setting up the next problem.
 *
 * State accumulates: milestone 3 builds on milestone 2's code.
 */

import type { Pipeline, PipelineStep } from '../../../workers/continuum-core/bindings/modules/sentinel';
import type { ProjectTeacherPipelineConfig } from '../../genome/shared/AcademyTypes';
import { academyEvent } from '../../genome/shared/AcademyTypes';

/**
 * Build the project teacher sentinel pipeline.
 *
 * Step flow:
 *   0: Shell — Read project.json
 *   1: Shell — Create working dir, copy scaffold, npm install
 *   2: Emit — project:setup:complete { workingDir }
 *   3: Emit — curriculum:ready { milestones from project.json }
 *   4: Loop (milestoneCount iterations):
 *     loop.0:  Shell — Read milestone test file
 *     loop.1:  Emit — milestone:ready { spec, testContent }
 *     loop.2:  Watch — milestone:attempted (COLD)
 *     loop.3:  LLM (agentMode) — Analyze student's code, diagnose gaps
 *     loop.4:  Command — genome/dataset-synthesize (grounded in real mistakes)
 *     loop.5:  Emit — dataset:ready
 *     loop.6:  Watch — training:complete
 *     loop.7:  Emit — milestone:retry { feedback, hints }
 *     loop.8:  Watch — milestone:attempted (WARM)
 *     loop.9:  LLM — Evaluate test output, decide pass/fail
 *     loop.10: Condition — passed?
 *       Then: [Emit milestone:passed]
 *       Else: [Emit session:failed — no inner retry for v1]
 *   5: Emit — session:complete
 */
export function buildProjectTeacherPipeline(config: ProjectTeacherPipelineConfig): Pipeline {
  const {
    sessionId,
    skill,
    personaName,
    baseModel,
    projectDir,
    config: academyConfig,
  } = config;

  const evt = (action: string) => academyEvent(sessionId, action as any);

  const steps: PipelineStep[] = [
    // Step 0: Read project.json
    {
      type: 'shell',
      cmd: `cat project.json`,
      workingDir: projectDir,
    },

    // Step 1: Create working dir from scaffold, install deps
    {
      type: 'shell',
      cmd: [
        `WORKDIR=$(mktemp -d)`,
        `cp -r scaffold/* "$WORKDIR/"`,
        `cp -r tests "$WORKDIR/"`,
        `cd "$WORKDIR"`,
        `npm install --silent 2>&1`,
        `echo "$WORKDIR"`,
      ].join('\n'),
      workingDir: projectDir,
      timeoutSecs: 120,
    },

    // Step 2: Emit project:setup:complete with working dir path
    // The last line of step 1's output is the WORKDIR path
    {
      type: 'emit',
      event: evt('project:setup:complete'),
      payload: {
        sessionId,
        workingDir: '{{steps.1.output}}',
      },
    },

    // Step 3: Emit curriculum:ready with milestones from project.json
    {
      type: 'emit',
      event: evt('curriculum:ready'),
      payload: {
        sessionId,
        milestones: '{{steps.0.output}}',
      },
    },

    // Step 4: Milestone loop — iterate over each milestone
    {
      type: 'loop',
      count: config.milestones.length,
      steps: buildMilestoneLoopSteps(sessionId, skill, personaName, projectDir, academyConfig, evt),
    },

    // Step 5: Emit session:complete
    {
      type: 'emit',
      event: evt('session:complete'),
      payload: {
        sessionId,
        skill,
        personaName,
      },
    },
  ];

  return {
    name: `project-teacher-${skill}`,
    steps,
    inputs: {
      sessionId,
      skill,
      personaName,
      baseModel,
      projectDir,
    },
  };
}

/**
 * Build the per-milestone loop steps.
 *
 * Each iteration handles one milestone: cold attempt → analysis → training → warm attempt → evaluate.
 */
function buildMilestoneLoopSteps(
  sessionId: string,
  skill: string,
  personaName: string,
  projectDir: string,
  academyConfig: ProjectTeacherPipelineConfig['config'],
  evt: (action: string) => string,
): PipelineStep[] {
  return [
    // loop.0: Read the milestone's test file
    {
      type: 'shell',
      cmd: [
        `MILESTONE_IDX={{input.iteration}}`,
        // Parse the test file path from project.json using the milestone index
        `TEST_FILE=$(cat project.json | node -e "const d=require('fs').readFileSync('/dev/stdin','utf8');const p=JSON.parse(d);console.log(p.milestones[$MILESTONE_IDX].testFile)")`,
        `cat "$TEST_FILE"`,
      ].join('\n'),
      workingDir: projectDir,
    },

    // loop.1: Emit milestone:ready with the spec and test content
    {
      type: 'emit',
      event: evt('milestone:ready'),
      payload: {
        sessionId,
        milestoneIndex: '{{input.iteration}}',
        testContent: '{{loop.0.output}}',
      },
    },

    // loop.2: Watch for student's COLD attempt (no pre-training for this milestone)
    {
      type: 'watch',
      event: evt('milestone:attempted'),
      timeoutSecs: 600,
    },

    // loop.3: LLM (agentMode) — Analyze the student's cold attempt
    // Teacher uses cloud model + tools to read actual code, examine errors, diagnose gaps
    {
      type: 'llm',
      prompt: [
        `You are an expert programming tutor analyzing a student's attempt at milestone {{input.iteration}} of a ${skill} project.`,
        '',
        'The student attempted to implement this milestone with NO pre-training (cold attempt).',
        'Examine their work and identify conceptual gaps that need targeted training.',
        '',
        '=== STUDENT\'S ATTEMPT DATA ===',
        '{{loop.2.data.payload}}',
        '',
        'Analyze:',
        '1. What did the student get right?',
        '2. What concepts are they missing or struggling with?',
        '3. What specific mistakes reveal gaps in understanding?',
        '4. What training topics would most help them succeed on a retry?',
        '',
        'If there are non-pedagogical issues (import typos, missing semicolons, etc.),',
        'note them but focus your analysis on CONCEPTUAL gaps.',
        '',
        'Output ONLY a JSON object (no markdown, no code fences):',
        '{',
        '  "correctAspects": ["what they got right"],',
        '  "weakConcepts": ["concept1", "concept2"],',
        '  "mistakes": [{"description": "...", "concept": "..."}],',
        '  "trainingTopics": ["topic for dataset synthesis"],',
        '  "feedback": "Overall feedback for the student",',
        '  "hints": ["hint1", "hint2"]',
        '}',
      ].join('\n'),
      ...(academyConfig.teacherModel && { model: academyConfig.teacherModel }),
      ...(academyConfig.teacherProvider && { provider: academyConfig.teacherProvider }),
      agentMode: true,
      maxIterations: 5,
      temperature: 0.3,
      maxTokens: 4096,
    },

    // loop.4: Synthesize training data grounded in the student's actual mistakes
    {
      type: 'command',
      command: 'genome/dataset-synthesize',
      params: {
        topic: `${skill}-milestone-{{input.iteration}}`,
        skill,
        personaName,
        exampleCount: academyConfig.examplesPerTopic,
        difficulty: 'intermediate',
        groundingContext: [
          'The student is working on a multi-milestone project and struggled with this milestone.',
          'Here is the analysis of their cold attempt:',
          '{{loop.3.output}}',
          '',
          'Generate training examples that teach the SPECIFIC concepts the student is missing.',
          'Focus on the weak areas identified in the analysis.',
        ].join('\n'),
        ...(academyConfig.teacherModel && { model: academyConfig.teacherModel }),
        ...(academyConfig.teacherProvider && { provider: academyConfig.teacherProvider }),
      },
    },

    // loop.5: Emit dataset:ready for student to train
    {
      type: 'emit',
      event: evt('dataset:ready'),
      payload: {
        sessionId,
        datasetPath: '{{loop.4.data.datasetPath}}',
        topicIndex: '{{input.iteration}}',
        topicName: `${skill}-milestone-{{input.iteration}}`,
        exampleCount: '{{loop.4.data.exampleCount}}',
      },
    },

    // loop.6: Watch for student to finish training
    {
      type: 'watch',
      event: evt('training:complete'),
      timeoutSecs: 600,
    },

    // loop.7: Emit milestone:retry with feedback and hints from analysis
    {
      type: 'emit',
      event: evt('milestone:retry'),
      payload: {
        sessionId,
        milestoneIndex: '{{input.iteration}}',
        round: 0,
        feedback: '{{loop.3.output.feedback}}',
        hints: '{{loop.3.output.hints}}',
        weakConcepts: '{{loop.3.output.weakConcepts}}',
      },
    },

    // loop.8: Watch for student's WARM attempt (with trained adapter + feedback)
    {
      type: 'watch',
      event: evt('milestone:attempted'),
      timeoutSecs: 600,
    },

    // loop.9: LLM — Evaluate the warm attempt's test output
    {
      type: 'llm',
      prompt: [
        'You are evaluating a student\'s WARM attempt at a project milestone after targeted training.',
        `The passing score threshold is ${academyConfig.passingScore}%.`,
        '',
        '=== STUDENT\'S WARM ATTEMPT DATA ===',
        '{{loop.8.data.payload}}',
        '',
        'Focus on the test output to determine pass/fail:',
        '- Look for a summary line like "Results: X passed, Y failed"',
        '- Count passed vs failed tests',
        '- Calculate score as (passed / total) * 100',
        '',
        `A score of ${academyConfig.passingScore} or higher means the student passed this milestone.`,
        '',
        'Output ONLY a JSON object (no markdown, no code fences):',
        '{',
        '  "totalTests": <number>,',
        '  "testsPassed": <number>,',
        '  "testsFailed": <number>,',
        '  "score": <0-100>,',
        '  "passed": <true/false>,',
        '  "feedback": "What the student did well and what still needs work"',
        '}',
      ].join('\n'),
      ...(academyConfig.teacherModel && { model: academyConfig.teacherModel }),
      ...(academyConfig.teacherProvider && { provider: academyConfig.teacherProvider }),
      temperature: 0.2,
      maxTokens: 2048,
    },

    // loop.10: Condition — did the student pass this milestone?
    {
      type: 'condition',
      if: '{{loop.9.output.passed}}',
      then: [
        {
          type: 'emit',
          event: evt('milestone:passed'),
          payload: {
            sessionId,
            milestoneIndex: '{{input.iteration}}',
            round: 0,
            score: '{{loop.9.output.score}}',
            attemptType: 'warm',
          },
        },
      ],
      else: [
        // For v1, emit session:failed on milestone failure.
        // Future: inner retry loop with more training rounds.
        {
          type: 'emit',
          event: evt('session:failed'),
          payload: {
            sessionId,
            milestoneIndex: '{{input.iteration}}',
            score: '{{loop.9.output.score}}',
            feedback: '{{loop.9.output.feedback}}',
          },
        },
      ],
    },
  ];
}
