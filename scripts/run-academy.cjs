#!/usr/bin/env node
/**
 * Academy Training Script
 * Train AI personas through adversarial boot camp
 */

require('dotenv').config();
const Academy = require('../src/core/Academy.cjs');
const ModelCaliber = require('../src/core/ModelCaliber.cjs');
const { ModelRegistry } = require('../src/core/AIModel.cjs');

async function main() {
  console.log(`
🎓 ACADEMY 🎓
"A very funny thing happened on the way to AI safety..."

Training AI personas through adversarial boot camp!
`);

  // Initialize training infrastructure
  const modelRegistry = new ModelRegistry();
  const modelCaliber = new ModelCaliber();
  const academy = new Academy(modelRegistry, modelCaliber);

  // Train multiple personas
  const trainingQueue = [
    { name: 'sheriff-mahoney', specialization: 'protocol_enforcement' },
    { name: 'officer-hightower', specialization: 'security_validation' },
    { name: 'cadet-jones', specialization: 'command_detection' }
  ];

  console.log(`👮 Training ${trainingQueue.length} recruits through Academy...\\n`);

  for (const recruit of trainingQueue) {
    try {
      const result = await academy.trainNewPersona(
        recruit.name, 
        recruit.specialization, 
        5 // 5 rounds for demo
      );

      if (result.status === 'graduated') {
        console.log(`\\n✅ ${recruit.name} graduated and is ready for deployment!`);
      } else {
        console.log(`\\n❌ ${recruit.name} needs additional training`);
      }

    } catch (error) {
      console.error(`💥 Training failed for ${recruit.name}:`, error.message);
    }
  }

  // Show final academy stats
  console.log(`\\n🏆 Academy Final Report:`);
  academy.printAcademyStats();

  // List all graduated personas
  const graduates = academy.getGraduatedPersonas();
  if (graduates.length > 0) {
    console.log(`\\n👮 Academy Graduates:`);
    graduates.forEach(persona => {
      console.log(`   🎓 ${persona.metadata.name} - ${persona.metadata.specialty} (${(persona.performance.academyScore * 100).toFixed(1)}%)`);
    });
  }

  console.log(`\\n🎬 "That's a wrap! Remember: In Academy, everyone's a winner... eventually!"`);
}

if (require.main === module) {
  main().catch(console.error);
}

module.exports = { main };