import { InferenceGrpcClient } from '../system/core/services/InferenceGrpcClient';

async function test() {
  console.log('Testing direct Candle inference...');
  const client = InferenceGrpcClient.sharedInstance();

  // Simple Llama 3.2 chat format prompt
  const prompt = '<|begin_of_text|><|start_header_id|>user<|end_header_id|>\n\nWhat is 2+2?<|eot_id|><|start_header_id|>assistant<|end_header_id|>\n\n';

  console.log('Prompt:', prompt);

  const result = await client.generate('Llama-3.2-3B-Instruct', prompt, { maxTokens: 50 });
  console.log('Result:', JSON.stringify(result, null, 2));
}

test().catch(console.error);
