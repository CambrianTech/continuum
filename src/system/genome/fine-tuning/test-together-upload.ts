#!/usr/bin/env npx tsx

/**
 * Test Together AI file upload directly to diagnose the issue
 */

import { initializeSecrets, getSecret } from '../../../system/secrets/SecretManager';
import * as fs from 'fs';
import * as path from 'path';

// Declare globals (Node.js 18+ built-ins)
declare const fetch: typeof globalThis.fetch;
/* eslint-disable @typescript-eslint/naming-convention */
declare const FormData: typeof globalThis.FormData;
declare const Blob: typeof globalThis.Blob;
/* eslint-enable @typescript-eslint/naming-convention */

async function testTogetherUpload(): Promise<void> {
  console.log('🧪 Testing Together AI File Upload\n');

  // Initialize SecretManager first
  await initializeSecrets();

  // Get API key
  const apiKey = getSecret('TOGETHER_API_KEY', 'test-together-upload');
  if (!apiKey) {
    console.error('❌ TOGETHER_API_KEY not found in config.env');
    process.exit(1);
  }

  console.log(`✅ API Key loaded: ${apiKey.substring(0, 8)}...`);

  // Create minimal test dataset
  const testData = [
    {
      messages: [
        { role: 'system', content: 'You are a helpful assistant.' },
        { role: 'user', content: 'Hello!' },
        { role: 'assistant', content: 'Hi there!' }
      ]
    },
    {
      messages: [
        { role: 'system', content: 'You are a helpful assistant.' },
        { role: 'user', content: 'How are you?' },
        { role: 'assistant', content: 'I am doing well, thank you!' }
      ]
    }
  ];

  // Export to JSONL
  const tempDir = '/tmp';
  const tempPath = path.join(tempDir, `together-test-${Date.now()}.jsonl`);
  const jsonl = testData.map(d => JSON.stringify(d)).join('\n');
  await fs.promises.writeFile(tempPath, jsonl, 'utf-8');
  console.log(`\n📝 Created test dataset: ${tempPath}`);
  console.log(`   Size: ${jsonl.length} bytes`);

  // Test 1: Fixed implementation with file_name field
  console.log('\n🧪 Test 1: FormData with file_name field (FIXED)');
  try {
    const fileContent = await fs.promises.readFile(tempPath, 'utf-8');
    const blob = new Blob([fileContent], { type: 'application/jsonl' });
    const filename = path.basename(tempPath);

    const formData = new FormData();
    formData.append('file', blob, filename);
    formData.append('file_name', filename);  // REQUIRED by Together AI!
    formData.append('purpose', 'fine-tune');

    console.log('   Uploading...');
    const response = await fetch('https://api.together.xyz/v1/files/upload', {
      method: 'POST',
      headers: {
        'Authorization': `Bearer ${apiKey}`
      },
      body: formData
    });

    console.log(`   Response: ${response.status} ${response.statusText}`);
    const responseText = await response.text();
    console.log(`   Body: ${responseText}`);

    if (response.ok) {
      console.log('   ✅ Success!');
      const data = JSON.parse(responseText);
      console.log(`   File ID: ${data.id}`);
    } else {
      console.log('   ❌ Failed');
    }
  } catch (error) {
    console.error('   ❌ Error:', error);
  }

  // Test 2: Try with application/json content-type
  console.log('\n🧪 Test 2: FormData with file_name field (application/json type)');
  try {
    const fileContent = await fs.promises.readFile(tempPath, 'utf-8');
    const blob = new Blob([fileContent], { type: 'application/json' });
    const filename = path.basename(tempPath);

    const formData = new FormData();
    formData.append('file', blob, filename);
    formData.append('file_name', filename);  // REQUIRED by Together AI!
    formData.append('purpose', 'fine-tune');

    console.log('   Uploading...');
    const response = await fetch('https://api.together.xyz/v1/files/upload', {
      method: 'POST',
      headers: {
        'Authorization': `Bearer ${apiKey}`
      },
      body: formData
    });

    console.log(`   Response: ${response.status} ${response.statusText}`);
    const responseText = await response.text();
    console.log(`   Body: ${responseText}`);

    if (response.ok) {
      console.log('   ✅ Success!');
      const data = JSON.parse(responseText);
      console.log(`   File ID: ${data.id}`);
    } else {
      console.log('   ❌ Failed');
    }
  } catch (error) {
    console.error('   ❌ Error:', error);
  }

  // Test 3: Try with simpler filename
  console.log('\n🧪 Test 3: FormData with file_name field (simple filename)');
  try {
    const fileContent = await fs.promises.readFile(tempPath, 'utf-8');
    const blob = new Blob([fileContent], { type: 'application/jsonl' });
    const filename = 'training.jsonl';

    const formData = new FormData();
    formData.append('file', blob, filename);
    formData.append('file_name', filename);  // REQUIRED by Together AI!
    formData.append('purpose', 'fine-tune');

    console.log('   Uploading...');
    const response = await fetch('https://api.together.xyz/v1/files/upload', {
      method: 'POST',
      headers: {
        'Authorization': `Bearer ${apiKey}`
      },
      body: formData
    });

    console.log(`   Response: ${response.status} ${response.statusText}`);
    const responseText = await response.text();
    console.log(`   Body: ${responseText}`);

    if (response.ok) {
      console.log('   ✅ Success!');
      const data = JSON.parse(responseText);
      console.log(`   File ID: ${data.id}`);
    } else {
      console.log('   ❌ Failed');
    }
  } catch (error) {
    console.error('   ❌ Error:', error);
  }

  // Cleanup
  await fs.promises.unlink(tempPath);
  console.log('\n✅ Test complete\n');
}

testTogetherUpload().catch(console.error);
