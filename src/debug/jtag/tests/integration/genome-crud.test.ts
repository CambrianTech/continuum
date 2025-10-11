/**
 * Genome Entity CRUD Test
 *
 * Tests Phase 1.1: GenomeEntity and GenomeLayerEntity registration
 *
 * Validates:
 * 1. GenomeLayerEntity CREATE/READ (with 768-dim embedding validation)
 * 2. GenomeEntity CREATE/READ (with layer references)
 * 3. Database persistence for both entities
 */

import { runJtagCommand } from '../test-utils/CRUDTestUtils';
import { writeFileSync, unlinkSync } from 'fs';
import { join } from 'path';

interface TestResult {
  operation: string;
  entity: string;
  dbPersistence: boolean;
  success: boolean;
}

// Generate 768-dimensional test embedding
function generateTestEmbedding(): number[] {
  const embedding: number[] = [];
  for (let i = 0; i < 768; i++) {
    // Simple pattern: sin wave with exponential decay
    embedding.push(Math.sin(i / 100) * Math.exp(-i / 1000));
  }
  return embedding;
}

async function testGenomeCRUD() {
  console.log('🧬 Genome Entity CRUD Test (Phase 1.1)');
  console.log('======================================\n');

  const results: TestResult[] = [];

  try {
    // 1. CREATE GenomeLayerEntity
    console.log('📝 Creating GenomeLayerEntity...');
    const layerData = {
      name: 'Test Math Expertise Layer',
      description: 'LoRA layer trained on mathematical reasoning',
      traitType: 'domain_expertise',
      source: 'trained',
      modelPath: '/models/lora/math_v1.safetensors',
      sizeMB: 42.5,
      rank: 16,
      embedding: generateTestEmbedding(),
      trainingMetadata: {
        curriculumType: 'progressive_difficulty',
        datasetSize: 5000,
        epochs: 3,
        finalLoss: 0.023,
        trainingDuration: 7200
      },
      fitness: {
        accuracy: 0.87,
        efficiency: 0.92,
        usageCount: 0,
        cacheHitRate: 0.0
      }
    };

    // Write data to temp file (command line too long with 768-dim array)
    const tmpFile = join('/tmp', `genome-layer-${Date.now()}.json`);
    writeFileSync(tmpFile, JSON.stringify(layerData));

    const createLayerResult = await runJtagCommand(`data/create --collection="genome_layers" --dataFile="${tmpFile}"`);
    unlinkSync(tmpFile); // Clean up temp file

    if (!createLayerResult?.success || !createLayerResult?.id) {
      console.log(`❌ CREATE GenomeLayerEntity failed: ${createLayerResult?.error || 'Unknown error'}`);
      throw new Error('Failed to create GenomeLayerEntity');
    }

    const layerId = createLayerResult.id;
    console.log(`✅ Created GenomeLayerEntity: ${layerId}`);

    // Verify layer persisted to database
    const dbReadLayer = await runJtagCommand(`data/read --collection="genome_layers" --id="${layerId}"`);
    const layerPersisted = Boolean(dbReadLayer?.success && dbReadLayer?.found);

    results.push({
      operation: 'CREATE',
      entity: 'genome_layers',
      dbPersistence: layerPersisted,
      success: layerPersisted
    });
    console.log(`   DB Persistence: ${layerPersisted ? '✅' : '❌'}\n`);

    // 2. CREATE GenomeEntity
    console.log('📝 Creating GenomeEntity...');
    const genomeData = {
      name: 'Math Tutor Genome v1',
      description: 'Specialized genome for mathematical tutoring',
      personaId: '00000000-0000-0000-0000-000000000001',
      baseModel: 'llama-3.1-8B',
      layers: [
        {
          layerId,
          traitType: 'domain_expertise',
          orderIndex: 0,
          weight: 1.0,
          enabled: true
        }
      ],
      compositeEmbedding: generateTestEmbedding(),
      metadata: {
        generation: 1,
        parentGenomes: [],
        trainingDuration: 7200,
        creationMethod: 'manual'
      },
      fitness: {
        overallAccuracy: 0.87,
        totalParameters: 1048576,
        totalSizeMB: 42.5,
        averageLatency: 250
      }
    };

    // Write data to temp file (command line too long with 768-dim array)
    const tmpGenomeFile = join('/tmp', `genome-${Date.now()}.json`);
    writeFileSync(tmpGenomeFile, JSON.stringify(genomeData));

    const createGenomeResult = await runJtagCommand(`data/create --collection="genomes" --dataFile="${tmpGenomeFile}"`);
    unlinkSync(tmpGenomeFile); // Clean up temp file

    if (!createGenomeResult?.success || !createGenomeResult?.id) {
      console.log(`❌ CREATE GenomeEntity failed: ${createGenomeResult?.error || 'Unknown error'}`);
      throw new Error('Failed to create GenomeEntity');
    }

    const genomeId = createGenomeResult.id;
    console.log(`✅ Created GenomeEntity: ${genomeId}`);

    // Verify genome persisted to database
    const dbReadGenome = await runJtagCommand(`data/read --collection="genomes" --id="${genomeId}"`);
    const genomePersisted = Boolean(dbReadGenome?.success && dbReadGenome?.found);

    results.push({
      operation: 'CREATE',
      entity: 'genomes',
      dbPersistence: genomePersisted,
      success: genomePersisted
    });
    console.log(`   DB Persistence: ${genomePersisted ? '✅' : '❌'}\n`);

    // 3. Verify layer data integrity
    if (dbReadLayer?.data) {
      console.log('🔍 Verifying GenomeLayerEntity data integrity...');
      const layer = dbReadLayer.data;
      const embeddingValid = Array.isArray(layer.embedding) && layer.embedding.length === 768;
      const fitnessValid = layer.fitness && typeof layer.fitness.accuracy === 'number';
      const metadataValid = layer.trainingMetadata && layer.trainingMetadata.epochs === 3;

      console.log(`   - Embedding (768-dim): ${embeddingValid ? '✅' : '❌'}`);
      console.log(`   - Fitness data: ${fitnessValid ? '✅' : '❌'}`);
      console.log(`   - Training metadata: ${metadataValid ? '✅' : '❌'}\n`);

      results.push({
        operation: 'READ',
        entity: 'genome_layers',
        dbPersistence: embeddingValid && fitnessValid && metadataValid,
        success: embeddingValid && fitnessValid && metadataValid
      });
    }

    // 4. Verify genome data integrity
    if (dbReadGenome?.data) {
      console.log('🔍 Verifying GenomeEntity data integrity...');
      const genome = dbReadGenome.data;
      const layersValid = Array.isArray(genome.layers) && genome.layers.length === 1;
      const layerRefValid = layersValid && genome.layers[0].layerId === layerId;
      const embeddingValid = Array.isArray(genome.compositeEmbedding) && genome.compositeEmbedding.length === 768;
      const metadataValid = genome.metadata && genome.metadata.generation === 1;

      console.log(`   - Layer references: ${layersValid ? '✅' : '❌'}`);
      console.log(`   - Layer ID match: ${layerRefValid ? '✅' : '❌'}`);
      console.log(`   - Composite embedding (768-dim): ${embeddingValid ? '✅' : '❌'}`);
      console.log(`   - Genome metadata: ${metadataValid ? '✅' : '❌'}\n`);

      results.push({
        operation: 'READ',
        entity: 'genomes',
        dbPersistence: layersValid && layerRefValid && embeddingValid && metadataValid,
        success: layersValid && layerRefValid && embeddingValid && metadataValid
      });
    }

    // Results Summary
    console.log('📊 Genome Entity CRUD Test Results:');
    console.log('====================================');

    const passedTests = results.filter(r => r.success).length;
    const totalTests = results.length;

    results.forEach(result => {
      const status = result.success ? '✅' : '❌';
      console.log(`${status} ${result.entity} ${result.operation}: DB(${result.dbPersistence ? '✅' : '❌'})`);
    });

    const successRate = ((passedTests / totalTests) * 100).toFixed(1);
    console.log(`\n📈 Results: ${passedTests}/${totalTests} passed (${successRate}%)`);

    if (successRate === '100.0') {
      console.log('🎉 ALL GENOME CRUD TESTS PASSED!');
      console.log('✨ Phase 1.1 Complete: Genome entities successfully registered and operational!');
      console.log('\n📋 Next Steps:');
      console.log('   Phase 1.2: Extend PersonaUser with genomeId reference');
      console.log('   Phase 2.1: AI Daemon with Worker Threads');
    } else {
      console.log('⚠️ Some tests failed - check results above');
      process.exit(1);
    }

  } catch (error) {
    console.error('❌ Test failed:', error instanceof Error ? error.message : error);
    process.exit(1);
  }
}

testGenomeCRUD().catch(error => {
  console.error('❌ Test execution failed:', error);
  process.exit(1);
});
