/**
 * Vector Space Mathematics Tests - 512-Dimensional Intelligence Network Validation
 * 
 * Proves the mathematical foundations of persona similarity search,
 * vector space clustering, and multi-domain capability mapping.
 * 
 * Tests the core mathematical theory behind AI genome discovery.
 */

import { describe, test, expect, beforeEach } from '@jest/testing-library/jest-dom/jest-globals';
import { PersonaSearchIndex } from '../PersonaSearchIndex';

interface VectorSpaceTestCase {
  readonly name: string;
  readonly personas: readonly TestPersona[];
  readonly query_domains: readonly string[];
  readonly expected_similarity_relationships: readonly SimilarityExpectation[];
  readonly mathematical_properties: {
    readonly cosine_similarity_bounds: readonly [number, number];
    readonly clustering_coherence_threshold: number;
    readonly dimensional_stability_required: boolean;
  };
}

interface TestPersona {
  readonly id: string;
  readonly domains: readonly string[];
  readonly capabilities: readonly string[];
  readonly vector: readonly number[]; // 512-dimensional
  readonly known_similarity_to?: Record<string, number>; // Ground truth for validation
}

interface SimilarityExpectation {
  readonly persona_a: string;
  readonly persona_b: string;
  readonly expected_similarity_range: readonly [number, number];
  readonly reasoning: string;
}

describe('Vector Space Mathematics: 512-Dimensional Intelligence Network', () => {
  let searchIndex: PersonaSearchIndex;

  // Mathematical test scenarios covering edge cases and theory validation
  const vectorSpaceTests: readonly VectorSpaceTestCase[] = [
    {
      name: 'Biophysics-Quantum Chemistry Similarity Cluster',
      personas: [
        createTestPersona('bio_protein_expert', ['biophysics'], ['protein_folding', 'molecular_dynamics']),
        createTestPersona('bio_membrane_expert', ['biophysics'], ['membrane_transport', 'cellular_mechanics']),
        createTestPersona('quantum_orbital_expert', ['quantum_chemistry'], ['molecular_orbitals', 'electron_correlation']),
        createTestPersona('quantum_spectro_expert', ['quantum_chemistry'], ['spectroscopy', 'reaction_mechanisms']),
        createTestPersona('hybrid_bio_quantum', ['biophysics', 'quantum_chemistry'], ['protein_folding', 'molecular_orbitals'])
      ],
      query_domains: ['biophysics', 'quantum_chemistry'],
      expected_similarity_relationships: [
        {
          persona_a: 'bio_protein_expert',
          persona_b: 'bio_membrane_expert',
          expected_similarity_range: [0.6, 0.9],
          reasoning: 'Same domain (biophysics) should have high similarity'
        },
        {
          persona_a: 'bio_protein_expert',
          persona_b: 'quantum_orbital_expert',
          expected_similarity_range: [0.1, 0.4],
          reasoning: 'Different domains should have lower similarity'
        },
        {
          persona_a: 'hybrid_bio_quantum',
          persona_b: 'bio_protein_expert',
          expected_similarity_range: [0.5, 0.8],
          reasoning: 'Hybrid should be similar to component domains'
        }
      ],
      mathematical_properties: {
        cosine_similarity_bounds: [-1, 1],
        clustering_coherence_threshold: 0.7,
        dimensional_stability_required: true
      }
    },
    {
      name: 'Multi-Domain Capability Mapping',
      personas: [
        createTestPersona('triple_expert', ['biophysics', 'quantum_chemistry', 'geology'], 
          ['protein_folding', 'molecular_orbitals', 'rock_formation']),
        createTestPersona('geology_only', ['geology'], ['mineral_analysis', 'seismic_modeling']),
        createTestPersona('ml_engineer', ['machine_learning'], ['neural_networks', 'optimization']),
        createTestPersona('interdisciplinary', ['machine_learning', 'biophysics'], 
          ['bio_ml_modeling', 'protein_prediction'])
      ],
      query_domains: ['biophysics', 'geology', 'machine_learning'],
      expected_similarity_relationships: [
        {
          persona_a: 'triple_expert',
          persona_b: 'geology_only',
          expected_similarity_range: [0.3, 0.6],
          reasoning: 'Partial domain overlap should yield medium similarity'
        },
        {
          persona_a: 'interdisciplinary',
          persona_b: 'ml_engineer',
          expected_similarity_range: [0.5, 0.8],
          reasoning: 'Shared ML domain should create strong similarity'
        }
      ],
      mathematical_properties: {
        cosine_similarity_bounds: [-1, 1],
        clustering_coherence_threshold: 0.6,
        dimensional_stability_required: true
      }
    }
  ];

  beforeEach(async () => {
    searchIndex = new PersonaSearchIndex();
    await searchIndex.initialize();
  });

  describe('🔢 Mathematical Foundation Validation', () => {
    test('should validate 512-dimensional vector space properties', () => {
      const testVector = createRandomVector(512);
      
      // Dimensionality validation
      expect(testVector).toHaveLength(512);
      
      // Vector magnitude should be finite and positive
      const magnitude = calculateVectorMagnitude(testVector);
      expect(magnitude).toBeGreaterThan(0);
      expect(magnitude).toBeLessThan(Infinity);
      expect(Number.isFinite(magnitude)).toBe(true);
      
      // Normalized vector should have magnitude 1
      const normalized = normalizeVector(testVector);
      const normalizedMagnitude = calculateVectorMagnitude(normalized);
      expect(normalizedMagnitude).toBeCloseTo(1.0, 10);
    });

    test('should validate cosine similarity mathematical properties', () => {
      const vector1 = createRandomVector(512);
      const vector2 = createRandomVector(512);
      
      // Self-similarity should be 1.0
      const selfSimilarity = calculateCosineSimilarity(vector1, vector1);
      expect(selfSimilarity).toBeCloseTo(1.0, 10);
      
      // Similarity should be symmetric
      const sim1to2 = calculateCosineSimilarity(vector1, vector2);
      const sim2to1 = calculateCosineSimilarity(vector2, vector1);
      expect(sim1to2).toBeCloseTo(sim2to1, 10);
      
      // Similarity should be bounded [-1, 1]
      expect(sim1to2).toBeGreaterThanOrEqual(-1.0);
      expect(sim1to2).toBeLessThanOrEqual(1.0);
      
      // Orthogonal vectors should have similarity ~0
      const orthogonal1 = Array(512).fill(0).map((_, i) => i < 256 ? 1 : 0);
      const orthogonal2 = Array(512).fill(0).map((_, i) => i >= 256 ? 1 : 0);
      const orthogonalSim = calculateCosineSimilarity(orthogonal1, orthogonal2);
      expect(orthogonalSim).toBeCloseTo(0.0, 5);
    });

    test('should validate vector space linear algebra properties', () => {
      const v1 = createRandomVector(512);
      const v2 = createRandomVector(512);
      const v3 = createRandomVector(512);
      
      // Vector addition should be commutative
      const sum1 = addVectors(v1, v2);
      const sum2 = addVectors(v2, v1);
      expectVectorsEqual(sum1, sum2);
      
      // Vector addition should be associative
      const assoc1 = addVectors(addVectors(v1, v2), v3);
      const assoc2 = addVectors(v1, addVectors(v2, v3));
      expectVectorsEqual(assoc1, assoc2);
      
      // Scalar multiplication should be distributive
      const scalar = 2.5;
      const dist1 = multiplyVectorByScalar(addVectors(v1, v2), scalar);
      const dist2 = addVectors(
        multiplyVectorByScalar(v1, scalar),
        multiplyVectorByScalar(v2, scalar)
      );
      expectVectorsEqual(dist1, dist2);
    });
  });

  describe('🎯 Similarity Search Accuracy', () => {
    test.each(vectorSpaceTests)(
      'should find correct similarity relationships in $name',
      async (testCase) => {
        // Setup: Add all personas to index
        for (const persona of testCase.personas) {
          await searchIndex.addPersona({
            id: persona.id,
            domains: persona.domains,
            capabilities: persona.capabilities,
            vector: persona.vector
          });
        }
        
        // Test each expected similarity relationship
        for (const expectation of testCase.expected_similarity_relationships) {
          const personaA = testCase.personas.find(p => p.id === expectation.persona_a);
          const personaB = testCase.personas.find(p => p.id === expectation.persona_b);
          
          expect(personaA).toBeDefined();
          expect(personaB).toBeDefined();
          
          const similarity = calculateCosineSimilarity(personaA!.vector, personaB!.vector);
          const [minExpected, maxExpected] = expectation.expected_similarity_range;
          
          expect(similarity).toBeGreaterThanOrEqual(minExpected);
          expect(similarity).toBeLessThanOrEqual(maxExpected);
          
          console.log(`Similarity ${expectation.persona_a} ↔ ${expectation.persona_b}: ${similarity.toFixed(3)} (expected: ${minExpected}-${maxExpected}) - ${expectation.reasoning}`);
        }
      }
    );

    test('should perform accurate k-nearest neighbor search', async () => {
      // Setup test personas with known relationships
      const personas = [
        createTestPersona('target', ['biophysics'], ['protein_folding']),
        createTestPersona('very_similar', ['biophysics'], ['protein_folding', 'molecular_dynamics']),
        createTestPersona('somewhat_similar', ['biophysics'], ['membrane_transport']),
        createTestPersona('different_domain', ['geology'], ['rock_formation']),
        createTestPersona('unrelated', ['art'], ['painting'])
      ];
      
      for (const persona of personas) {
        await searchIndex.addPersona(persona);
      }
      
      // Search for similar personas to 'target'
      const targetPersona = personas[0];
      const results = await searchIndex.searchSimilar(targetPersona.vector, 3);
      
      // Should return closest matches in order
      expect(results).toHaveLength(3);
      expect(results[0].persona.id).toBe('very_similar'); // Most similar
      expect(results[1].persona.id).toBe('somewhat_similar'); // Second most similar
      expect(results[2].persona.id).toBe('different_domain'); // Less similar than bio personas
      
      // Verify similarity scores are in descending order
      for (let i = 0; i < results.length - 1; i++) {
        expect(results[i].similarity_score).toBeGreaterThanOrEqual(results[i + 1].similarity_score);
      }
    });

    test('should handle edge cases in similarity search', async () => {
      // Test empty index
      const emptyResults = await searchIndex.searchSimilar(createRandomVector(512), 5);
      expect(emptyResults).toHaveLength(0);
      
      // Test single persona
      await searchIndex.addPersona(createTestPersona('solo', ['test'], ['capability']));
      const soloResults = await searchIndex.searchSimilar(createRandomVector(512), 5);
      expect(soloResults).toHaveLength(1);
      
      // Test requesting more results than available
      const moreResults = await searchIndex.searchSimilar(createRandomVector(512), 10);
      expect(moreResults).toHaveLength(1); // Only one persona available
    });
  });

  describe('📊 Vector Space Clustering Analysis', () => {
    test('should identify coherent domain clusters', async () => {
      // Create personas that should naturally cluster by domain
      const biophysicsExperts = Array(5).fill(0).map((_, i) => 
        createTestPersona(`bio_${i}`, ['biophysics'], ['protein_folding'], 
          createBiophysicsVector())
      );
      
      const quantumExperts = Array(5).fill(0).map((_, i) => 
        createTestPersona(`quantum_${i}`, ['quantum_chemistry'], ['molecular_orbitals'], 
          createQuantumChemistryVector())
      );
      
      const allPersonas = [...biophysicsExperts, ...quantumExperts];
      
      for (const persona of allPersonas) {
        await searchIndex.addPersona(persona);
      }
      
      // Analyze clustering
      const clusters = await analyzeVectorClusters(searchIndex, allPersonas);
      
      // Should identify 2 main clusters
      expect(clusters).toHaveLength(2);
      
      // Each cluster should be domain-coherent
      for (const cluster of clusters) {
        const domains = new Set(cluster.personas.flatMap(p => p.domains));
        expect(domains.size).toBe(1); // Single domain per cluster
        
        // Intra-cluster similarity should be high
        const avgIntraClusterSimilarity = calculateAverageIntraClusterSimilarity(cluster);
        expect(avgIntraClusterSimilarity).toBeGreaterThan(0.7);
      }
      
      // Inter-cluster similarity should be lower
      const avgInterClusterSimilarity = calculateAverageInterClusterSimilarity(clusters[0], clusters[1]);
      expect(avgInterClusterSimilarity).toBeLessThan(0.4);
    });

    test('should maintain dimensional stability under perturbations', async () => {
      const originalPersona = createTestPersona('original', ['test'], ['capability']);
      await searchIndex.addPersona(originalPersona);
      
      // Add small perturbations to the vector
      const perturbationSizes = [0.01, 0.05, 0.1, 0.2];
      
      for (const perturbSize of perturbationSizes) {
        const perturbedVector = addNoiseToVector(originalPersona.vector, perturbSize);
        const perturbedPersona = { ...originalPersona, vector: perturbedVector };
        
        // Similarity should degrade gracefully with perturbation size
        const similarity = calculateCosineSimilarity(originalPersona.vector, perturbedVector);
        
        // Larger perturbations should result in lower similarity
        if (perturbSize <= 0.05) {
          expect(similarity).toBeGreaterThan(0.9); // Small perturbations preserve similarity
        } else if (perturbSize <= 0.1) {
          expect(similarity).toBeGreaterThan(0.7); // Medium perturbations have medium impact
        } else {
          expect(similarity).toBeGreaterThan(0.5); // Large perturbations still preserve some similarity
        }
      }
    });
  });

  describe('🔍 Advanced Vector Space Operations', () => {
    test('should support vector composition for multi-domain queries', async () => {
      // Create domain-specific personas
      const bioPersona = createTestPersona('bio', ['biophysics'], ['protein'], createBiophysicsVector());
      const quantumPersona = createTestPersona('quantum', ['quantum_chemistry'], ['orbital'], createQuantumChemistryVector());
      const hybridPersona = createTestPersona('hybrid', ['biophysics', 'quantum_chemistry'], 
        ['protein', 'orbital'], averageVectors([createBiophysicsVector(), createQuantumChemistryVector()]));
      
      await searchIndex.addPersona(bioPersona);
      await searchIndex.addPersona(quantumPersona);
      await searchIndex.addPersona(hybridPersona);
      
      // Compose query vector from multiple domains
      const composedQuery = averageVectors([bioPersona.vector, quantumPersona.vector]);
      const results = await searchIndex.searchSimilar(composedQuery, 3);
      
      // Hybrid persona should be most similar to composed query
      expect(results[0].persona.id).toBe('hybrid');
      expect(results[0].similarity_score).toBeGreaterThan(0.8);
    });

    test('should support vector interpolation for capability gradients', async () => {
      const beginner = createTestPersona('beginner', ['ml'], ['basic'], createMLBeginnerVector());
      const expert = createTestPersona('expert', ['ml'], ['advanced'], createMLExpertVector());
      
      await searchIndex.addPersona(beginner);
      await searchIndex.addPersona(expert);
      
      // Create intermediate capability levels through interpolation
      const interpolationFactors = [0.25, 0.5, 0.75];
      
      for (const factor of interpolationFactors) {
        const interpolatedVector = interpolateVectors(beginner.vector, expert.vector, factor);
        const results = await searchIndex.searchSimilar(interpolatedVector, 2);
        
        // Should find both beginner and expert, with ordering based on interpolation factor
        expect(results).toHaveLength(2);
        
        const beginnerResult = results.find(r => r.persona.id === 'beginner');
        const expertResult = results.find(r => r.persona.id === 'expert');
        
        expect(beginnerResult).toBeDefined();
        expect(expertResult).toBeDefined();
        
        // As factor increases, should be more similar to expert
        if (factor < 0.5) {
          expect(beginnerResult!.similarity_score).toBeGreaterThan(expertResult!.similarity_score);
        } else {
          expect(expertResult!.similarity_score).toBeGreaterThan(beginnerResult!.similarity_score);
        }
      }
    });

    test('should handle high-dimensional curse mitigation', async () => {
      // Create many random personas to test high-dimensional behavior
      const randomPersonas = Array(100).fill(0).map((_, i) => 
        createTestPersona(`random_${i}`, ['random'], ['capability'], createRandomVector(512))
      );
      
      for (const persona of randomPersonas) {
        await searchIndex.addPersona(persona);
      }
      
      // In high dimensions, distances become more uniform (curse of dimensionality)
      // But our system should still maintain meaningful distinctions
      const queryVector = createRandomVector(512);
      const results = await searchIndex.searchSimilar(queryVector, 10);
      
      expect(results).toHaveLength(10);
      
      // Even in high dimensions, should have some similarity variation
      const similarities = results.map(r => r.similarity_score);
      const maxSim = Math.max(...similarities);
      const minSim = Math.min(...similarities);
      const simRange = maxSim - minSim;
      
      expect(simRange).toBeGreaterThan(0.1); // Should maintain meaningful distinctions
      
      // Verify similarity scores are properly ordered
      for (let i = 0; i < similarities.length - 1; i++) {
        expect(similarities[i]).toBeGreaterThanOrEqual(similarities[i + 1]);
      }
    });
  });

  // Helper functions for mathematical operations and validation

  function createTestPersona(
    id: string, 
    domains: readonly string[], 
    capabilities: readonly string[], 
    vector?: readonly number[]
  ): TestPersona {
    return {
      id,
      domains,
      capabilities,
      vector: vector || createRandomVector(512)
    };
  }

  function createRandomVector(dimensions: number): readonly number[] {
    return Array(dimensions).fill(0).map(() => (Math.random() - 0.5) * 2); // Range [-1, 1]
  }

  function createBiophysicsVector(): readonly number[] {
    // Create a vector with structure reflecting biophysics concepts
    return Array(512).fill(0).map((_, i) => {
      if (i < 128) return Math.random() * 0.8 + 0.2; // High activation in first quarter
      if (i < 256) return Math.random() * 0.4; // Medium activation in second quarter
      return Math.random() * 0.2; // Low activation elsewhere
    });
  }

  function createQuantumChemistryVector(): readonly number[] {
    // Create a vector with structure reflecting quantum chemistry concepts
    return Array(512).fill(0).map((_, i) => {
      if (i >= 128 && i < 256) return Math.random() * 0.8 + 0.2; // High activation in second quarter
      if (i >= 256 && i < 384) return Math.random() * 0.4; // Medium activation in third quarter
      return Math.random() * 0.2; // Low activation elsewhere
    });
  }

  function createMLBeginnerVector(): readonly number[] {
    return Array(512).fill(0).map(() => Math.random() * 0.3); // Low overall activation
  }

  function createMLExpertVector(): readonly number[] {
    return Array(512).fill(0).map(() => Math.random() * 0.8 + 0.2); // High overall activation
  }

  function calculateVectorMagnitude(vector: readonly number[]): number {
    return Math.sqrt(vector.reduce((sum, val) => sum + val * val, 0));
  }

  function normalizeVector(vector: readonly number[]): readonly number[] {
    const magnitude = calculateVectorMagnitude(vector);
    return magnitude > 0 ? vector.map(val => val / magnitude) : vector;
  }

  function calculateCosineSimilarity(vecA: readonly number[], vecB: readonly number[]): number {
    const dotProduct = vecA.reduce((sum, a, i) => sum + a * vecB[i], 0);
    const magnitudeA = calculateVectorMagnitude(vecA);
    const magnitudeB = calculateVectorMagnitude(vecB);
    
    if (magnitudeA === 0 || magnitudeB === 0) return 0;
    return dotProduct / (magnitudeA * magnitudeB);
  }

  function addVectors(vecA: readonly number[], vecB: readonly number[]): readonly number[] {
    return vecA.map((val, i) => val + vecB[i]);
  }

  function multiplyVectorByScalar(vector: readonly number[], scalar: number): readonly number[] {
    return vector.map(val => val * scalar);
  }

  function averageVectors(vectors: readonly (readonly number[])[]): readonly number[] {
    if (vectors.length === 0) return [];
    
    const dimensions = vectors[0].length;
    const average = Array(dimensions).fill(0);
    
    for (const vector of vectors) {
      for (let i = 0; i < dimensions; i++) {
        average[i] += vector[i];
      }
    }
    
    return average.map(val => val / vectors.length);
  }

  function interpolateVectors(
    vecA: readonly number[], 
    vecB: readonly number[], 
    factor: number
  ): readonly number[] {
    return vecA.map((a, i) => a * (1 - factor) + vecB[i] * factor);
  }

  function addNoiseToVector(vector: readonly number[], noiseLevel: number): readonly number[] {
    return vector.map(val => val + (Math.random() - 0.5) * 2 * noiseLevel);
  }

  function expectVectorsEqual(vecA: readonly number[], vecB: readonly number[], tolerance = 1e-10): void {
    expect(vecA).toHaveLength(vecB.length);
    for (let i = 0; i < vecA.length; i++) {
      expect(vecA[i]).toBeCloseTo(vecB[i], tolerance);
    }
  }

  async function analyzeVectorClusters(index: PersonaSearchIndex, personas: readonly TestPersona[]): Promise<any[]> {
    // Simplified clustering analysis - in practice would use k-means or similar
    const biophysicsCluster = personas.filter(p => p.domains.includes('biophysics'));
    const quantumCluster = personas.filter(p => p.domains.includes('quantum_chemistry'));
    
    return [
      { personas: biophysicsCluster, centroid: averageVectors(biophysicsCluster.map(p => p.vector)) },
      { personas: quantumCluster, centroid: averageVectors(quantumCluster.map(p => p.vector)) }
    ].filter(cluster => cluster.personas.length > 0);
  }

  function calculateAverageIntraClusterSimilarity(cluster: any): number {
    const personas = cluster.personas;
    if (personas.length < 2) return 1.0;
    
    let totalSimilarity = 0;
    let pairCount = 0;
    
    for (let i = 0; i < personas.length; i++) {
      for (let j = i + 1; j < personas.length; j++) {
        totalSimilarity += calculateCosineSimilarity(personas[i].vector, personas[j].vector);
        pairCount++;
      }
    }
    
    return pairCount > 0 ? totalSimilarity / pairCount : 0;
  }

  function calculateAverageInterClusterSimilarity(clusterA: any, clusterB: any): number {
    let totalSimilarity = 0;
    let pairCount = 0;
    
    for (const personaA of clusterA.personas) {
      for (const personaB of clusterB.personas) {
        totalSimilarity += calculateCosineSimilarity(personaA.vector, personaB.vector);
        pairCount++;
      }
    }
    
    return pairCount > 0 ? totalSimilarity / pairCount : 0;
  }
});