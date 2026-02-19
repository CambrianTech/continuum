/**
 * SQL Adapter Test
 * Tests the SQL expert LoRA adapter
 */
import { InferenceGrpcClient } from '../system/core/services/InferenceGrpcClient';

async function main() {
  console.log('SQL Adapter Test\n');
  const client = new InferenceGrpcClient();

  try {
    // Check status
    console.log('1. Checking status...');
    const status = await client.status();
    console.log('   Model: ' + status.currentModel);
    console.log('   Healthy: ' + status.healthy + '\n');

    // Generate with base model first
    const sqlPrompt = 'Convert this to SQL: Get all users where age is greater than 25\n\nSQL:';

    console.log('2. Base model (no adapter):');
    console.log('   Prompt: "' + sqlPrompt.replace(/\n/g, ' ') + '"');
    const base = await client.generate(status.currentModel, sqlPrompt, { maxTokens: 60, temperature: 0.1 });
    console.log('   Result: ' + base.text.trim() + '\n');

    // Load SQL adapter
    console.log('3. Loading SQL adapter...');
    const adapterPath = '/Users/joel/.continuum/adapters/installed/SujanKarki--Llama-3.2-3B-Instruct_text_to_sql_lora_newdataset/adapter_model.safetensors';
    await client.loadAdapter('sql-expert', adapterPath, { scale: 1.0, merge: true });
    console.log('   Adapter loaded\n');

    // Generate with SQL adapter
    console.log('4. With SQL adapter:');
    const merged = await client.generate(status.currentModel, sqlPrompt, { maxTokens: 60, temperature: 0.1 });
    console.log('   Result: ' + merged.text.trim() + '\n');

    console.log('Test complete!');
  } catch (err) {
    console.error('Error:', err);
  }
}

main().catch(console.error);
