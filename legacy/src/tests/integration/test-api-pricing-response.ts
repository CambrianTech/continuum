#!/usr/bin/env tsx
/**
 * Test: What pricing info do AI APIs actually return?
 */

import { OpenAIAdapter } from '../../daemons/ai-provider-daemon/shared/adapters/OpenAIAdapter';
import { AnthropicAdapter } from '../../daemons/ai-provider-daemon/shared/adapters/AnthropicAdapter';
import { initializeSecrets, SecretManager } from '../../system/secrets/SecretManager';

async function inspectAPIResponse() {
  await initializeSecrets();

  const openai = new OpenAIAdapter();
  await openai.initialize();

  console.log('🔍 Testing OpenAI API response structure...\n');
  
  const response = await openai.generateText!({
    messages: [{ role: 'user', content: 'Hi' }],
    maxTokens: 5,
  });

  console.log('📋 Full OpenAI Response Object:');
  console.log(JSON.stringify(response, null, 2));
  
  console.log('\n🔍 Testing Anthropic API response structure...\n');
  
  const anthropic = new AnthropicAdapter();
  await anthropic.initialize();
  
  const claudeResponse = await anthropic.generateText!({
    messages: [{ role: 'user', content: 'Hi' }],
    maxTokens: 5,
  });

  console.log('📋 Full Anthropic Response Object:');
  console.log(JSON.stringify(claudeResponse, null, 2));
}

inspectAPIResponse().catch(console.error);
