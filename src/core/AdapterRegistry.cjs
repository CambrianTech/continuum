/**
 * Adapter Registry - Share and discover LoRA adapters
 * Makes it easy to share tiny fine-tuned adapters between teams
 */

const fs = require('fs');
const path = require('path');
const crypto = require('crypto');

class AdapterRegistry {
  constructor(registryPath = '.continuum/adapter_registry') {
    this.registryPath = registryPath;
    this.adaptersPath = path.join(registryPath, 'adapters');
    this.metadataPath = path.join(registryPath, 'metadata.json');
    
    this.ensureRegistryExists();
  }

  /**
   * Ensure registry directory structure exists
   */
  ensureRegistryExists() {
    if (!fs.existsSync(this.registryPath)) {
      fs.mkdirSync(this.registryPath, { recursive: true });
    }
    
    if (!fs.existsSync(this.adaptersPath)) {
      fs.mkdirSync(this.adaptersPath, { recursive: true });
    }
    
    if (!fs.existsSync(this.metadataPath)) {
      const initialMetadata = {
        version: '1.0.0',
        created: new Date().toISOString(),
        adapters: {},
        stats: {
          totalAdapters: 0,
          totalSize: 0,
          baseModels: []
        }
      };
      fs.writeFileSync(this.metadataPath, JSON.stringify(initialMetadata, null, 2));
    }
  }

  /**
   * Publish a LoRA adapter to the registry
   */
  async publishAdapter(adapterPath, metadata = {}) {
    if (!fs.existsSync(adapterPath)) {
      throw new Error(`Adapter not found: ${adapterPath}`);
    }

    // Load adapter data
    const adapterData = JSON.parse(fs.readFileSync(adapterPath, 'utf8'));
    
    // Generate unique ID
    const adapterContent = JSON.stringify(adapterData);
    const hash = crypto.createHash('sha256').update(adapterContent).digest('hex').substring(0, 12);
    const adapterId = `${adapterData.metadata.baseModel.replace(/[^a-zA-Z0-9]/g, '_')}_${metadata.name || 'adapter'}_${hash}`;
    
    // Create adapter package
    const packageData = {
      id: adapterId,
      name: metadata.name || `Adapter for ${adapterData.metadata.baseModel}`,
      description: metadata.description || 'Fine-tuned adapter',
      baseModel: adapterData.metadata.baseModel,
      specialization: metadata.specialization || 'general',
      author: metadata.author || 'unknown',
      version: metadata.version || '1.0.0',
      tags: metadata.tags || [],
      
      // Technical specs
      rank: adapterData.metadata.rank,
      alpha: adapterData.metadata.alpha,
      targetLayers: adapterData.metadata.targetLayers,
      
      // Stats
      size: fs.statSync(adapterPath).size,
      parameters: this.countAdapterParameters(adapterData),
      
      // Metadata
      created: new Date().toISOString(),
      hash: hash,
      
      // Adapter data
      adapters: adapterData.adapters
    };
    
    // Save to registry
    const packagePath = path.join(this.adaptersPath, `${adapterId}.json`);
    fs.writeFileSync(packagePath, JSON.stringify(packageData, null, 2));
    
    // Update registry metadata
    await this.updateRegistryMetadata(adapterId, packageData);
    
    console.log(`📦 Adapter published: ${adapterId}`);
    console.log(`📁 Path: ${packagePath}`);
    console.log(`💾 Size: ${Math.round(packageData.size / 1024)}KB`);
    console.log(`🏷️ Tags: ${packageData.tags.join(', ') || 'none'}`);
    
    return {
      id: adapterId,
      packagePath,
      size: packageData.size,
      url: this.generateShareableURL(adapterId)
    };
  }

  /**
   * Install an adapter from the registry
   */
  async installAdapter(adapterId, targetPath) {
    const packagePath = path.join(this.adaptersPath, `${adapterId}.json`);
    
    if (!fs.existsSync(packagePath)) {
      throw new Error(`Adapter not found in registry: ${adapterId}`);
    }
    
    const packageData = JSON.parse(fs.readFileSync(packagePath, 'utf8'));
    
    // Create adapter data in standard format
    const adapterData = {
      metadata: {
        baseModel: packageData.baseModel,
        rank: packageData.rank,
        alpha: packageData.alpha,
        targetLayers: packageData.targetLayers,
        timestamp: new Date().toISOString(),
        installedFrom: adapterId
      },
      adapters: packageData.adapters
    };
    
    // Ensure target directory exists
    const targetDir = path.dirname(targetPath);
    if (!fs.existsSync(targetDir)) {
      fs.mkdirSync(targetDir, { recursive: true });
    }
    
    // Save adapter
    fs.writeFileSync(targetPath, JSON.stringify(adapterData, null, 2));
    
    console.log(`📥 Adapter installed: ${adapterId}`);
    console.log(`📁 Installed to: ${targetPath}`);
    console.log(`🎯 Base model: ${packageData.baseModel}`);
    console.log(`🏷️ Specialization: ${packageData.specialization}`);
    
    return {
      adapterId,
      targetPath,
      baseModel: packageData.baseModel,
      metadata: packageData
    };
  }

  /**
   * Search for adapters in the registry
   */
  searchAdapters(query = {}) {
    const metadata = this.loadRegistryMetadata();
    const results = [];
    
    for (const [adapterId, adapterInfo] of Object.entries(metadata.adapters)) {
      let match = true;
      
      // Filter by base model
      if (query.baseModel && !adapterInfo.baseModel.includes(query.baseModel)) {
        match = false;
      }
      
      // Filter by specialization
      if (query.specialization && !adapterInfo.specialization.includes(query.specialization)) {
        match = false;
      }
      
      // Filter by tags
      if (query.tags && !query.tags.some(tag => adapterInfo.tags.includes(tag))) {
        match = false;
      }
      
      // Filter by author
      if (query.author && !adapterInfo.author.includes(query.author)) {
        match = false;
      }
      
      if (match) {
        results.push({
          id: adapterId,
          ...adapterInfo
        });
      }
    }
    
    // Sort by creation date (newest first)
    results.sort((a, b) => new Date(b.created) - new Date(a.created));
    
    return results;
  }

  /**
   * List all adapters for a specific base model
   */
  listAdaptersForModel(baseModel) {
    return this.searchAdapters({ baseModel });
  }

  /**
   * Get adapter details
   */
  getAdapterInfo(adapterId) {
    const packagePath = path.join(this.adaptersPath, `${adapterId}.json`);
    
    if (!fs.existsSync(packagePath)) {
      return null;
    }
    
    return JSON.parse(fs.readFileSync(packagePath, 'utf8'));
  }

  /**
   * Export adapter for sharing (creates a shareable package)
   */
  async exportAdapter(adapterId, exportPath) {
    const adapterInfo = this.getAdapterInfo(adapterId);
    
    if (!adapterInfo) {
      throw new Error(`Adapter not found: ${adapterId}`);
    }
    
    // Create export package
    const exportData = {
      format: 'continuum_adapter_v1',
      exported: new Date().toISOString(),
      adapter: adapterInfo
    };
    
    fs.writeFileSync(exportPath, JSON.stringify(exportData, null, 2));
    
    const fileSize = fs.statSync(exportPath).size;
    
    console.log(`📤 Adapter exported: ${adapterId}`);
    console.log(`📁 Export path: ${exportPath}`);
    console.log(`💾 Size: ${Math.round(fileSize / 1024)}KB`);
    console.log(`🌐 Ready for sharing!`);
    
    return {
      exportPath,
      size: fileSize,
      adapterId
    };
  }

  /**
   * Import adapter from shareable package
   */
  async importAdapter(importPath) {
    if (!fs.existsSync(importPath)) {
      throw new Error(`Import file not found: ${importPath}`);
    }
    
    const importData = JSON.parse(fs.readFileSync(importPath, 'utf8'));
    
    if (importData.format !== 'continuum_adapter_v1') {
      throw new Error(`Unsupported adapter format: ${importData.format}`);
    }
    
    const adapterInfo = importData.adapter;
    const adapterId = adapterInfo.id;
    
    // Save to registry
    const packagePath = path.join(this.adaptersPath, `${adapterId}.json`);
    fs.writeFileSync(packagePath, JSON.stringify(adapterInfo, null, 2));
    
    // Update metadata
    await this.updateRegistryMetadata(adapterId, adapterInfo);
    
    console.log(`📥 Adapter imported: ${adapterId}`);
    console.log(`🎯 Base model: ${adapterInfo.baseModel}`);
    console.log(`🏷️ Specialization: ${adapterInfo.specialization}`);
    console.log(`👤 Author: ${adapterInfo.author}`);
    
    return {
      adapterId,
      baseModel: adapterInfo.baseModel,
      metadata: adapterInfo
    };
  }

  /**
   * Update registry metadata
   */
  async updateRegistryMetadata(adapterId, adapterInfo) {
    const metadata = this.loadRegistryMetadata();
    
    metadata.adapters[adapterId] = {
      name: adapterInfo.name,
      baseModel: adapterInfo.baseModel,
      specialization: adapterInfo.specialization,
      author: adapterInfo.author,
      version: adapterInfo.version,
      tags: adapterInfo.tags,
      size: adapterInfo.size,
      parameters: adapterInfo.parameters,
      created: adapterInfo.created
    };
    
    // Update stats
    metadata.stats.totalAdapters = Object.keys(metadata.adapters).length;
    metadata.stats.totalSize = Object.values(metadata.adapters)
      .reduce((sum, adapter) => sum + adapter.size, 0);
    metadata.stats.baseModels = [...new Set(Object.values(metadata.adapters)
      .map(adapter => adapter.baseModel))];
    metadata.stats.lastUpdated = new Date().toISOString();
    
    fs.writeFileSync(this.metadataPath, JSON.stringify(metadata, null, 2));
  }

  /**
   * Load registry metadata
   */
  loadRegistryMetadata() {
    return JSON.parse(fs.readFileSync(this.metadataPath, 'utf8'));
  }

  /**
   * Get registry statistics
   */
  getStats() {
    const metadata = this.loadRegistryMetadata();
    return metadata.stats;
  }

  /**
   * Generate shareable URL (placeholder for future web integration)
   */
  generateShareableURL(adapterId) {
    return `continuum://adapter/${adapterId}`;
  }

  /**
   * Count adapter parameters
   */
  countAdapterParameters(adapterData) {
    let total = 0;
    
    for (const adapter of Object.values(adapterData.adapters)) {
      // A matrix: input_dim × rank
      total += adapter.A.length * adapter.A[0].length;
      // B matrix: rank × output_dim  
      total += adapter.B.length * adapter.B[0].length;
    }
    
    return total;
  }

  /**
   * Clean up old adapters
   */
  cleanup(options = {}) {
    const maxAge = options.maxAge || 30 * 24 * 60 * 60 * 1000; // 30 days
    const maxSize = options.maxSize || 1024 * 1024 * 1024; // 1GB
    
    const metadata = this.loadRegistryMetadata();
    const now = Date.now();
    let cleaned = 0;
    let sizeSaved = 0;
    
    for (const [adapterId, adapterInfo] of Object.entries(metadata.adapters)) {
      const age = now - new Date(adapterInfo.created).getTime();
      
      if (age > maxAge || metadata.stats.totalSize > maxSize) {
        const packagePath = path.join(this.adaptersPath, `${adapterId}.json`);
        
        if (fs.existsSync(packagePath)) {
          sizeSaved += fs.statSync(packagePath).size;
          fs.unlinkSync(packagePath);
          delete metadata.adapters[adapterId];
          cleaned++;
        }
      }
    }
    
    if (cleaned > 0) {
      // Update metadata
      metadata.stats.totalAdapters = Object.keys(metadata.adapters).length;
      metadata.stats.totalSize -= sizeSaved;
      fs.writeFileSync(this.metadataPath, JSON.stringify(metadata, null, 2));
      
      console.log(`🧹 Cleaned up ${cleaned} old adapters (${Math.round(sizeSaved / 1024)}KB)`);
    }
    
    return { cleaned, sizeSaved };
  }
}

module.exports = AdapterRegistry;