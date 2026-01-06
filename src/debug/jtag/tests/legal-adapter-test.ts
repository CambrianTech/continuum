/**
 * Legal Adapter Test
 * Tests the Legal expert LoRA adapter
 */
import { InferenceGrpcClient } from '../system/core/services/InferenceGrpcClient';

async function main() {
  console.log('Legal Adapter Test\n');
  const client = new InferenceGrpcClient();

  try {
    const status = await client.status();
    console.log('Model: ' + status.currentModel + '\n');

    const legalPrompt = 'What are the key elements required to form a valid contract?\n\nAnswer:';

    console.log('1. Base model (no adapter):');
    const base = await client.generate(status.currentModel, legalPrompt, { maxTokens: 100, temperature: 0.3 });
    console.log('   ' + base.text.trim().substring(0, 200) + '...\n');

    // Unload previous adapter and load legal
    console.log('2. Loading Legal adapter...');
    await client.unloadAdapter('sql-expert');
    const adapterPath = '/Users/joel/.continuum/adapters/installed/sartajbhuvaji--Legal-Llama-3.2-3B-Instruct/adapter_model.safetensors';
    await client.loadAdapter('legal-expert', adapterPath, { scale: 1.0, merge: true });
    console.log('   Loaded\n');

    console.log('3. With Legal adapter:');
    const merged = await client.generate(status.currentModel, legalPrompt, { maxTokens: 100, temperature: 0.3 });
    console.log('   ' + merged.text.trim().substring(0, 200) + '...\n');

    console.log('Test complete!');
  } catch (err) {
    console.error('Error:', err);
  }
}

main().catch(console.error);
