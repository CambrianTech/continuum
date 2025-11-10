/**
 * Git History Parser
 *
 * Parses git commit history into training examples for AI fine-tuning.
 * Creates conversation-style examples: "What changes for X?" → "Here's the diff"
 */

import { spawn } from 'child_process';
import * as path from 'path';

/**
 * Training example in chat-completion format
 */
export interface TrainingExample {
  messages: {
    role: 'system' | 'user' | 'assistant';
    content: string;
  }[];
  metadata: {
    source: string;
    timestamp: string;
    topics: string[];
    qualityScore: number;
    tokenCount: number;
    [key: string]: any;  // Source-specific metadata
  };
}

/**
 * Git commit metadata
 */
export interface GitCommit {
  hash: string;
  author: string;
  email: string;
  timestamp: string;
  message: string;
  diff: string;
  filesChanged: string[];
  linesAdded: number;
  linesDeleted: number;
}

/**
 * Parse options
 */
export interface GitParseOptions {
  repoPath: string;
  since?: string;  // ISO timestamp or relative (e.g., "6 months ago")
  until?: string;
  minQuality?: number;
  includeTopics?: string[];
  excludeTopics?: string[];
  maxCommits?: number;
}

export class GitHistoryParser {
  /**
   * Parse git history into training examples
   */
  async parse(options: GitParseOptions): Promise<TrainingExample[]> {
    console.log(`📚 Parsing git history from: ${options.repoPath}`);

    // 1. Fetch commits
    const commits = await this.fetchCommits(options);
    console.log(`   Found ${commits.length} commits`);

    // 2. Convert to training examples
    const examples: TrainingExample[] = [];
    for (const commit of commits) {
      const example = this.convertCommitToTrainingExample(commit);

      // Filter by quality
      if (example.metadata.qualityScore < (options.minQuality || 0.0)) {
        continue;
      }

      // Filter by topics
      if (options.includeTopics && options.includeTopics.length > 0) {
        const hasIncludedTopic = example.metadata.topics.some(t =>
          options.includeTopics!.includes(t)
        );
        if (!hasIncludedTopic) continue;
      }

      if (options.excludeTopics && options.excludeTopics.length > 0) {
        const hasExcludedTopic = example.metadata.topics.some(t =>
          options.excludeTopics!.includes(t)
        );
        if (hasExcludedTopic) continue;
      }

      examples.push(example);
    }

    console.log(`   Generated ${examples.length} training examples`);
    return examples;
  }

  /**
   * Fetch commits from git repository
   */
  private async fetchCommits(options: GitParseOptions): Promise<GitCommit[]> {
    // Get commit metadata first (without numstat which causes parsing issues)
    const metadataArgs = [
      'log',
      '--format=%H%n%an%n%ae%n%aI%n%s%n%b%n---END-COMMIT---',
      '--no-merges'  // Skip merge commits (usually not useful for training)
    ];

    if (options.since) {
      metadataArgs.push(`--since=${options.since}`);
    }

    if (options.until) {
      metadataArgs.push(`--until=${options.until}`);
    }

    if (options.maxCommits) {
      metadataArgs.push(`-n`, String(options.maxCommits));
    }

    const output = await this.runGitCommand(options.repoPath, metadataArgs);
    return this.parseGitLogOutput(output, options.repoPath);
  }

  /**
   * Parse git log output into structured commits
   */
  private async parseGitLogOutput(output: string, repoPath: string): Promise<GitCommit[]> {
    const commits: GitCommit[] = [];
    const commitBlocks = output.split('---END-COMMIT---').filter(b => b.trim());

    for (const block of commitBlocks) {
      const lines = block.trim().split('\n');
      if (lines.length < 5) continue;

      const hash = lines[0];
      const author = lines[1];
      const email = lines[2];
      const timestamp = lines[3];
      const subject = lines[4];
      const body = lines.slice(5).join('\n').trim();

      // Get diff for this commit
      const diff = await this.getCommitDiff(repoPath, hash);

      // Parse file changes and line counts from diff
      const { filesChanged, linesAdded, linesDeleted } = this.parseDiffStats(diff);

      commits.push({
        hash,
        author,
        email,
        timestamp,
        message: subject + (body ? '\n\n' + body : ''),
        diff,
        filesChanged,
        linesAdded,
        linesDeleted
      });
    }

    return commits;
  }

  /**
   * Get full diff for a commit
   */
  private async getCommitDiff(repoPath: string, hash: string): Promise<string> {
    const args = ['show', '--format=', '--patch', hash];
    return await this.runGitCommand(repoPath, args);
  }

  /**
   * Parse diff to get file changes and line counts
   */
  private parseDiffStats(diff: string): { filesChanged: string[]; linesAdded: number; linesDeleted: number } {
    const filesChanged: string[] = [];
    let linesAdded = 0;
    let linesDeleted = 0;

    // Parse diff headers to find files changed
    const fileHeaderRegex = /^diff --git a\/(.*) b\//gm;
    let match;

    while ((match = fileHeaderRegex.exec(diff)) !== null) {
      filesChanged.push(match[1]);
    }

    // Count added/deleted lines
    const lines = diff.split('\n');
    for (const line of lines) {
      if (line.startsWith('+') && !line.startsWith('+++')) {
        linesAdded++;
      } else if (line.startsWith('-') && !line.startsWith('---')) {
        linesDeleted++;
      }
    }

    return { filesChanged, linesAdded, linesDeleted };
  }

  /**
   * Convert git commit to training example
   */
  private convertCommitToTrainingExample(commit: GitCommit): TrainingExample {
    // Extract commit message (remove conventional commit prefix if present)
    const message = commit.message.split('\n')[0];

    // Quality score based on multiple factors
    const qualityScore = this.calculateQualityScore(commit);

    // Infer topics from files changed and commit message
    const topics = this.inferTopics(commit);

    // Estimate token count (rough: 1 token ≈ 4 chars)
    const tokenCount = Math.ceil(commit.diff.length / 4);

    return {
      messages: [
        {
          role: 'system',
          content: 'You are learning from git commit history to understand code changes and development patterns.'
        },
        {
          role: 'user',
          content: `What code changes are needed to implement: ${message}`
        },
        {
          role: 'assistant',
          content: commit.diff
        }
      ],
      metadata: {
        source: 'git',
        timestamp: commit.timestamp,
        topics,
        qualityScore,
        tokenCount,
        git: {
          hash: commit.hash,
          author: commit.author,
          email: commit.email,
          filesChanged: commit.filesChanged.length,
          linesAdded: commit.linesAdded,
          linesDeleted: commit.linesDeleted
        }
      }
    };
  }

  /**
   * Calculate quality score for a commit
   */
  private calculateQualityScore(commit: GitCommit): number {
    let score = 0.5; // Base score

    // Good commit messages are higher quality
    if (commit.message.length > 50) score += 0.1;
    if (commit.message.length > 100) score += 0.1;

    // Multiple files indicate substantial changes
    if (commit.filesChanged.length > 1) score += 0.1;
    if (commit.filesChanged.length > 3) score += 0.1;

    // Reasonable diff size (too small or too large is lower quality)
    const totalLines = commit.linesAdded + commit.linesDeleted;
    if (totalLines > 10 && totalLines < 500) score += 0.1;
    if (totalLines > 50 && totalLines < 200) score += 0.1;  // Sweet spot

    // Avoid trivial commits
    if (totalLines < 5) score -= 0.2;

    // Avoid massive refactors (harder to learn from)
    if (totalLines > 1000) score -= 0.1;

    return Math.max(0.0, Math.min(1.0, score));
  }

  /**
   * Infer topics from commit data
   */
  private inferTopics(commit: GitCommit): string[] {
    const topics: string[] = [];
    const content = (commit.message + ' ' + commit.filesChanged.join(' ')).toLowerCase();

    // Programming languages
    if (content.includes('.ts')) topics.push('typescript');
    if (content.includes('.tsx')) topics.push('react');
    if (content.includes('.py')) topics.push('python');
    if (content.includes('.rs')) topics.push('rust');

    // Common patterns
    if (content.includes('test')) topics.push('testing');
    if (content.includes('fix')) topics.push('bugfix');
    if (content.includes('feat')) topics.push('feature');
    if (content.includes('refactor')) topics.push('refactoring');
    if (content.includes('perf')) topics.push('performance');
    if (content.includes('docs')) topics.push('documentation');

    // Architecture/design
    if (content.includes('entity') || content.includes('model')) topics.push('data-model');
    if (content.includes('command')) topics.push('command-pattern');
    if (content.includes('async') || content.includes('await')) topics.push('async');

    return topics;
  }

  /**
   * Run git command and return output
   */
  private async runGitCommand(repoPath: string, args: string[]): Promise<string> {
    return new Promise((resolve, reject) => {
      const git = spawn('git', args, {
        cwd: repoPath,
        stdio: 'pipe'
      });

      let stdout = '';
      let stderr = '';

      git.stdout?.on('data', (data) => {
        stdout += data.toString();
      });

      git.stderr?.on('data', (data) => {
        stderr += data.toString();
      });

      git.on('close', (code) => {
        if (code === 0) {
          resolve(stdout);
        } else {
          reject(new Error(`git command failed with code ${code}: ${stderr}`));
        }
      });

      git.on('error', reject);
    });
  }
}
