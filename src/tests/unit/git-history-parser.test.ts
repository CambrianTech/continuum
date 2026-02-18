#!/usr/bin/env tsx
/**
 * Git History Parser Test
 *
 * Tests the GitHistoryParser on the continuum repository
 */

import { GitHistoryParser } from '../../commands/ai/dataset/shared/parsers/GitHistoryParser';

console.log('🧪 Git History Parser Test');
console.log('==========================\n');

async function testGitHistoryParser() {
  const parser = new GitHistoryParser();

  const repoPath = process.env.REPO_PATH || process.cwd();

  console.log(`📝 Parsing last 10 commits from ${repoPath}...\n`);

  try {
    const examples = await parser.parse({
      repoPath,
      maxCommits: 10,
      minQuality: 0.5  // Only include quality commits
    });

    console.log(`✅ Successfully parsed ${examples.length} training examples\n`);

    // Show first example
    if (examples.length > 0) {
      console.log('📊 First Training Example:');
      console.log('========================\n');

      const first = examples[0];

      console.log('System Message:');
      console.log(`  ${first.messages[0].content}\n`);

      console.log('User Message:');
      console.log(`  ${first.messages[1].content}\n`);

      console.log('Assistant Message (diff):');
      const diffPreview = first.messages[2].content.slice(0, 500);
      console.log(`  ${diffPreview}...\n  [truncated, full length: ${first.messages[2].content.length} chars]\n`);

      console.log('Metadata:');
      console.log(`  Source: ${first.metadata.source}`);
      console.log(`  Timestamp: ${first.metadata.timestamp}`);
      console.log(`  Topics: ${first.metadata.topics.join(', ')}`);
      console.log(`  Quality Score: ${first.metadata.qualityScore.toFixed(2)}`);
      console.log(`  Token Count: ${first.metadata.tokenCount}`);
      console.log(`  Git Hash: ${first.metadata.git.hash}`);
      console.log(`  Files Changed: ${first.metadata.git.filesChanged}`);
      console.log(`  Lines +${first.metadata.git.linesAdded} -${first.metadata.git.linesDeleted}\n`);
    }

    // Show statistics
    console.log('📈 Statistics:');
    console.log('=============\n');

    const totalTokens = examples.reduce((sum, ex) => sum + ex.metadata.tokenCount, 0);
    const avgQuality = examples.reduce((sum, ex) => sum + ex.metadata.qualityScore, 0) / examples.length;
    const allTopics = new Set(examples.flatMap(ex => ex.metadata.topics));

    console.log(`  Total Examples: ${examples.length}`);
    console.log(`  Total Tokens: ${totalTokens.toLocaleString()}`);
    console.log(`  Avg Quality: ${avgQuality.toFixed(2)}`);
    console.log(`  Unique Topics: ${allTopics.size}`);
    console.log(`  Topics: ${Array.from(allTopics).join(', ')}\n`);

    // Quality distribution
    const high = examples.filter(ex => ex.metadata.qualityScore >= 0.8).length;
    const medium = examples.filter(ex => ex.metadata.qualityScore >= 0.5 && ex.metadata.qualityScore < 0.8).length;
    const low = examples.filter(ex => ex.metadata.qualityScore < 0.5).length;

    console.log('  Quality Distribution:');
    console.log(`    High (0.8-1.0): ${high} (${((high / examples.length) * 100).toFixed(0)}%)`);
    console.log(`    Medium (0.5-0.8): ${medium} (${((medium / examples.length) * 100).toFixed(0)}%)`);
    console.log(`    Low (0.0-0.5): ${low} (${((low / examples.length) * 100).toFixed(0)}%)\n`);

    console.log('✅ Git History Parser test passed!');
    process.exit(0);
  } catch (error) {
    console.error('❌ Test failed:', error);
    process.exit(1);
  }
}

testGitHistoryParser();
