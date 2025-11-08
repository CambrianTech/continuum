#!/usr/bin/env tsx
/**
 * Generate Git Training Dataset
 *
 * Parses git history into training data JSONL file
 */

import { GitHistoryParser } from '../commands/ai/dataset/shared/parsers/GitHistoryParser';
import { createWriteStream } from 'fs';
import { promises as fs } from 'fs';
import * as path from 'path';

console.log('📚 Git Training Dataset Generator');
console.log('==================================\n');

async function generateGitDataset() {
  const parser = new GitHistoryParser();

  // Parse last 6 months of commits
  console.log('📝 Parsing continuum git history (last 6 months)...\n');

  try {
    const examples = await parser.parse({
      repoPath: '/Volumes/FlashGordon/cambrian/continuum',
      since: '6 months ago',
      minQuality: 0.5  // Only include quality commits
    });

    console.log(`✅ Successfully parsed ${examples.length} training examples\n`);

    // Create output directory
    const outputDir = '/Volumes/FlashGordon/cambrian/datasets/parsed';
    await fs.mkdir(outputDir, { recursive: true });

    // Generate filename with timestamp
    const timestamp = new Date().toISOString().replace(/[:.]/g, '-').slice(0, -5);
    const outputPath = path.join(outputDir, `continuum-git-${timestamp}.jsonl`);

    // Write JSONL file
    console.log(`📄 Writing training data to: ${outputPath}\n`);
    const writeStream = createWriteStream(outputPath);

    for (const example of examples) {
      writeStream.write(JSON.stringify(example) + '\n');
    }

    await new Promise((resolve, reject) => {
      writeStream.end();
      writeStream.on('finish', resolve);
      writeStream.on('error', reject);
    });

    // Get file size
    const stats = await fs.stat(outputPath);
    const sizeMB = (stats.size / (1024 * 1024)).toFixed(2);

    console.log('✅ Training data file created successfully!\n');

    // Calculate statistics
    console.log('📈 Dataset Statistics:');
    console.log('====================\n');

    const totalTokens = examples.reduce((sum, ex) => sum + ex.metadata.tokenCount, 0);
    const avgQuality = examples.reduce((sum, ex) => sum + ex.metadata.qualityScore, 0) / examples.length;
    const avgTokens = totalTokens / examples.length;
    const allTopics = new Set(examples.flatMap(ex => ex.metadata.topics));

    console.log(`  File Size: ${sizeMB} MB`);
    console.log(`  Total Examples: ${examples.length.toLocaleString()}`);
    console.log(`  Total Tokens: ${totalTokens.toLocaleString()}`);
    console.log(`  Avg Tokens/Example: ${Math.round(avgTokens).toLocaleString()}`);
    console.log(`  Avg Quality Score: ${avgQuality.toFixed(2)}\n`);

    // Quality distribution
    const high = examples.filter(ex => ex.metadata.qualityScore >= 0.8).length;
    const medium = examples.filter(ex => ex.metadata.qualityScore >= 0.5 && ex.metadata.qualityScore < 0.8).length;
    const low = examples.filter(ex => ex.metadata.qualityScore < 0.5).length;

    console.log('  Quality Distribution:');
    console.log(`    High (0.8-1.0): ${high.toLocaleString()} (${((high / examples.length) * 100).toFixed(0)}%)`);
    console.log(`    Medium (0.5-0.8): ${medium.toLocaleString()} (${((medium / examples.length) * 100).toFixed(0)}%)`);
    console.log(`    Low (0.0-0.5): ${low.toLocaleString()} (${((low / examples.length) * 100).toFixed(0)}%)\n`);

    console.log(`  Unique Topics: ${allTopics.size}`);
    console.log(`  Topics: ${Array.from(allTopics).sort().join(', ')}\n`);

    // Show sample commit messages
    console.log('📋 Sample Commit Messages (first 5):');
    console.log('===================================\n');

    for (let i = 0; i < Math.min(5, examples.length); i++) {
      const example = examples[i];
      const commitMsg = example.messages[1].content.replace('What code changes are needed to implement: ', '');
      const shortMsg = commitMsg.length > 80 ? commitMsg.slice(0, 77) + '...' : commitMsg;
      console.log(`  ${i + 1}. ${shortMsg}`);
      console.log(`     Quality: ${example.metadata.qualityScore.toFixed(2)}, Tokens: ${example.metadata.tokenCount.toLocaleString()}, Topics: ${example.metadata.topics.join(', ')}\n`);
    }

    console.log(`✅ Dataset ready for training!`);
    console.log(`📁 Location: ${outputPath}\n`);

    process.exit(0);
  } catch (error) {
    console.error('❌ Failed to generate dataset:', error);
    process.exit(1);
  }
}

generateGitDataset();
