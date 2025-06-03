#!/usr/bin/env node
/**
 * Persona Factory Demo - AI-driven persona construction
 * Shows complete pipeline: Requirements → Design → Police Academy → Deployment
 */

require('dotenv').config();
const PersonaFactory = require('../src/core/PersonaFactory.cjs');
const ModelCaliber = require('../src/core/ModelCaliber.cjs');
const { ModelRegistry } = require('../src/core/AIModel.cjs');

async function main() {
  console.log(`
🏭 PERSONA FACTORY 🏭
"AI personas, constructed on demand!"

Demonstrating the complete AI persona lifecycle:
Requirements → Design → Police Academy → Deployment
`);

  // Initialize the factory
  const modelRegistry = new ModelRegistry();
  const modelCaliber = new ModelCaliber();
  const factory = new PersonaFactory(modelRegistry, modelCaliber);

  // Demo 1: Single persona construction
  console.log(`\\n📋 DEMO 1: Single Persona Construction`);
  
  const securityRequirements = {
    task: "security protocol validation for financial APIs",
    requiredAccuracy: 92,
    maxLatency: "fast",
    maxCost: "balanced"
  };

  try {
    const securityPersona = await factory.constructPersona(securityRequirements);
    
    if (securityPersona.status === 'graduated') {
      console.log(`\\n🚀 Deploying ${securityPersona.name} for security validation...`);
      const deployment = await factory.deployPersona(securityPersona.name, {
        task: "Validate API security protocols"
      });
      
      // Benchmark the persona
      const benchmark = await factory.benchmarkPersona(securityPersona.name, 'security_suite');
    }
  } catch (error) {
    console.error(`❌ Demo 1 failed:`, error.message);
  }

  // Demo 2: Squad construction
  console.log(`\\n\\n📋 DEMO 2: Squad Construction`);
  
  const squadRequirements = {
    name: "legal-compliance-squad",
    mission: "comprehensive legal document analysis",
    roles: ["legal-analyst", "compliance-checker", "risk-assessor"],
    minAccuracy: 88,
    maxLatency: "standard", 
    budget: "premium"
  };

  try {
    const squad = await factory.constructSquad(squadRequirements);
    
    console.log(`\\n📊 Squad Performance Summary:`);
    for (const [role, persona] of squad.members) {
      if (persona.status === 'graduated') {
        console.log(`   👮 ${role}: ${(persona.graduationScore * 100).toFixed(1)}% accuracy`);
      } else {
        console.log(`   ⚠️ ${role}: needs additional training`);
      }
    }
  } catch (error) {
    console.error(`❌ Demo 2 failed:`, error.message);
  }

  // Demo 3: Dynamic requirements
  console.log(`\\n\\n📋 DEMO 3: High-Performance Specialist`);
  
  const highPerfRequirements = {
    task: "critical command injection detection",
    requiredAccuracy: 96,
    maxLatency: "ultra-fast",
    maxCost: "minimal"
  };

  try {
    const specialist = await factory.constructPersona(highPerfRequirements);
    
    if (specialist.status === 'graduated') {
      console.log(`\\n🏆 High-performance specialist ready!`);
      await factory.benchmarkPersona(specialist.name, 'injection_detection_suite');
    }
  } catch (error) {
    console.error(`❌ Demo 3 failed:`, error.message);
  }

  // Show final factory statistics
  console.log(`\\n\\n📊 PERSONA FACTORY FINAL REPORT:`);
  const stats = factory.getFactoryStats();
  console.log(`   🏭 Total Personas Constructed: ${stats.totalConstructed}`);
  console.log(`   👮 Active Personas: ${stats.activePersonas.join(', ')}`);
  console.log(`   🎓 Academy Graduation Rate: ${(stats.academyStats.graduated / Math.max(1, stats.academyStats.totalRecruits) * 100).toFixed(1)}%`);

  console.log(`\\n🎬 "That's a wrap! Your AI workforce is ready for deployment!"`);
  console.log(`💡 Next: Deploy personas in your Continuum squads for real tasks!`);
}

if (require.main === module) {
  main().catch(console.error);
}

module.exports = { main };