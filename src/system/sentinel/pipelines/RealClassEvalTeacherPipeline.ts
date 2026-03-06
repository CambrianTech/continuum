/**
 * RealClassEvalTeacherPipeline — Sentinel pipeline for grading students on
 * real-world Python class implementations from RealClassEval (arxiv:2510.26130).
 *
 * The teacher:
 * 1. Prepares challenges from pre-computed challenges.json (no LLM needed)
 * 2. Loops through ALL challenges:
 *    a. Presents skeleton + tests to the student
 *    b. Receives the student's implementation
 *    c. Runs PYNGUIN tests deterministically
 *    d. Parses pytest output to determine pass/fail
 *    e. On fail: synthesizes targeted training data, waits for student to train
 * 3. Reports aggregate Pass@1 score
 */

import type { Pipeline, PipelineStep } from '../../../workers/continuum-core/bindings/modules/sentinel';
import type { RealClassEvalTeacherPipelineConfig } from '../../genome/shared/AcademyTypes';
import { academyEvent } from '../../genome/shared/AcademyTypes';

/**
 * Build the RealClassEval teacher sentinel pipeline.
 *
 * Step flow:
 *   0: Shell — Prepare challenges.json (deterministic extraction, no LLM)
 *   1: Shell — Read challenges.json
 *   2: Emit — curriculum:ready (with challenge count)
 *   3: Loop (one iteration per challenge):
 *     loop.0: Emit — challenge:ready
 *     loop.1: Watch — challenge:attempted
 *     loop.2: Shell — Run tests
 *     loop.3: Shell — Parse pytest output (deterministic grading, no LLM)
 *     loop.4: Condition — passed?
 *       Then: [Emit verdict:ready (passed)]
 *       Else: [synthesize, Emit verdict:ready (with datasetPath), Watch training:complete]
 *   4: Emit — session:complete with aggregate score
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
    // Step 0: Prepare challenges.json from eval.jsonl (deterministic, no LLM)
    {
      type: 'shell',
      cmd: 'python3',
      args: [
        'scripts/prepare-realclasseval-challenges.py',
        datasetDir,
        String(academyConfig.questionsPerExam),
      ],
      timeoutSecs: 30,
    },

    // Step 1: Read the prepared challenges
    {
      type: 'shell',
      cmd: 'cat',
      args: [`${datasetDir}/challenges.json`],
      timeoutSecs: 10,
    },

    // Step 2: Emit curriculum:ready
    {
      type: 'emit',
      event: evt('curriculum:ready'),
      payload: {
        sessionId,
        totalChallenges: '{{steps.1.output.totalChallenges}}',
        skill,
      },
    },

    // Step 3: Challenge loop — one iteration per challenge
    {
      type: 'loop',
      count: academyConfig.questionsPerExam,
      steps: buildChallengeSteps(sessionId, skill, personaName, datasetDir, academyConfig, evt),
    },

    // Step 4: Emit session:complete
    {
      type: 'emit',
      event: evt('session:complete'),
      payload: {
        sessionId,
        skill,
        personaName,
        iterationsCompleted: '{{steps.3.data.iterationsCompleted}}',
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
 * Each iteration processes ONE challenge:
 *   0: Emit challenge:ready (skeleton + tests)
 *   1: Watch challenge:attempted (student submits implementation)
 *   2: Shell — Write student code + run tests
 *   3: Shell — Parse pytest output into structured result (no LLM needed)
 *   4: Condition — passed?
 *      Then: [Emit verdict:ready (passed)]
 *      Else: [synthesize, Emit verdict:ready (with datasetPath), Watch training:complete]
 */
function buildChallengeSteps(
  sessionId: string,
  skill: string,
  personaName: string,
  datasetDir: string,
  academyConfig: RealClassEvalTeacherPipelineConfig['config'],
  evt: (action: string) => string,
): PipelineStep[] {
  return [
    // loop.0: Emit challenge:ready
    {
      type: 'emit',
      event: evt('challenge:ready'),
      payload: {
        sessionId,
        skeleton: '{{steps.1.output.challenges.{{input.iteration}}.skeleton}}',
        testCode: '{{steps.1.output.challenges.{{input.iteration}}.testCode}}',
        className: '{{steps.1.output.challenges.{{input.iteration}}.className}}',
        moduleName: '{{steps.1.output.challenges.{{input.iteration}}.moduleName}}',
        challengeIndex: '{{input.iteration}}',
      },
    },

    // loop.1: Watch for student's implementation attempt
    {
      type: 'watch',
      event: evt('challenge:attempted'),
      timeoutSecs: 600,
    },

    // loop.2: Write student code to module file + run tests
    {
      type: 'shell',
      cmd: [
        'TMPDIR=$(mktemp -d)',
        'MODULE_NAME="{{steps.1.output.challenges.{{input.iteration}}.moduleName}}"',
        '[ -z "$MODULE_NAME" ] && MODULE_NAME="solution"',
        "cat << 'STUDENT_CODE_EOF' > \"$TMPDIR/${MODULE_NAME}.py\"",
        '{{loop.1.data.payload.implementation}}',
        'STUDENT_CODE_EOF',
        "cat << 'TEST_CODE_EOF' > \"$TMPDIR/test_solution.py\"",
        '{{steps.1.output.challenges.{{input.iteration}}.testCode}}',
        'TEST_CODE_EOF',
        'cd "$TMPDIR"',
        'python3 -m pytest test_solution.py -v 2>&1; true',
      ].join('\n'),
      timeoutSecs: 60,
    },

    // loop.3: Parse pytest output deterministically (no LLM needed)
    {
      type: 'shell',
      cmd: [
        "OUTPUT='{{loop.2.output}}'",
        // Count passed, failed, errors from pytest verbose output
        'PASSED=$(echo "$OUTPUT" | grep -c "PASSED" || true)',
        'FAILED=$(echo "$OUTPUT" | grep -c "FAILED" || true)',
        'XFAILED=$(echo "$OUTPUT" | grep -c "XFAIL" || true)',
        'ERRORS=$(echo "$OUTPUT" | grep -c "ERROR" || true)',
        // Total = passed + xfailed (both count as success) - xpass counts as fail
        'TOTAL=$((PASSED + FAILED + XFAILED + ERRORS))',
        '[ "$TOTAL" -eq 0 ] && TOTAL=1',  // avoid div by zero
        'SUCCEEDED=$((PASSED + XFAILED))',
        'SCORE=$(( (SUCCEEDED * 100) / TOTAL ))',
        `PASS_THRESHOLD=${academyConfig.passingScore}`,
        '[ "$SCORE" -ge "$PASS_THRESHOLD" ] && VERDICT="true" || VERDICT="false"',
        // Output structured JSON
        'cat << EOF',
        '{"totalTests":$TOTAL,"testsPassed":$SUCCEEDED,"testsFailed":$((FAILED + ERRORS)),"score":$SCORE,"passed":$VERDICT}',
        'EOF',
      ].join('\n'),
      timeoutSecs: 10,
    },

    // loop.4: Condition — did the student pass?
    {
      type: 'condition',
      if: '{{loop.3.output.passed}}',
      then: [
        // Student passed
        {
          type: 'emit',
          event: evt('verdict:ready'),
          payload: {
            sessionId,
            passed: true,
            challengeIndex: '{{input.iteration}}',
            score: '{{loop.3.output.score}}',
          },
        },
      ],
      else: [
        // Student failed — synthesize targeted training data
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
              '{{steps.1.output.challenges.{{input.iteration}}.referenceImpl}}',
              '',
              'Student attempt:',
              '{{loop.1.data.payload.implementation}}',
              '',
              'Pytest output:',
              '{{loop.2.output}}',
            ].join('\n'),
            ...(academyConfig.teacherModel && { model: academyConfig.teacherModel }),
            ...(academyConfig.teacherProvider && { provider: academyConfig.teacherProvider }),
          },
        },
        // Emit verdict with datasetPath so student knows to train
        {
          type: 'emit',
          event: evt('verdict:ready'),
          payload: {
            sessionId,
            passed: false,
            datasetPath: '{{loop.4.data.datasetPath}}',
            challengeIndex: '{{input.iteration}}',
            exampleCount: '{{loop.4.data.exampleCount}}',
          },
        },
        // Wait for student to finish training
        {
          type: 'watch',
          event: evt('training:complete'),
          timeoutSecs: 600,
        },
      ],
    },
  ];
}
