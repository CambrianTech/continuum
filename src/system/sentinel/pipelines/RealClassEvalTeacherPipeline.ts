/**
 * RealClassEvalTeacherPipeline — Sentinel pipeline for grading students on
 * real-world Python class implementations from RealClassEval (arxiv:2510.26130).
 *
 * The teacher:
 * 1. Reads the dataset manifest and eval split
 * 2. Selects N hardest examples by cyclomatic complexity
 * 3. In a loop per challenge class:
 *    a. Presents skeleton + tests to the student
 *    b. Receives the student's implementation
 *    c. Runs PYNGUIN tests deterministically
 *    d. On fail: analyzes gaps, synthesizes targeted training data, retries
 * 4. Reports aggregate Pass@1 score
 */

import type { Pipeline, PipelineStep } from '../../../workers/continuum-core/bindings/modules/sentinel';
import type { RealClassEvalTeacherPipelineConfig } from '../../genome/shared/AcademyTypes';
import { academyEvent } from '../../genome/shared/AcademyTypes';

/**
 * Build the RealClassEval teacher sentinel pipeline.
 *
 * Step flow:
 *   0: Shell — Read manifest
 *   1: Shell — Read eval split examples
 *   2: LLM — Select hardest challenges, build curriculum plan
 *   3: Emit — curriculum:ready
 *   4: Loop (per challenge class, maxTopicAttempts retries each):
 *     loop.0: Emit — challenge:ready (skeleton + tests for this class)
 *     loop.1: Watch — challenge:attempted (student's implementation)
 *     loop.2: Shell — Run tests against student code
 *     loop.3: LLM — Evaluate test output, decide pass/fail
 *     loop.4: Condition — passed?
 *       Then: [Emit topic:passed]
 *       Else: [Command genome/dataset-synthesize, Emit dataset:ready, Watch training:complete, Emit topic:remediate]
 *   5: Emit — session:complete with aggregate score
 */
export function buildRealClassEvalTeacherPipeline(config: RealClassEvalTeacherPipelineConfig): Pipeline {
  const {
    sessionId,
    skill,
    personaName,
    datasetDir,
    config: academyConfig,
  } = config;

  const evt = (action: string) => academyEvent(sessionId, action as any);

  const steps: PipelineStep[] = [
    // Step 0: Read dataset manifest
    {
      type: 'shell',
      cmd: 'cat',
      args: [`${datasetDir}/manifest.json`],
      timeoutSecs: 10,
    },

    // Step 1: Read eval split examples (these are the challenge problems)
    {
      type: 'shell',
      cmd: 'cat',
      args: [`${datasetDir}/eval.jsonl`],
      timeoutSecs: 30,
    },

    // Step 2: LLM — Select the hardest challenges and build a curriculum plan
    {
      type: 'llm',
      prompt: [
        'You are a coding instructor using the RealClassEval benchmark to test student ability.',
        '',
        '=== DATASET MANIFEST ===',
        '{{steps.0.output}}',
        '',
        '=== EVAL EXAMPLES (JSONL, one per line) ===',
        '{{steps.1.output}}',
        '',
        `Select ${academyConfig.questionsPerExam} challenging Python class implementations from the examples above.`,
        'Prefer classes with higher complexity and more test coverage.',
        '',
        'For each selected challenge, extract:',
        '1. The class skeleton (from the "user" message content, between the first code block)',
        '2. The test code (from the "user" message content, between the second code block)',
        '3. The reference implementation (from the "assistant" message content)',
        '',
        'Output ONLY a JSON object (no markdown, no code fences):',
        '{',
        '  "challenges": [',
        '    {',
        '      "index": 0,',
        '      "className": "ClassName",',
        '      "skeleton": "class Foo:\\n    def bar(self):\\n        pass",',
        '      "testCode": "def test_bar():\\n    f = Foo()\\n    assert f.bar() == 42",',
        '      "referenceImpl": "class Foo:\\n    def bar(self):\\n        return 42",',
        '      "concepts": ["concept1", "concept2"]',
        '    }',
        '  ],',
        '  "totalChallenges": <number>,',
        '  "summary": "Brief description of the challenge set"',
        '}',
      ].join('\n'),
      ...(academyConfig.teacherModel && { model: academyConfig.teacherModel }),
      ...(academyConfig.teacherProvider && { provider: academyConfig.teacherProvider }),
      temperature: 0.3,
      maxTokens: 8192,
    },

    // Step 3: Emit curriculum:ready
    {
      type: 'emit',
      event: evt('curriculum:ready'),
      payload: {
        sessionId,
        challenges: '{{steps.2.output}}',
        skill,
      },
    },

    // Step 4: Challenge loop (iterates over challenges with retry)
    {
      type: 'loop',
      until: '{{loop.3.output.passed}}',
      maxIterations: academyConfig.maxTopicAttempts,
      steps: buildChallengeSteps(sessionId, skill, personaName, academyConfig, evt),
    },

    // Step 5: Emit session:complete with aggregate score
    {
      type: 'emit',
      event: evt('session:complete'),
      payload: {
        sessionId,
        skill,
        personaName,
        finalEvaluation: '{{steps.4.output}}',
      },
    },
  ];

  return {
    name: `realclasseval-teacher-${skill}`,
    steps,
    inputs: {
      sessionId,
      skill,
      personaName,
      datasetDir,
    },
  };
}

/**
 * Build the inner challenge loop steps.
 *
 * Each iteration:
 *   0: Emit challenge:ready (skeleton + tests for current challenge)
 *   1: Watch challenge:attempted (student submits implementation)
 *   2: Shell — Write student code + run tests
 *   3: LLM — Evaluate test output, determine pass/fail
 *   4: Condition — passed?
 *      Then: [Emit topic:passed]
 *      Else: [dataset-synthesize, Emit dataset:ready, Watch training:complete, Emit topic:remediate]
 */
function buildChallengeSteps(
  sessionId: string,
  skill: string,
  personaName: string,
  academyConfig: RealClassEvalTeacherPipelineConfig['config'],
  evt: (action: string) => string,
): PipelineStep[] {
  return [
    // loop.0: Emit challenge:ready — send skeleton + tests to student
    {
      type: 'emit',
      event: evt('challenge:ready'),
      payload: {
        sessionId,
        // Reference the curriculum from step 2 (outer pipeline)
        // The current challenge is determined by iteration index
        skeleton: '{{steps.2.output.challenges[input.iteration].skeleton}}',
        testCode: '{{steps.2.output.challenges[input.iteration].testCode}}',
        className: '{{steps.2.output.challenges[input.iteration].className}}',
        challengeIndex: '{{input.iteration}}',
      },
    },

    // loop.1: Watch for student's implementation attempt
    {
      type: 'watch',
      event: evt('challenge:attempted'),
      timeoutSecs: 600,
    },

    // loop.2: Write student code to temp file + run tests
    {
      type: 'shell',
      cmd: [
        'TMPDIR=$(mktemp -d)',
        "cat << 'STUDENT_CODE_EOF' > \"$TMPDIR/solution.py\"",
        '{{loop.1.data.payload.implementation}}',
        'STUDENT_CODE_EOF',
        "cat << 'TEST_CODE_EOF' > \"$TMPDIR/test_solution.py\"",
        '{{steps.2.output.challenges[input.iteration].testCode}}',
        'TEST_CODE_EOF',
        'cd "$TMPDIR"',
        'python3 -m pytest test_solution.py -v 2>&1; true',
      ].join('\n'),
      timeoutSecs: 60,
    },

    // loop.3: LLM — Evaluate the test output
    {
      type: 'llm',
      prompt: [
        'You are evaluating a student\'s Python class implementation against the RealClassEval benchmark.',
        `The passing score threshold is ${academyConfig.passingScore}%.`,
        '',
        '=== REFERENCE IMPLEMENTATION ===',
        '{{steps.2.output.challenges[input.iteration].referenceImpl}}',
        '',
        '=== STUDENT IMPLEMENTATION ===',
        '{{loop.1.data.payload.implementation}}',
        '',
        '=== TEST OUTPUT ===',
        '{{loop.2.output}}',
        '',
        'Analyze the test output:',
        '- Count passed vs failed tests',
        '- Calculate score as (passed / total) * 100',
        `- A score >= ${academyConfig.passingScore} means the student passed`,
        '',
        'Output ONLY a JSON object (no markdown, no code fences):',
        '{',
        '  "totalTests": <number>,',
        '  "testsPassed": <number>,',
        '  "testsFailed": <number>,',
        '  "score": <0-100>,',
        '  "passed": <true/false>,',
        '  "feedback": "Explanation of results",',
        '  "weakAreas": ["area1", "area2"],',
        '  "failedTests": ["description of each failed test"]',
        '}',
      ].join('\n'),
      ...(academyConfig.teacherModel && { model: academyConfig.teacherModel }),
      ...(academyConfig.teacherProvider && { provider: academyConfig.teacherProvider }),
      temperature: 0.2,
      maxTokens: 2048,
    },

    // loop.4: Condition — did the student pass?
    {
      type: 'condition',
      if: '{{loop.3.output.passed}}',
      then: [
        {
          type: 'emit',
          event: evt('topic:passed'),
          payload: {
            sessionId,
            challengeIndex: '{{input.iteration}}',
            score: '{{loop.3.output.score}}',
          },
        },
      ],
      else: [
        // Synthesize targeted training data grounded in the failure
        {
          type: 'command',
          command: 'genome/dataset-synthesize',
          params: {
            topic: `${skill}-realclasseval-remediation`,
            skill,
            personaName,
            exampleCount: academyConfig.examplesPerTopic,
            difficulty: 'intermediate',
            groundingContext: [
              'Student failed a RealClassEval Python class implementation.',
              '',
              'Reference implementation:',
              '{{steps.2.output.challenges[input.iteration].referenceImpl}}',
              '',
              'Student attempt:',
              '{{loop.1.data.payload.implementation}}',
              '',
              'Test failures:',
              '{{loop.3.output.feedback}}',
              '',
              'Weak areas: {{loop.3.output.weakAreas}}',
            ].join('\n'),
            ...(academyConfig.teacherModel && { model: academyConfig.teacherModel }),
            ...(academyConfig.teacherProvider && { provider: academyConfig.teacherProvider }),
          },
        },
        {
          type: 'emit',
          event: evt('dataset:ready'),
          payload: {
            sessionId,
            datasetPath: '{{loop.4.else.0.data.datasetPath}}',
            challengeIndex: '{{input.iteration}}',
            exampleCount: '{{loop.4.else.0.data.exampleCount}}',
            isRemediation: true,
          },
        },
        {
          type: 'watch',
          event: evt('training:complete'),
          timeoutSecs: 600,
        },
        {
          type: 'emit',
          event: evt('topic:remediate'),
          payload: {
            sessionId,
            challengeIndex: '{{input.iteration}}',
            feedback: '{{loop.3.output.feedback}}',
            weakAreas: '{{loop.3.output.weakAreas}}',
          },
        },
      ],
    },
  ];
}
