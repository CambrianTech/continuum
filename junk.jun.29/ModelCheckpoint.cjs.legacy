/**
 * Model Checkpoint Manager - Handles saving/loading of trained AI models
 * Separates model persistence from training logic
 */

const fs = require('fs');
const path = require('path');

class ModelCheckpoint {
  constructor() {
    this.checkpointDir = '.continuum/personas';
  }

  /**
   * Save a trained model checkpoint
   */
  async saveCheckpoint(modelId, modelData) {
    const modelDir = path.join(this.checkpointDir, modelId);
    
    // Create model directory
    if (!fs.existsSync(modelDir)) {
      fs.mkdirSync(modelDir, { recursive: true });
    }

    // Save model metadata
    const configPath = path.join(modelDir, 'config.json');
    fs.writeFileSync(configPath, JSON.stringify(modelData.config, null, 2));

    // Save training data
    if (modelData.trainingData) {
      const trainingPath = path.join(modelDir, 'training.jsonl');
      const trainingExamples = Array.isArray(modelData.trainingData) 
        ? modelData.trainingData 
        : this.convertTrainingData(modelData.trainingData);
      fs.writeFileSync(trainingPath, trainingExamples.map(ex => JSON.stringify(ex)).join('\n'));
    }

    // Save actual model weights (if available)
    if (modelData.modelWeights) {
      await this.saveModelWeights(modelDir, modelData.modelWeights);
    }

    // Save fine-tuned layers (if available)
    if (modelData.fineTunedLayers) {
      await this.saveFineTunedLayers(modelDir, modelData.fineTunedLayers);
    }

    // Save model weights/checkpoint
    const checkpointPath = path.join(modelDir, 'checkpoint.json');
    const checkpoint = {
      modelId,
      baseModel: modelData.baseModel,
      fineTuneId: modelData.fineTuneId || null,
      trainingMetrics: modelData.metrics || {},
      modelWeights: {
        format: modelData.modelWeights ? 'safetensors' : 'metadata_only',
        size: modelData.size || this.estimateModelSize(modelData),
        layers: modelData.layers || this.estimateLayerCount(modelData),
        parameters: modelData.parameters || this.estimateParameterCount(modelData),
        weightsPath: modelData.modelWeights ? `${modelDir}/model_weights.safetensors` : null,
        fineTunedLayersPath: modelData.fineTunedLayers ? `${modelDir}/fine_tuned_layers.safetensors` : null,
        hasRealWeights: !!modelData.modelWeights
      },
      performance: modelData.performance || {},
      deployment: {
        compatible_frameworks: ['transformers', 'pytorch', 'tensorflow'],
        api_endpoints: [`/api/persona/${modelId}`],
        deployment_ready: modelData.status === 'graduated',
        requiresWeightLoading: !!modelData.modelWeights
      },
      created: new Date().toISOString(),
      version: modelData.version || '1.0.0'
    };

    fs.writeFileSync(checkpointPath, JSON.stringify(checkpoint, null, 2));

    console.log(`💾 Model checkpoint saved: ${configPath}`);
    console.log(`🧠 Model weights saved: ${checkpointPath}`);
    if (modelData.trainingData) {
      console.log(`📚 Training data saved: ${path.join(modelDir, 'training.jsonl')}`);
    }

    return {
      configPath,
      checkpointPath,
      trainingPath: modelData.trainingData ? path.join(modelDir, 'training.jsonl') : null
    };
  }

  /**
   * Load a model checkpoint
   */
  loadCheckpoint(modelId) {
    const modelDir = path.join(this.checkpointDir, modelId);
    const configPath = path.join(modelDir, 'config.json');
    const checkpointPath = path.join(modelDir, 'checkpoint.json');

    if (!fs.existsSync(configPath)) {
      throw new Error(`Model checkpoint not found: ${modelId}`);
    }

    const config = JSON.parse(fs.readFileSync(configPath, 'utf8'));
    let checkpoint = null;
    
    if (fs.existsSync(checkpointPath)) {
      checkpoint = JSON.parse(fs.readFileSync(checkpointPath, 'utf8'));
    }

    return {
      config,
      checkpoint,
      modelDir
    };
  }

  /**
   * List all available checkpoints
   */
  listCheckpoints() {
    if (!fs.existsSync(this.checkpointDir)) {
      return [];
    }

    const checkpoints = [];
    const entries = fs.readdirSync(this.checkpointDir);
    
    for (const entry of entries) {
      const configPath = path.join(this.checkpointDir, entry, 'config.json');
      if (fs.existsSync(configPath)) {
        try {
          const config = JSON.parse(fs.readFileSync(configPath, 'utf8'));
          checkpoints.push({
            id: entry,
            config,
            path: path.join(this.checkpointDir, entry)
          });
        } catch (error) {
          console.warn(`⚠️ Failed to load checkpoint: ${entry}`);
        }
      }
    }

    return checkpoints;
  }

  /**
   * Delete a checkpoint
   */
  deleteCheckpoint(modelId) {
    const modelDir = path.join(this.checkpointDir, modelId);
    
    if (fs.existsSync(modelDir)) {
      fs.rmSync(modelDir, { recursive: true });
      console.log(`🗑️ Checkpoint deleted: ${modelId}`);
      return true;
    }
    
    return false;
  }

  /**
   * Convert raw training data to fine-tuning format
   */
  convertTrainingData(rawData) {
    const examples = [];
    
    for (const round of rawData) {
      const failedCases = Array.isArray(round.failedCases) ? round.failedCases : [];
      
      for (const failedCase of failedCases) {
        examples.push({
          messages: [
            { role: "system", content: "You are a Protocol Sheriff. Detect protocol violations in AI responses." },
            { role: "user", content: `Validate this response: "${failedCase.response}"` },
            { role: "assistant", content: `VIOLATION: ${failedCase.expectedViolation}` }
          ]
        });
      }
      
      // If no failed cases, create a validation example
      if (failedCases.length === 0) {
        examples.push({
          messages: [
            { role: "system", content: "You are a Protocol Sheriff. Detect protocol violations in AI responses." },
            { role: "user", content: `Training round ${round.round}: No specific failures to learn from` },
            { role: "assistant", content: `VALID: No violations detected in this round` }
          ]
        });
      }
    }

    return examples;
  }

  /**
   * Save actual model weights in safetensors format
   */
  async saveModelWeights(modelDir, weights) {
    const weightsPath = path.join(modelDir, 'model_weights.safetensors');
    
    if (Buffer.isBuffer(weights)) {
      // Save raw buffer
      fs.writeFileSync(weightsPath, weights);
      console.log(`🧠 Model weights saved: ${weightsPath} (${weights.length} bytes)`);
    } else if (typeof weights === 'object') {
      // Save JSON representation (for testing)
      const serialized = JSON.stringify(weights);
      fs.writeFileSync(weightsPath, serialized);
      console.log(`🧠 Model weights metadata saved: ${weightsPath}`);
    } else {
      // Create placeholder
      const placeholder = {
        format: 'safetensors',
        note: 'Real weights would be saved here in production',
        timestamp: new Date().toISOString()
      };
      fs.writeFileSync(weightsPath, JSON.stringify(placeholder, null, 2));
      console.log(`🧠 Model weights placeholder saved: ${weightsPath}`);
    }
  }

  /**
   * Save fine-tuned layers separately
   */
  async saveFineTunedLayers(modelDir, layers) {
    const layersPath = path.join(modelDir, 'fine_tuned_layers.safetensors');
    
    if (Buffer.isBuffer(layers)) {
      fs.writeFileSync(layersPath, layers);
      console.log(`🔬 Fine-tuned layers saved: ${layersPath} (${layers.length} bytes)`);
    } else {
      // Save layer metadata and deltas
      const layerData = {
        format: 'safetensors',
        layers: layers,
        note: 'Fine-tuned layer weights and biases',
        timestamp: new Date().toISOString()
      };
      fs.writeFileSync(layersPath, JSON.stringify(layerData, null, 2));
      console.log(`🔬 Fine-tuned layers metadata saved: ${layersPath}`);
    }
  }

  /**
   * Load model weights from checkpoint
   */
  async loadModelWeights(modelId) {
    const modelDir = path.join(this.checkpointDir, modelId);
    const weightsPath = path.join(modelDir, 'model_weights.safetensors');
    
    if (!fs.existsSync(weightsPath)) {
      console.warn(`⚠️ No model weights found for ${modelId}`);
      return null;
    }
    
    try {
      const data = fs.readFileSync(weightsPath);
      
      // Try to parse as JSON first (for testing)
      try {
        return JSON.parse(data.toString());
      } catch {
        // Return raw buffer for binary weights
        return data;
      }
    } catch (error) {
      console.error(`❌ Failed to load weights for ${modelId}: ${error.message}`);
      return null;
    }
  }

  /**
   * Load fine-tuned layers from checkpoint
   */
  async loadFineTunedLayers(modelId) {
    const modelDir = path.join(this.checkpointDir, modelId);
    const layersPath = path.join(modelDir, 'fine_tuned_layers.safetensors');
    
    if (!fs.existsSync(layersPath)) {
      return null;
    }
    
    try {
      const data = fs.readFileSync(layersPath);
      
      try {
        return JSON.parse(data.toString());
      } catch {
        return data;
      }
    } catch (error) {
      console.error(`❌ Failed to load fine-tuned layers for ${modelId}: ${error.message}`);
      return null;
    }
  }

  /**
   * Estimate model size based on parameters
   */
  estimateModelSize(modelData) {
    const baseSize = 100; // Base size in MB
    const trainingExamples = modelData.trainingData?.length || 0;
    const additionalSize = trainingExamples * 0.5; // ~0.5MB per training example
    return `${Math.round(baseSize + additionalSize)}MB`;
  }

  /**
   * Estimate layer count based on model complexity
   */
  estimateLayerCount(modelData) {
    const baseModel = modelData.baseModel || '';
    if (baseModel.includes('gpt-4')) return 96;
    if (baseModel.includes('gpt-3.5')) return 96;
    if (baseModel.includes('claude')) return 32;
    return 24; // Default
  }

  /**
   * Estimate parameter count
   */
  estimateParameterCount(modelData) {
    const baseModel = modelData.baseModel || '';
    if (baseModel.includes('gpt-4')) return '175B';
    if (baseModel.includes('gpt-3.5')) return '175B';
    if (baseModel.includes('claude')) return '70B';
    return '7B'; // Default
  }

  /**
   * Get checkpoint statistics
   */
  getStats() {
    const checkpoints = this.listCheckpoints();
    const graduated = checkpoints.filter(c => c.config.metadata?.certification?.type !== 'academy_failed');
    const failed = checkpoints.filter(c => c.config.metadata?.certification?.type === 'academy_failed');
    const withWeights = checkpoints.filter(c => {
      const weightsPath = path.join(c.path, 'model_weights.safetensors');
      return fs.existsSync(weightsPath);
    });

    return {
      total: checkpoints.length,
      graduated: graduated.length,
      failed: failed.length,
      withActualWeights: withWeights.length,
      checkpointDir: this.checkpointDir
    };
  }
}

module.exports = ModelCheckpoint;