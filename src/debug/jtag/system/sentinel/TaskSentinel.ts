/**
 * TaskSentinel - Recursive task execution with spawn limits
 *
 * Like recursion:
 * - Base case: Task is atomic (can be done directly)
 * - Recursive case: Task needs subtasks (spawn child Sentinels)
 * - Limit: Max depth, max children, timeout
 *
 * Example: "Create a snake game"
 * 1. TaskSentinel receives high-level goal
 * 2. Breaks into subtasks: [create HTML, create CSS, create JS, verify]
 * 3. Each subtask may spawn further (create JS → [game loop, snake class, input handler])
 * 4. Leaf tasks execute directly (write file, run build)
 * 5. Results bubble up
 */

import * as fs from 'fs';
import * as path from 'path';
import { execSync } from 'child_process';
import { BuildSentinel } from './BuildSentinel';

export interface Task {
  id: string;
  description: string;
  type: 'atomic' | 'composite';
  status: 'pending' | 'running' | 'completed' | 'failed';
  children?: Task[];
  result?: TaskResult;
  depth: number;
}

export interface TaskResult {
  success: boolean;
  output?: string;
  error?: string;
  filesCreated?: string[];
  filesModified?: string[];
  duration: number;
}

export interface TaskSentinelConfig {
  maxDepth: number;           // Recursion limit
  maxChildren: number;        // Max subtasks per task
  maxTotalTasks: number;      // Total task limit
  timeoutMs: number;          // Overall timeout
  workingDir: string;
  onProgress?: (task: Task, message: string) => void;
}

export interface TaskPlan {
  goal: string;
  tasks: TaskNode[];
}

export interface TaskNode {
  description: string;
  type: 'write' | 'build' | 'run' | 'verify' | 'composite';
  file?: string;
  content?: string;
  command?: string;
  children?: TaskNode[];
}

export class TaskSentinel {
  private config: Required<TaskSentinelConfig>;
  private taskCount = 0;
  private startTime = 0;
  private rootTask: Task | null = null;

  constructor(config: Partial<TaskSentinelConfig> & { workingDir: string }) {
    this.config = {
      maxDepth: 5,
      maxChildren: 10,
      maxTotalTasks: 50,
      timeoutMs: 300000, // 5 minutes
      onProgress: () => {},
      ...config,
    };
  }

  /**
   * Execute a task plan recursively
   */
  async execute(plan: TaskPlan): Promise<TaskResult> {
    this.startTime = Date.now();
    this.taskCount = 0;

    this.report(null, `Starting: ${plan.goal}`);

    // Create root task
    this.rootTask = {
      id: 'root',
      description: plan.goal,
      type: 'composite',
      status: 'running',
      depth: 0,
      children: [],
    };

    try {
      // Execute all top-level tasks
      const results: TaskResult[] = [];
      for (const taskNode of plan.tasks) {
        const result = await this.executeNode(taskNode, this.rootTask, 1);
        results.push(result);
        if (!result.success) {
          // Stop on first failure (could make configurable)
          this.rootTask.status = 'failed';
          return {
            success: false,
            error: `Task failed: ${taskNode.description}`,
            duration: Date.now() - this.startTime,
          };
        }
      }

      this.rootTask.status = 'completed';
      return {
        success: true,
        output: `Completed ${this.taskCount} tasks`,
        filesCreated: results.flatMap(r => r.filesCreated || []),
        filesModified: results.flatMap(r => r.filesModified || []),
        duration: Date.now() - this.startTime,
      };
    } catch (error: any) {
      this.rootTask.status = 'failed';
      return {
        success: false,
        error: error.message,
        duration: Date.now() - this.startTime,
      };
    }
  }

  /**
   * Execute a single task node (may recurse for composite tasks)
   */
  private async executeNode(node: TaskNode, parent: Task, depth: number): Promise<TaskResult> {
    // Check limits (BASE CASES for recursion)
    if (depth > this.config.maxDepth) {
      return { success: false, error: `Max depth (${this.config.maxDepth}) exceeded`, duration: 0 };
    }
    if (this.taskCount >= this.config.maxTotalTasks) {
      return { success: false, error: `Max tasks (${this.config.maxTotalTasks}) exceeded`, duration: 0 };
    }
    if (Date.now() - this.startTime > this.config.timeoutMs) {
      return { success: false, error: `Timeout (${this.config.timeoutMs}ms) exceeded`, duration: 0 };
    }

    this.taskCount++;
    const taskId = `task-${this.taskCount}`;
    const task: Task = {
      id: taskId,
      description: node.description,
      type: node.type === 'composite' ? 'composite' : 'atomic',
      status: 'running',
      depth,
      children: [],
    };
    parent.children?.push(task);

    this.report(task, `[depth=${depth}] ${node.description}`);

    const startTime = Date.now();

    try {
      let result: TaskResult;

      switch (node.type) {
        case 'write':
          result = await this.executeWrite(node);
          break;
        case 'build':
          result = await this.executeBuild(node);
          break;
        case 'run':
          result = await this.executeRun(node);
          break;
        case 'verify':
          result = await this.executeVerify(node);
          break;
        case 'composite':
          // RECURSIVE CASE: Execute children
          if (!node.children || node.children.length === 0) {
            result = { success: true, output: 'No subtasks', duration: 0 };
          } else if (node.children.length > this.config.maxChildren) {
            result = { success: false, error: `Too many children (${node.children.length} > ${this.config.maxChildren})`, duration: 0 };
          } else {
            const childResults: TaskResult[] = [];
            for (const child of node.children) {
              const childResult = await this.executeNode(child, task, depth + 1);
              childResults.push(childResult);
              if (!childResult.success) {
                result = { success: false, error: `Child failed: ${child.description}`, duration: Date.now() - startTime };
                break;
              }
            }
            if (!result!) {
              result = {
                success: true,
                output: `Completed ${node.children.length} subtasks`,
                filesCreated: childResults.flatMap(r => r.filesCreated || []),
                filesModified: childResults.flatMap(r => r.filesModified || []),
                duration: Date.now() - startTime,
              };
            }
          }
          break;
        default:
          result = { success: false, error: `Unknown task type: ${node.type}`, duration: 0 };
      }

      task.status = result.success ? 'completed' : 'failed';
      task.result = result;
      return result;
    } catch (error: any) {
      task.status = 'failed';
      const result = { success: false, error: error.message, duration: Date.now() - startTime };
      task.result = result;
      return result;
    }
  }

  /**
   * Write a file
   */
  private async executeWrite(node: TaskNode): Promise<TaskResult> {
    if (!node.file || !node.content) {
      return { success: false, error: 'Write task needs file and content', duration: 0 };
    }

    const fullPath = path.resolve(this.config.workingDir, node.file);
    const dir = path.dirname(fullPath);

    // Create directory if needed
    if (!fs.existsSync(dir)) {
      fs.mkdirSync(dir, { recursive: true });
    }

    const existed = fs.existsSync(fullPath);
    fs.writeFileSync(fullPath, node.content);

    return {
      success: true,
      output: `Wrote ${node.file}`,
      filesCreated: existed ? [] : [node.file],
      filesModified: existed ? [node.file] : [],
      duration: 0,
    };
  }

  /**
   * Run a build (delegates to BuildSentinel)
   */
  private async executeBuild(node: TaskNode): Promise<TaskResult> {
    const command = node.command || 'npm run build:ts';
    const sentinel = new BuildSentinel({
      command,
      workingDir: this.config.workingDir,
      maxAttempts: 3,
      canAutoFix: true,
    });

    const result = await sentinel.run();

    return {
      success: result.success,
      output: result.success ? 'Build succeeded' : `Build failed: ${result.escalationReason}`,
      error: result.success ? undefined : JSON.stringify(result.finalErrors),
      duration: 0,
    };
  }

  /**
   * Run a command
   */
  private async executeRun(node: TaskNode): Promise<TaskResult> {
    if (!node.command) {
      return { success: false, error: 'Run task needs command', duration: 0 };
    }

    try {
      const output = execSync(node.command, {
        cwd: this.config.workingDir,
        encoding: 'utf-8',
        timeout: 30000,
      });
      return { success: true, output, duration: 0 };
    } catch (error: any) {
      return {
        success: false,
        error: error.message,
        output: error.stdout?.toString() || error.stderr?.toString(),
        duration: 0,
      };
    }
  }

  /**
   * Verify something exists or works
   */
  private async executeVerify(node: TaskNode): Promise<TaskResult> {
    if (node.file) {
      const fullPath = path.resolve(this.config.workingDir, node.file);
      if (fs.existsSync(fullPath)) {
        return { success: true, output: `File exists: ${node.file}`, duration: 0 };
      } else {
        return { success: false, error: `File not found: ${node.file}`, duration: 0 };
      }
    }
    if (node.command) {
      return this.executeRun(node);
    }
    return { success: true, output: 'Nothing to verify', duration: 0 };
  }

  private report(task: Task | null, message: string) {
    this.config.onProgress(task!, message);
  }
}

/**
 * Helper to create a snake game plan
 */
export function createSnakeGamePlan(outputDir: string): TaskPlan {
  return {
    goal: 'Create a playable Snake game',
    tasks: [
      {
        description: 'Create game files',
        type: 'composite',
        children: [
          {
            description: 'Create HTML file',
            type: 'write',
            file: `${outputDir}/index.html`,
            content: `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Snake Game</title>
  <link rel="stylesheet" href="style.css">
</head>
<body>
  <div class="game-container">
    <h1>Snake Game</h1>
    <canvas id="gameCanvas" width="400" height="400"></canvas>
    <div class="score">Score: <span id="score">0</span></div>
    <button id="startBtn">Start Game</button>
  </div>
  <script src="game.js"></script>
</body>
</html>`,
          },
          {
            description: 'Create CSS file',
            type: 'write',
            file: `${outputDir}/style.css`,
            content: `* {
  margin: 0;
  padding: 0;
  box-sizing: border-box;
}

body {
  display: flex;
  justify-content: center;
  align-items: center;
  min-height: 100vh;
  background: #1a1a2e;
  font-family: 'Segoe UI', sans-serif;
}

.game-container {
  text-align: center;
}

h1 {
  color: #4ecca3;
  margin-bottom: 20px;
}

#gameCanvas {
  border: 2px solid #4ecca3;
  background: #16213e;
}

.score {
  color: #fff;
  font-size: 24px;
  margin: 20px 0;
}

#startBtn {
  padding: 10px 30px;
  font-size: 18px;
  background: #4ecca3;
  border: none;
  border-radius: 5px;
  cursor: pointer;
  transition: background 0.3s;
}

#startBtn:hover {
  background: #3db892;
}`,
          },
          {
            description: 'Create JavaScript game logic',
            type: 'write',
            file: `${outputDir}/game.js`,
            content: `// Snake Game - Created by TaskSentinel
const canvas = document.getElementById('gameCanvas');
const ctx = canvas.getContext('2d');
const scoreEl = document.getElementById('score');
const startBtn = document.getElementById('startBtn');

const gridSize = 20;
const tileCount = canvas.width / gridSize;

let snake = [];
let food = { x: 0, y: 0 };
let dx = 0;
let dy = 0;
let score = 0;
let gameLoop = null;

function initGame() {
  snake = [
    { x: 10, y: 10 },
    { x: 9, y: 10 },
    { x: 8, y: 10 }
  ];
  dx = 1;
  dy = 0;
  score = 0;
  scoreEl.textContent = score;
  placeFood();
}

function placeFood() {
  food.x = Math.floor(Math.random() * tileCount);
  food.y = Math.floor(Math.random() * tileCount);
  // Don't place on snake
  for (const segment of snake) {
    if (segment.x === food.x && segment.y === food.y) {
      placeFood();
      return;
    }
  }
}

function draw() {
  // Clear canvas
  ctx.fillStyle = '#16213e';
  ctx.fillRect(0, 0, canvas.width, canvas.height);

  // Draw snake
  ctx.fillStyle = '#4ecca3';
  for (const segment of snake) {
    ctx.fillRect(segment.x * gridSize, segment.y * gridSize, gridSize - 2, gridSize - 2);
  }

  // Draw food
  ctx.fillStyle = '#e94560';
  ctx.fillRect(food.x * gridSize, food.y * gridSize, gridSize - 2, gridSize - 2);
}

function update() {
  const head = { x: snake[0].x + dx, y: snake[0].y + dy };

  // Wall collision
  if (head.x < 0 || head.x >= tileCount || head.y < 0 || head.y >= tileCount) {
    gameOver();
    return;
  }

  // Self collision
  for (const segment of snake) {
    if (head.x === segment.x && head.y === segment.y) {
      gameOver();
      return;
    }
  }

  snake.unshift(head);

  // Food collision
  if (head.x === food.x && head.y === food.y) {
    score += 10;
    scoreEl.textContent = score;
    placeFood();
  } else {
    snake.pop();
  }

  draw();
}

function gameOver() {
  clearInterval(gameLoop);
  gameLoop = null;
  ctx.fillStyle = 'rgba(0, 0, 0, 0.75)';
  ctx.fillRect(0, 0, canvas.width, canvas.height);
  ctx.fillStyle = '#fff';
  ctx.font = '30px Segoe UI';
  ctx.textAlign = 'center';
  ctx.fillText('Game Over!', canvas.width / 2, canvas.height / 2);
  ctx.font = '20px Segoe UI';
  ctx.fillText('Score: ' + score, canvas.width / 2, canvas.height / 2 + 40);
  startBtn.textContent = 'Play Again';
}

function handleKeydown(e) {
  switch (e.key) {
    case 'ArrowUp':
      if (dy !== 1) { dx = 0; dy = -1; }
      break;
    case 'ArrowDown':
      if (dy !== -1) { dx = 0; dy = 1; }
      break;
    case 'ArrowLeft':
      if (dx !== 1) { dx = -1; dy = 0; }
      break;
    case 'ArrowRight':
      if (dx !== -1) { dx = 1; dy = 0; }
      break;
  }
}

function startGame() {
  if (gameLoop) return;
  initGame();
  draw();
  gameLoop = setInterval(update, 100);
  startBtn.textContent = 'Playing...';
}

document.addEventListener('keydown', handleKeydown);
startBtn.addEventListener('click', startGame);

// Initial draw
initGame();
draw();
console.log('Snake game loaded! Click Start to play.');`,
          },
        ],
      },
      {
        description: 'Verify all files created',
        type: 'composite',
        children: [
          { description: 'Verify HTML', type: 'verify', file: `${outputDir}/index.html` },
          { description: 'Verify CSS', type: 'verify', file: `${outputDir}/style.css` },
          { description: 'Verify JS', type: 'verify', file: `${outputDir}/game.js` },
        ],
      },
    ],
  };
}

// CLI test
export async function testTaskSentinel() {
  const outputDir = 'system/sentinel/olympics/snake-game';

  const sentinel = new TaskSentinel({
    workingDir: '/Volumes/FlashGordon/cambrian/continuum/src/debug/jtag',
    maxDepth: 5,
    maxChildren: 10,
    maxTotalTasks: 50,
    timeoutMs: 60000,
    onProgress: (task, message) => {
      const indent = task ? '  '.repeat(task.depth) : '';
      console.log(`${indent}${message}`);
    },
  });

  const plan = createSnakeGamePlan(outputDir);

  console.log('\n=== TaskSentinel: Create Snake Game ===\n');
  const result = await sentinel.execute(plan);

  console.log('\n=== Result ===');
  console.log(`Success: ${result.success}`);
  console.log(`Duration: ${result.duration}ms`);
  if (result.filesCreated?.length) {
    console.log(`Files created: ${result.filesCreated.join(', ')}`);
  }
  if (result.error) {
    console.log(`Error: ${result.error}`);
  }

  return result;
}
