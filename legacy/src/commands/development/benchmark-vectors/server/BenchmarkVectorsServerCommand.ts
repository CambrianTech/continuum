/**
 * BenchmarkVectorsServerCommand - Benchmark vector operations
 *
 * Measures current TypeScript performance to establish baseline
 * before Rust migration. Tests:
 * - Single cosine similarity
 * - Batch similarity (1 query vs N vectors)
 * - Full search with ranking (find top-K from N)
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { BenchmarkVectorsParams, BenchmarkVectorsResult, BenchmarkResult } from '../shared/BenchmarkVectorsTypes';

export class BenchmarkVectorsServerCommand extends CommandBase<BenchmarkVectorsParams, BenchmarkVectorsResult> {
  static readonly commandName = 'development/benchmark-vectors';
  readonly name = 'development/benchmark-vectors';
  readonly description = 'Benchmark vector operations (cosine similarity, batch search)';

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('development/benchmark-vectors', context, subpath, commander);
  }

  /**
   * Generate random vector for testing
   */
  private generateRandomVector(dimensions: number): number[] {
    const vec = new Array(dimensions);
    for (let i = 0; i < dimensions; i++) {
      vec[i] = Math.random() * 2 - 1; // -1 to 1
    }
    return vec;
  }

  /**
   * Current TypeScript cosine similarity (what we're benchmarking)
   */
  private cosineSimilarity(a: number[], b: number[]): number {
    let dotProduct = 0;
    let normA = 0;
    let normB = 0;

    for (let i = 0; i < a.length; i++) {
      dotProduct += a[i] * b[i];
      normA += a[i] * a[i];
      normB += b[i] * b[i];
    }

    const denominator = Math.sqrt(normA) * Math.sqrt(normB);
    return denominator === 0 ? 0 : dotProduct / denominator;
  }

  /**
   * Benchmark single similarity operation
   */
  private benchmarkSingle(
    vectors: number[][],
    iterations: number
  ): BenchmarkResult {
    const times: number[] = [];

    for (let iter = 0; iter < iterations; iter++) {
      const start = performance.now();

      // Do 1000 similarity calculations per iteration
      for (let i = 0; i < 1000; i++) {
        const idx1 = i % vectors.length;
        const idx2 = (i + 1) % vectors.length;
        this.cosineSimilarity(vectors[idx1], vectors[idx2]);
      }

      times.push(performance.now() - start);
    }

    const totalTimeMs = times.reduce((a, b) => a + b, 0);
    const avgTimeMs = totalTimeMs / iterations;
    const opsPerIteration = 1000;

    return {
      name: 'Single Cosine Similarity (1000 ops/iter)',
      iterations,
      totalTimeMs,
      avgTimeMs,
      minTimeMs: Math.min(...times),
      maxTimeMs: Math.max(...times),
      opsPerSecond: (opsPerIteration / avgTimeMs) * 1000,
      details: `${(avgTimeMs / opsPerIteration).toFixed(4)}ms per similarity`
    };
  }

  /**
   * Benchmark batch similarity (1 query vs N vectors)
   */
  private benchmarkBatch(
    vectors: number[][],
    iterations: number
  ): BenchmarkResult {
    const times: number[] = [];
    const query = vectors[0];

    for (let iter = 0; iter < iterations; iter++) {
      const start = performance.now();

      // Compare query against all vectors
      const scores: number[] = [];
      for (let i = 0; i < vectors.length; i++) {
        scores.push(this.cosineSimilarity(query, vectors[i]));
      }

      times.push(performance.now() - start);
    }

    const totalTimeMs = times.reduce((a, b) => a + b, 0);
    const avgTimeMs = totalTimeMs / iterations;

    return {
      name: `Batch Similarity (1 query vs ${vectors.length} vectors)`,
      iterations,
      totalTimeMs,
      avgTimeMs,
      minTimeMs: Math.min(...times),
      maxTimeMs: Math.max(...times),
      opsPerSecond: (vectors.length / avgTimeMs) * 1000,
      details: `${(avgTimeMs / vectors.length).toFixed(4)}ms per comparison`
    };
  }

  /**
   * Benchmark full search with ranking (find top-K)
   */
  private benchmarkSearch(
    vectors: number[][],
    iterations: number,
    topK: number = 10
  ): BenchmarkResult {
    const times: number[] = [];
    const query = vectors[0];

    for (let iter = 0; iter < iterations; iter++) {
      const start = performance.now();

      // Compare query against all vectors and get top-K
      const results: Array<{ index: number; score: number }> = [];
      for (let i = 0; i < vectors.length; i++) {
        results.push({
          index: i,
          score: this.cosineSimilarity(query, vectors[i])
        });
      }

      // Sort by score descending
      results.sort((a, b) => b.score - a.score);

      // Take top-K
      const topResults = results.slice(0, topK);

      times.push(performance.now() - start);
    }

    const totalTimeMs = times.reduce((a, b) => a + b, 0);
    const avgTimeMs = totalTimeMs / iterations;

    return {
      name: `Full Search + Rank (top-${topK} from ${vectors.length})`,
      iterations,
      totalTimeMs,
      avgTimeMs,
      minTimeMs: Math.min(...times),
      maxTimeMs: Math.max(...times),
      opsPerSecond: 1000 / avgTimeMs,
      details: `${avgTimeMs.toFixed(2)}ms per search`
    };
  }

  /**
   * Benchmark JSON parse/stringify (for embedding storage)
   */
  private benchmarkJsonParse(
    vectors: number[][],
    iterations: number
  ): BenchmarkResult {
    const times: number[] = [];

    // Pre-stringify vectors (simulating DB storage)
    const stringified = vectors.map(v => JSON.stringify(v));

    for (let iter = 0; iter < iterations; iter++) {
      const start = performance.now();

      // Parse all vectors (what happens during vector search from DB)
      for (let i = 0; i < stringified.length; i++) {
        JSON.parse(stringified[i]);
      }

      times.push(performance.now() - start);
    }

    const totalTimeMs = times.reduce((a, b) => a + b, 0);
    const avgTimeMs = totalTimeMs / iterations;

    return {
      name: `JSON.parse embeddings (${vectors.length} vectors)`,
      iterations,
      totalTimeMs,
      avgTimeMs,
      minTimeMs: Math.min(...times),
      maxTimeMs: Math.max(...times),
      opsPerSecond: (vectors.length / avgTimeMs) * 1000,
      details: `${(avgTimeMs / vectors.length).toFixed(4)}ms per parse`
    };
  }

  /**
   * Benchmark BLOB storage with Array.from copy (backward compatible)
   */
  private benchmarkBlobParse(
    vectors: number[][],
    iterations: number
  ): BenchmarkResult {
    const times: number[] = [];

    // Pre-serialize vectors to Buffer (simulating BLOB storage)
    const blobs = vectors.map(v => {
      const float32 = new Float32Array(v);
      return Buffer.from(float32.buffer);
    });

    for (let iter = 0; iter < iterations; iter++) {
      const start = performance.now();

      // Parse all vectors with Array.from (creates new array)
      for (let i = 0; i < blobs.length; i++) {
        const blob = blobs[i];
        const float32 = new Float32Array(
          blob.buffer,
          blob.byteOffset,
          blob.length / Float32Array.BYTES_PER_ELEMENT
        );
        Array.from(float32);
      }

      times.push(performance.now() - start);
    }

    const totalTimeMs = times.reduce((a, b) => a + b, 0);
    const avgTimeMs = totalTimeMs / iterations;

    return {
      name: `BLOB decode + copy (${vectors.length} vectors)`,
      iterations,
      totalTimeMs,
      avgTimeMs,
      minTimeMs: Math.min(...times),
      maxTimeMs: Math.max(...times),
      opsPerSecond: (vectors.length / avgTimeMs) * 1000,
      details: `${(avgTimeMs / vectors.length).toFixed(4)}ms per decode`
    };
  }

  /**
   * Benchmark BLOB zero-copy (Float32Array view only, no array copy)
   * This is the optimal path for vector operations
   */
  private benchmarkBlobZeroCopy(
    vectors: number[][],
    iterations: number
  ): BenchmarkResult {
    const times: number[] = [];

    // Pre-serialize vectors to Buffer
    const blobs = vectors.map(v => {
      const float32 = new Float32Array(v);
      return Buffer.from(float32.buffer);
    });

    for (let iter = 0; iter < iterations; iter++) {
      const start = performance.now();

      // Zero-copy: just create Float32Array view, no array allocation
      for (let i = 0; i < blobs.length; i++) {
        const blob = blobs[i];
        new Float32Array(
          blob.buffer,
          blob.byteOffset,
          blob.length / Float32Array.BYTES_PER_ELEMENT
        );
        // No Array.from - use Float32Array directly for math ops
      }

      times.push(performance.now() - start);
    }

    const totalTimeMs = times.reduce((a, b) => a + b, 0);
    const avgTimeMs = totalTimeMs / iterations;

    return {
      name: `BLOB zero-copy view (${vectors.length} vectors)`,
      iterations,
      totalTimeMs,
      avgTimeMs,
      minTimeMs: Math.min(...times),
      maxTimeMs: Math.max(...times),
      opsPerSecond: (vectors.length / avgTimeMs) * 1000,
      details: `${(avgTimeMs / vectors.length).toFixed(4)}ms per view`
    };
  }

  async execute(params: BenchmarkVectorsParams): Promise<BenchmarkVectorsResult> {
    const vectorCount = params.vectorCount ?? 1000;
    const dimensions = params.dimensions ?? 384;
    const iterations = params.iterations ?? 10;
    const benchmark = params.benchmark ?? 'all';

    console.log(`\n🔬 Vector Benchmark: ${vectorCount} vectors × ${dimensions} dims × ${iterations} iterations\n`);

    // Generate test vectors
    const genStart = performance.now();
    const vectors: number[][] = [];
    for (let i = 0; i < vectorCount; i++) {
      vectors.push(this.generateRandomVector(dimensions));
    }
    const genTime = performance.now() - genStart;
    console.log(`Generated ${vectorCount} vectors in ${genTime.toFixed(1)}ms\n`);

    const benchmarks: BenchmarkResult[] = [];
    const totalStart = performance.now();

    // Run benchmarks
    if (benchmark === 'all' || benchmark === 'single') {
      const result = this.benchmarkSingle(vectors, iterations);
      benchmarks.push(result);
      console.log(`✅ ${result.name}`);
      console.log(`   Avg: ${result.avgTimeMs.toFixed(2)}ms | ${result.opsPerSecond.toFixed(0)} ops/sec`);
      console.log(`   ${result.details}\n`);
    }

    if (benchmark === 'all' || benchmark === 'batch') {
      const result = this.benchmarkBatch(vectors, iterations);
      benchmarks.push(result);
      console.log(`✅ ${result.name}`);
      console.log(`   Avg: ${result.avgTimeMs.toFixed(2)}ms | ${result.opsPerSecond.toFixed(0)} ops/sec`);
      console.log(`   ${result.details}\n`);
    }

    if (benchmark === 'all' || benchmark === 'search') {
      const result = this.benchmarkSearch(vectors, iterations);
      benchmarks.push(result);
      console.log(`✅ ${result.name}`);
      console.log(`   Avg: ${result.avgTimeMs.toFixed(2)}ms | ${result.opsPerSecond.toFixed(0)} ops/sec`);
      console.log(`   ${result.details}\n`);

      // Benchmark JSON parsing (old slow method)
      const jsonResult = this.benchmarkJsonParse(vectors, iterations);
      benchmarks.push(jsonResult);
      console.log(`✅ ${jsonResult.name}`);
      console.log(`   Avg: ${jsonResult.avgTimeMs.toFixed(2)}ms | ${jsonResult.opsPerSecond.toFixed(0)} ops/sec`);
      console.log(`   ${jsonResult.details}\n`);

      // Benchmark BLOB with copy (backward compatible)
      const blobResult = this.benchmarkBlobParse(vectors, iterations);
      benchmarks.push(blobResult);
      console.log(`✅ ${blobResult.name}`);
      console.log(`   Avg: ${blobResult.avgTimeMs.toFixed(2)}ms | ${blobResult.opsPerSecond.toFixed(0)} ops/sec`);
      console.log(`   ${blobResult.details}`);
      console.log(`   🚀 Speedup: ${(jsonResult.avgTimeMs / blobResult.avgTimeMs).toFixed(1)}x faster than JSON.parse\n`);

      // Benchmark BLOB zero-copy (optimal path)
      const zeroCopyResult = this.benchmarkBlobZeroCopy(vectors, iterations);
      benchmarks.push(zeroCopyResult);
      console.log(`✅ ${zeroCopyResult.name}`);
      console.log(`   Avg: ${zeroCopyResult.avgTimeMs.toFixed(2)}ms | ${zeroCopyResult.opsPerSecond.toFixed(0)} ops/sec`);
      console.log(`   ${zeroCopyResult.details}`);
      console.log(`   🚀 Speedup: ${(jsonResult.avgTimeMs / zeroCopyResult.avgTimeMs).toFixed(1)}x faster than JSON.parse\n`);
    }

    const totalTimeMs = performance.now() - totalStart;

    // Generate recommendations
    const recommendations: string[] = [];

    const batchResult = benchmarks.find(b => b.name.includes('Batch'));
    if (batchResult && batchResult.avgTimeMs > 50) {
      recommendations.push(`Batch similarity takes ${batchResult.avgTimeMs.toFixed(0)}ms - Rust SIMD could reduce to <5ms`);
    }

    const searchResult = benchmarks.find(b => b.name.includes('Search'));
    if (searchResult && searchResult.avgTimeMs > 100) {
      recommendations.push(`Full search takes ${searchResult.avgTimeMs.toFixed(0)}ms - Rust parallel sort could reduce to <20ms`);
    }

    const jsonResult = benchmarks.find(b => b.name.includes('JSON'));
    const blobResult = benchmarks.find(b => b.name.includes('BLOB'));
    if (jsonResult && blobResult) {
      const speedup = jsonResult.avgTimeMs / blobResult.avgTimeMs;
      recommendations.push(`BLOB storage is ${speedup.toFixed(1)}x faster than JSON (${blobResult.avgTimeMs.toFixed(1)}ms vs ${jsonResult.avgTimeMs.toFixed(0)}ms)`);
    } else if (jsonResult && jsonResult.avgTimeMs > 20) {
      recommendations.push(`JSON parsing takes ${jsonResult.avgTimeMs.toFixed(0)}ms - BLOB storage reduces to <5ms`);
    }

    if (recommendations.length === 0) {
      recommendations.push('Performance is acceptable for current vector count');
    }

    console.log('📊 Recommendations:');
    recommendations.forEach(r => console.log(`   • ${r}`));

    return {
      context: params.context,
      sessionId: params.sessionId,
      success: true,
      vectorCount,
      dimensions,
      benchmarks,
      summary: {
        totalTimeMs,
        recommendations
      }
    };
  }
}

export default BenchmarkVectorsServerCommand;
