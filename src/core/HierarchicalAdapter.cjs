/**
 * Hierarchical Adapter System - Stack specialized LoRA adapters
 * Examples: base-model → lawyer → patent-lawyer → uspto-specialist
 */

const fs = require('fs');
const path = require('path');
const LoRAAdapter = require('./LoRAAdapter.cjs');

class HierarchicalAdapter {
  constructor(baseModel) {
    this.baseModel = baseModel;
    this.adapterStack = []; // Stack of adapters to apply
    this.hierarchy = new Map(); // Track specialization hierarchy
  }

  /**
   * Add a specialization layer to the stack
   */
  addSpecialization(adapterId, metadata = {}) {
    const layer = {
      id: adapterId,
      name: metadata.name || adapterId,
      specialization: metadata.specialization || 'general',
      parent: metadata.parent || null,
      level: this.adapterStack.length,
      domain: metadata.domain || 'general',
      addedAt: new Date().toISOString()
    };
    
    this.adapterStack.push(layer);
    this.hierarchy.set(adapterId, layer);
    
    console.log(`🔗 Added layer ${layer.level}: ${layer.name} (${layer.specialization})`);
    return layer;
  }

  /**
   * Create a complete specialization path
   */
  createSpecializationPath(domainPath, baseAdapterPath) {
    console.log(`🏗️ Building specialization path: ${domainPath}`);
    
    // Example: "continuum.legal.patent.uspto" becomes hierarchy
    const domains = domainPath.split('.');
    let currentPath = this.baseModel;
    
    const pathLayers = [];
    
    for (let i = 0; i < domains.length; i++) {
      const domain = domains[i];
      const fullPath = domains.slice(0, i + 1).join('.');
      const parentPath = i > 0 ? domains.slice(0, i).join('.') : null;
      
      const layer = {
        domain: fullPath,
        name: this.getDomainName(domain),
        specialization: this.getSpecializationType(domain),
        parent: parentPath,
        level: i,
        adapterPath: path.join(baseAdapterPath, `${fullPath}.json`)
      };
      
      pathLayers.push(layer);
      currentPath += ` → ${layer.name}`;
    }
    
    console.log(`📋 Specialization path: ${currentPath}`);
    return pathLayers;
  }

  /**
   * Load and stack multiple adapters
   */
  async loadAdapterStack(adapterPaths) {
    console.log(`📚 Loading adapter stack with ${adapterPaths.length} layers...`);
    
    const loadedAdapters = [];
    
    for (let i = 0; i < adapterPaths.length; i++) {
      const adapterPath = adapterPaths[i];
      console.log(`\n🔍 Loading layer ${i}: ${path.basename(adapterPath)}`);
      
      try {
        const adapter = await LoRAAdapter.loadAdapters(adapterPath);
        
        loadedAdapters.push({
          level: i,
          path: adapterPath,
          adapter: adapter,
          parameters: adapter.countAdapterParameters(),
          baseModel: adapter.baseModel
        });
        
        console.log(`✅ Layer ${i}: ${adapter.countAdapterParameters().toLocaleString()} parameters`);
        
      } catch (error) {
        console.warn(`⚠️ Failed to load layer ${i}: ${error.message}`);
      }
    }
    
    console.log(`\n📊 Total stack: ${loadedAdapters.length} layers`);
    const totalParams = loadedAdapters.reduce((sum, layer) => sum + layer.parameters, 0);
    console.log(`🔢 Combined parameters: ${totalParams.toLocaleString()}`);
    
    return loadedAdapters;
  }

  /**
   * Apply hierarchical adapters to base model
   */
  async applyHierarchicalAdapters(baseModelWeights, adapterStack) {
    console.log(`🔧 Applying ${adapterStack.length} hierarchical adapters...`);
    
    // In real implementation, this would:
    // 1. Start with base model weights
    // 2. Apply each adapter in sequence: W = W + (B @ A) * scaling
    // 3. Each layer builds on the previous layer's modifications
    
    let currentWeights = baseModelWeights; // Start with base
    const appliedModifications = [];
    
    for (const layer of adapterStack) {
      console.log(`⚡ Applying ${path.basename(layer.path)}...`);
      
      // Simulate applying adapter to current weights
      const modifications = layer.adapter.applyToModel(currentWeights);
      appliedModifications.push({
        layer: layer.level,
        name: path.basename(layer.path),
        modifications: modifications.length,
        parameters: layer.parameters
      });
      
      // In real implementation: currentWeights = applyAdapter(currentWeights, layer.adapter)
      console.log(`  🎯 Modified ${modifications.length} layer groups`);
    }
    
    console.log(`\n✅ Hierarchical application complete`);
    console.log(`🏗️ Applied ${appliedModifications.length} specialization layers`);
    
    return {
      finalWeights: currentWeights, // Would be the modified weights
      appliedLayers: appliedModifications,
      totalSpecializations: appliedModifications.length
    };
  }

  /**
   * Generate example specialization hierarchy
   */
  generateExampleHierarchy() {
    const examples = {
      legal: {
        'continuum.legal': {
          name: 'Legal Foundation',
          description: 'Basic legal reasoning and terminology',
          size: '12MB',
          capabilities: ['legal terminology', 'basic law concepts', 'legal reasoning']
        },
        'continuum.legal.patent': {
          name: 'Patent Law Specialist', 
          description: 'Patent law expertise building on legal foundation',
          size: '8MB',
          capabilities: ['patent applications', 'prior art search', 'patent claims']
        },
        'continuum.legal.patent.uspto': {
          name: 'USPTO Procedures Expert',
          description: 'Specific USPTO filing procedures and requirements',
          size: '5MB',
          capabilities: ['USPTO forms', 'filing deadlines', 'examination process']
        },
        'continuum.legal.patent.uspto.biotech': {
          name: 'Biotech Patent Specialist',
          description: 'Biotechnology-specific patent expertise',
          size: '4MB',
          capabilities: ['biotech patents', 'FDA considerations', 'biotech prior art']
        }
      },
      
      medical: {
        'continuum.medical': {
          name: 'Medical Foundation',
          description: 'Basic medical knowledge and terminology',
          size: '15MB',
          capabilities: ['medical terminology', 'anatomy', 'basic diagnostics']
        },
        'continuum.medical.cardiology': {
          name: 'Cardiology Specialist',
          description: 'Cardiovascular medicine expertise',
          size: '10MB',
          capabilities: ['heart conditions', 'cardiac procedures', 'EKG analysis']
        },
        'continuum.medical.cardiology.pediatric': {
          name: 'Pediatric Cardiology',
          description: 'Pediatric heart conditions and treatments',
          size: '6MB',
          capabilities: ['congenital heart defects', 'pediatric procedures']
        }
      },
      
      engineering: {
        'continuum.engineering': {
          name: 'Engineering Foundation',
          description: 'Basic engineering principles and math',
          size: '11MB',
          capabilities: ['engineering math', 'physics', 'problem solving']
        },
        'continuum.engineering.software': {
          name: 'Software Engineering',
          description: 'Software development and architecture',
          size: '9MB',
          capabilities: ['coding', 'architecture', 'debugging']
        },
        'continuum.engineering.software.ai': {
          name: 'AI/ML Engineering',
          description: 'AI and machine learning engineering',
          size: '7MB',
          capabilities: ['ML algorithms', 'model training', 'AI systems']
        }
      }
    };
    
    return examples;
  }

  /**
   * Demonstrate layered specialization
   */
  demonstrateLayering() {
    console.log(`\n🎭 Demonstrating Hierarchical Specialization:`);
    console.log(`\n📚 Base Model: ${this.baseModel}`);
    console.log(`   🧠 General intelligence, language understanding`);
    console.log(`   💾 Size: ~175GB (stays local, never shared)`);
    
    const examples = this.generateExampleHierarchy();
    
    // Legal specialization example
    console.log(`\n⚖️ Legal Specialization Stack:`);
    console.log(`   ${this.baseModel}`);
    console.log(`   ├── + Legal Foundation (12MB) → Legal reasoning`);
    console.log(`   ├── + Patent Law (8MB) → Patent expertise`);
    console.log(`   ├── + USPTO Procedures (5MB) → Filing expertise`);
    console.log(`   └── + Biotech Patents (4MB) → Biotech specialization`);
    console.log(`   📊 Total adapters: 29MB vs 175GB base model`);
    console.log(`   🎯 Result: continuum.legal.patent.uspto.biotech specialist`);
    
    // Medical specialization example  
    console.log(`\n🏥 Medical Specialization Stack:`);
    console.log(`   ${this.baseModel}`);
    console.log(`   ├── + Medical Foundation (15MB) → Medical knowledge`);
    console.log(`   ├── + Cardiology (10MB) → Heart expertise`);
    console.log(`   └── + Pediatric Cardiology (6MB) → Pediatric specialization`);
    console.log(`   📊 Total adapters: 31MB`);
    console.log(`   🎯 Result: continuum.medical.cardiology.pediatric specialist`);
    
    // Mixing specializations
    console.log(`\n🔀 Mixed Specialization (Legal + Medical for medtech):`);
    console.log(`   ${this.baseModel}`);
    console.log(`   ├── + Legal Foundation (12MB)`);
    console.log(`   ├── + Patent Law (8MB)`);
    console.log(`   ├── + Medical Foundation (15MB)`);
    console.log(`   └── + Medical Device Patents (6MB)`);
    console.log(`   📊 Total adapters: 41MB`);
    console.log(`   🎯 Result: Medical device patent specialist`);
  }

  /**
   * Calculate storage efficiency
   */
  calculateStorageEfficiency(adapterPaths) {
    const baseModelSize = 175 * 1024 * 1024 * 1024; // 175GB
    let totalAdapterSize = 0;
    
    for (const adapterPath of adapterPaths) {
      if (fs.existsSync(adapterPath)) {
        totalAdapterSize += fs.statSync(adapterPath).size;
      } else {
        totalAdapterSize += 8 * 1024 * 1024; // Estimate 8MB per adapter
      }
    }
    
    const reduction = baseModelSize / totalAdapterSize;
    
    return {
      baseModelSize,
      totalAdapterSize,
      reduction: Math.round(reduction),
      adapterSizeMB: Math.round(totalAdapterSize / 1024 / 1024),
      baseModelSizeGB: Math.round(baseModelSize / 1024 / 1024 / 1024)
    };
  }

  /**
   * Helper methods
   */
  getDomainName(domain) {
    const names = {
      continuum: 'Continuum Foundation',
      legal: 'Legal Foundation',
      patent: 'Patent Law',
      uspto: 'USPTO Procedures',
      biotech: 'Biotech Specialty',
      medical: 'Medical Foundation',
      cardiology: 'Cardiology',
      pediatric: 'Pediatric Specialty',
      engineering: 'Engineering Foundation',
      software: 'Software Engineering',
      ai: 'AI/ML Engineering'
    };
    
    return names[domain] || domain;
  }

  getSpecializationType(domain) {
    const types = {
      continuum: 'foundation',
      legal: 'domain_foundation',
      patent: 'specialization',
      uspto: 'procedural',
      biotech: 'subspecialty',
      medical: 'domain_foundation',
      cardiology: 'specialization',
      pediatric: 'subspecialty'
    };
    
    return types[domain] || 'specialty';
  }
}

module.exports = HierarchicalAdapter;