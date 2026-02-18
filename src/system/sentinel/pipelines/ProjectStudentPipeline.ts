/**
 * ProjectStudentPipeline — Sentinel pipeline for multi-milestone project-based student
 *
 * The project student builds a real project across multiple milestones:
 * 1. Watch for project setup and curriculum from teacher
 * 2. For each milestone:
 *    a. COLD attempt: read current project state, LLM generates code (baseModel),
 *       write files, compile, run tests, emit rich payload
 *    b. Train LoRA on teacher-synthesized data from cold attempt analysis
 *    c. WARM attempt: retry with trained adapter + teacher feedback
 *    d. Emit warm attempt results for teacher evaluation
 * 3. Post-loop: compose all trained adapters
 *
 * Key design: LLM code-writing uses baseModel (local, LoRA-trainable) and outputs
 * structured JSON with file contents. Shell steps write files via heredoc.
 * State accumulates — milestone N builds on milestone N-1's code.
 */

import type { Pipeline, PipelineStep } from '../../../workers/continuum-core/bindings/modules/sentinel';
import type { ProjectStudentPipelineConfig } from '../../genome/shared/AcademyTypes';
import { academyEvent } from '../../genome/shared/AcademyTypes';

/**
 * Build the project student sentinel pipeline.
 *
 * Step flow:
 *   0: Watch — curriculum:ready
 *   1: Watch — project:setup:complete { workingDir }
 *   2: Loop (milestoneCount iterations):
 *     loop.0:  Watch — milestone:ready
 *     loop.1:  Shell — Read current project state (file tree + source files)
 *     loop.2:  LLM (baseModel) — COLD attempt: generate code as JSON { files: {path: content} }
 *     loop.3:  Shell — Write files from LLM output, run tsc --noEmit, run milestone tests
 *     loop.4:  Shell — Capture file tree + all source files for payload
 *     loop.5:  Emit — milestone:attempted { cold payload }
 *     loop.6:  Watch — dataset:ready (teacher synthesized training data)
 *     loop.7:  Emit — training:started
 *     loop.8:  Command — genome/train (LoRA on gap-targeted data)
 *     loop.9:  Emit — training:complete
 *     loop.10: Watch — milestone:retry (teacher feedback + hints)
 *     loop.11: LLM (baseModel) — WARM attempt: fix code using feedback
 *     loop.12: Shell — Write warm files, compile, run tests
 *     loop.13: Shell — Capture diagnostics for warm payload
 *     loop.14: Emit — milestone:attempted { warm payload }
 *   3: Command — genome/compose (merge all trained adapters)
 */
export function buildProjectStudentPipeline(config: ProjectStudentPipelineConfig): Pipeline {
  const {
    sessionId,
    personaId,
    personaName,
    baseModel,
    projectDir,
    config: academyConfig,
  } = config;

  const evt = (action: string) => academyEvent(sessionId, action as any);
  const boundary = 'STUDENT_CODE_EOF';

  const steps: PipelineStep[] = [
    // Step 0: Wait for curriculum from teacher
    {
      type: 'watch',
      event: evt('curriculum:ready'),
      timeoutSecs: 300,
    },

    // Step 1: Wait for project working directory to be ready
    {
      type: 'watch',
      event: evt('project:setup:complete'),
      timeoutSecs: 300,
    },

    // Step 2: Milestone loop
    {
      type: 'loop',
      count: config.milestones.length,
      steps: buildMilestoneStudentSteps(
        sessionId, personaId, personaName, baseModel, projectDir, academyConfig, evt, boundary,
      ),
    },

    // Step 3: Post-loop — compose all trained adapters
    {
      type: 'command',
      command: 'genome/compose',
      params: {
        personaId,
        baseModel,
        name: `${personaName}-project-${sessionId.slice(0, 8)}`,
        layers: '{{steps.2.iterations.*.8.data.layerId}}',
        strategy: 'weighted-merge',
        activate: true,
      },
    },
  ];

  return {
    name: `project-student-${personaName.toLowerCase().replace(/[^a-z0-9]+/g, '-')}`,
    steps,
    inputs: {
      sessionId,
      personaId,
      personaName,
      baseModel,
      projectDir,
    },
  };
}

/**
 * Build the per-milestone student loop steps.
 *
 * Each iteration: cold attempt → train → warm attempt.
 */
function buildMilestoneStudentSteps(
  sessionId: string,
  personaId: string,
  personaName: string,
  baseModel: string,
  projectDir: string,
  academyConfig: ProjectStudentPipelineConfig['config'],
  evt: (action: string) => string,
  boundary: string,
): PipelineStep[] {
  return [
    // loop.0: Watch for milestone spec from teacher
    {
      type: 'watch',
      event: evt('milestone:ready'),
      timeoutSecs: 300,
    },

    // loop.1: Read current project state (file tree + key source files)
    // workingDir comes from project:setup:complete event
    {
      type: 'shell',
      cmd: [
        `WORKDIR=$(echo '{{steps.1.data.payload.workingDir}}' | tr -d '\\n' | tail -1)`,
        `cd "$WORKDIR"`,
        `echo "=== FILE TREE ==="`,
        `find src -type f 2>/dev/null || echo "(no src files yet)"`,
        `echo ""`,
        `echo "=== SOURCE FILES ==="`,
        `for f in $(find src -name "*.ts" -type f 2>/dev/null); do`,
        `  echo "--- $f ---"`,
        `  cat "$f"`,
        `  echo ""`,
        `done`,
        `echo "=== PACKAGE.JSON ==="`,
        `cat package.json 2>/dev/null || echo "(no package.json)"`,
      ].join('\n'),
      timeoutSecs: 10,
    },

    // loop.2: LLM (baseModel) — COLD attempt: generate code for this milestone
    // Uses local model so LoRA training can improve this step
    {
      type: 'llm',
      prompt: [
        `You are building a project step by step. You are now working on milestone {{input.iteration}}.`,
        '',
        '=== MILESTONE INFO ===',
        '{{loop.0.data.payload}}',
        '',
        '=== CURRENT PROJECT STATE ===',
        '{{loop.1.output}}',
        '',
        'Your task: implement the requirements for this milestone.',
        'Build on the existing code — do NOT rewrite files that already work.',
        'Add new functionality as described in the milestone requirements.',
        '',
        'Output ONLY a JSON object mapping file paths to their COMPLETE contents.',
        'Include ALL files that need to exist (both new and modified):',
        '{"src/index.ts": "full file content...", "src/routes.ts": "content..."}',
        '',
        'IMPORTANT:',
        '- Output valid JSON only, no markdown, no code fences',
        '- Each file value must be the COMPLETE file content (not a diff)',
        '- Preserve existing working functionality from previous milestones',
      ].join('\n'),
      model: baseModel,
      temperature: 0.3,
      maxTokens: 8192,
    },

    // loop.3: Write files from LLM output, compile, run milestone tests
    {
      type: 'shell',
      cmd: [
        `WORKDIR=$(echo '{{steps.1.data.payload.workingDir}}' | tr -d '\\n' | tail -1)`,
        `cd "$WORKDIR"`,
        '',
        `# Write files from LLM JSON output`,
        `node -e "`,
        `const fs = require('fs');`,
        `const path = require('path');`,
        `try {`,
        `  const files = JSON.parse(process.argv[1]);`,
        `  for (const [fp, content] of Object.entries(files)) {`,
        `    const dir = path.dirname(fp);`,
        `    if (dir !== '.') fs.mkdirSync(dir, { recursive: true });`,
        `    fs.writeFileSync(fp, content);`,
        `    console.log('Wrote: ' + fp);`,
        `  }`,
        `} catch(e) { console.error('JSON parse error: ' + e.message); }`,
        `" '{{loop.2.output}}'`,
        '',
        `# Compile check`,
        `echo "=== COMPILATION ==="`,
        `npx tsc --noEmit 2>&1; true`,
        '',
        `# Run milestone tests`,
        `echo "=== TEST OUTPUT ==="`,
        `MILESTONE_IDX={{input.iteration}}`,
        `TEST_FILE=$(cat "${projectDir}/project.json" | node -e "const d=require('fs').readFileSync('/dev/stdin','utf8');const p=JSON.parse(d);console.log(p.milestones[$MILESTONE_IDX].testFile)")`,
        `npx tsx "$TEST_FILE" 2>&1; true`,
      ].join('\n'),
      timeoutSecs: 60,
    },

    // loop.4: Capture file tree + all source files for the attempt payload
    {
      type: 'shell',
      cmd: [
        `WORKDIR=$(echo '{{steps.1.data.payload.workingDir}}' | tr -d '\\n' | tail -1)`,
        `cd "$WORKDIR"`,
        `echo "=== FILE TREE ==="`,
        `find src -type f 2>/dev/null`,
        `echo ""`,
        `echo "=== SOURCE FILES ==="`,
        `for f in $(find src -name "*.ts" -type f 2>/dev/null); do`,
        `  echo "--- $f ---"`,
        `  cat "$f"`,
        `  echo ""`,
        `done`,
      ].join('\n'),
      timeoutSecs: 10,
    },

    // loop.5: Emit milestone:attempted (COLD payload)
    {
      type: 'emit',
      event: evt('milestone:attempted'),
      payload: {
        sessionId,
        personaId,
        milestoneIndex: '{{input.iteration}}',
        attemptType: 'cold',
        round: 0,
        sourceFiles: '{{loop.4.output}}',
        compilationOutput: '{{loop.3.output}}',
        testOutput: '{{loop.3.output}}',
        fileTree: '{{loop.4.output}}',
      },
    },

    // loop.6: Watch for training data from teacher
    {
      type: 'watch',
      event: evt('dataset:ready'),
      timeoutSecs: 300,
    },

    // loop.7: Emit training:started
    {
      type: 'emit',
      event: evt('training:started'),
      payload: {
        sessionId,
        personaId,
        topicIndex: '{{input.iteration}}',
        datasetPath: '{{loop.6.data.payload.datasetPath}}',
        round: '{{input.iteration}}',
      },
    },

    // loop.8: Train LoRA adapter on teacher's synthesized data
    {
      type: 'command',
      command: 'genome/train',
      params: {
        personaId,
        personaName,
        traitType: `project-${sessionId.slice(0, 8)}-milestone-{{input.iteration}}`,
        baseModel,
        datasetPath: '{{loop.6.data.payload.datasetPath}}',
        rank: academyConfig.rank,
        epochs: academyConfig.epochs,
        learningRate: academyConfig.learningRate,
        batchSize: academyConfig.batchSize,
      },
    },

    // loop.9: Emit training:complete
    {
      type: 'emit',
      event: evt('training:complete'),
      payload: {
        sessionId,
        personaId,
        topicIndex: '{{input.iteration}}',
        layerId: '{{loop.8.data.layerId}}',
        metrics: {
          finalLoss: '{{loop.8.data.metrics.finalLoss}}',
          trainingTime: '{{loop.8.data.metrics.trainingTime}}',
          examplesProcessed: '{{loop.8.data.metrics.examplesProcessed}}',
          epochs: '{{loop.8.data.metrics.epochs}}',
        },
      },
    },

    // loop.10: Watch for teacher's retry signal with feedback
    {
      type: 'watch',
      event: evt('milestone:retry'),
      timeoutSecs: 300,
    },

    // loop.11: LLM (baseModel) — WARM attempt: fix code using feedback + training
    {
      type: 'llm',
      prompt: [
        `You are building a project step by step. Milestone {{input.iteration}} — RETRY with feedback.`,
        '',
        '=== TEACHER FEEDBACK ===',
        '{{loop.10.data.payload}}',
        '',
        '=== YOUR PREVIOUS ATTEMPT (current state) ===',
        '{{loop.4.output}}',
        '',
        '=== PREVIOUS TEST OUTPUT ===',
        '{{loop.3.output}}',
        '',
        '=== MILESTONE INFO ===',
        '{{loop.0.data.payload}}',
        '',
        'Fix the issues identified in the feedback.',
        'Use the hints provided to guide your implementation.',
        '',
        'Output ONLY a JSON object mapping file paths to their COMPLETE contents.',
        'Include ALL files that need to exist:',
        '{"src/index.ts": "full file content..."}',
        '',
        'IMPORTANT: Valid JSON only, no markdown, no code fences.',
      ].join('\n'),
      model: baseModel,
      temperature: 0.3,
      maxTokens: 8192,
    },

    // loop.12: Write warm attempt files, compile, run tests
    {
      type: 'shell',
      cmd: [
        `WORKDIR=$(echo '{{steps.1.data.payload.workingDir}}' | tr -d '\\n' | tail -1)`,
        `cd "$WORKDIR"`,
        '',
        `# Write files from LLM JSON output`,
        `node -e "`,
        `const fs = require('fs');`,
        `const path = require('path');`,
        `try {`,
        `  const files = JSON.parse(process.argv[1]);`,
        `  for (const [fp, content] of Object.entries(files)) {`,
        `    const dir = path.dirname(fp);`,
        `    if (dir !== '.') fs.mkdirSync(dir, { recursive: true });`,
        `    fs.writeFileSync(fp, content);`,
        `    console.log('Wrote: ' + fp);`,
        `  }`,
        `} catch(e) { console.error('JSON parse error: ' + e.message); }`,
        `" '{{loop.11.output}}'`,
        '',
        `# Compile check`,
        `echo "=== COMPILATION ==="`,
        `npx tsc --noEmit 2>&1; true`,
        '',
        `# Run milestone tests`,
        `echo "=== TEST OUTPUT ==="`,
        `MILESTONE_IDX={{input.iteration}}`,
        `TEST_FILE=$(cat "${projectDir}/project.json" | node -e "const d=require('fs').readFileSync('/dev/stdin','utf8');const p=JSON.parse(d);console.log(p.milestones[$MILESTONE_IDX].testFile)")`,
        `npx tsx "$TEST_FILE" 2>&1; true`,
      ].join('\n'),
      timeoutSecs: 60,
    },

    // loop.13: Capture warm attempt diagnostics
    {
      type: 'shell',
      cmd: [
        `WORKDIR=$(echo '{{steps.1.data.payload.workingDir}}' | tr -d '\\n' | tail -1)`,
        `cd "$WORKDIR"`,
        `echo "=== FILE TREE ==="`,
        `find src -type f 2>/dev/null`,
        `echo ""`,
        `echo "=== SOURCE FILES ==="`,
        `for f in $(find src -name "*.ts" -type f 2>/dev/null); do`,
        `  echo "--- $f ---"`,
        `  cat "$f"`,
        `  echo ""`,
        `done`,
      ].join('\n'),
      timeoutSecs: 10,
    },

    // loop.14: Emit milestone:attempted (WARM payload)
    {
      type: 'emit',
      event: evt('milestone:attempted'),
      payload: {
        sessionId,
        personaId,
        milestoneIndex: '{{input.iteration}}',
        attemptType: 'warm',
        round: 0,
        sourceFiles: '{{loop.13.output}}',
        compilationOutput: '{{loop.12.output}}',
        testOutput: '{{loop.12.output}}',
        fileTree: '{{loop.13.output}}',
      },
    },
  ];
}
