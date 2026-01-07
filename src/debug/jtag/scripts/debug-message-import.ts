#!/usr/bin/env tsx
/**
 * Debug Chat Message Import Issues
 *
 * Let's see why messages fail validation during import
 */

import { DataServiceFactory } from '../system/data/services/DataServiceFactory';
import { validateMessageData } from '../system/data/domains/ChatMessage';
import { getDatabasePath } from '../system/config/ServerConfig';

async function debugMessageImport() {
  console.log('🔍 Debugging chat message import issues...');

  try {
    const dataService = await DataServiceFactory.createSQLiteOnly(getDatabasePath());

    const initResult = await dataService.initialize();
    if (!initResult.success) {
      throw new Error(`DataService initialization failed: ${initResult.error?.message}`);
    }

    // Export current messages to see their structure
    console.log('\n📋 Step 1: Export current chat messages');
    const messagesExport = await dataService.export('chat_messages');
    if (!messagesExport.success) {
      throw new Error(`Export failed: ${messagesExport.error?.message}`);
    }

    const messages = messagesExport.data.entities;
    console.log(`✅ Exported ${messages.length} messages`);

    // Examine each message structure
    console.log('\n📋 Step 2: Examine message structures');
    messages.forEach((msg, index) => {
      console.log(`\n   Message ${index + 1}:`);
      console.log(`   - ID: ${msg.id}`);
      console.log(`   - Content type: ${typeof msg.content}`);
      if (typeof msg.content === 'object' && msg.content) {
        console.log(`   - Content.text: "${msg.content.text}" (type: ${typeof msg.content.text})`);
        console.log(`   - Content keys: ${Object.keys(msg.content)}`);
      } else {
        console.log(`   - Content: ${msg.content}`);
      }
      console.log(`   - SenderId: ${msg.senderId}`);
      console.log(`   - RoomId: ${msg.roomId}`);
    });

    // Test each message for validation
    console.log('\n📋 Step 3: Test validation on each message');
    messages.forEach((msg, index) => {
      console.log(`\n   Testing message ${index + 1} validation:`);

      // Remove BaseEntity fields for validation
      const { id, createdAt, updatedAt, version, ...cleanMsg } = msg;

      try {
        const validation = validateMessageData(cleanMsg as any);
        if (validation.success) {
          console.log(`   ✅ Message ${index + 1}: VALID`);
        } else {
          console.log(`   ❌ Message ${index + 1}: INVALID - ${validation.error.message}`);
          console.log(`      Error code: ${validation.error.code}`);
        }
      } catch (error: any) {
        console.log(`   ❌ Message ${index + 1}: VALIDATION ERROR - ${error.message}`);
      }
    });

    // Test import of each message individually
    console.log('\n📋 Step 4: Test individual message imports');
    for (let i = 0; i < messages.length; i++) {
      const msg = messages[i];
      const { id, createdAt, updatedAt, version, ...cleanMsg } = msg;

      console.log(`\n   Importing message ${i + 1} individually:`);
      try {
        const importResult = await dataService.import('chat_messages', [cleanMsg]);
        if (importResult.success) {
          console.log(`   ✅ Message ${i + 1}: imported successfully`);
        } else {
          console.log(`   ❌ Message ${i + 1}: import failed - ${importResult.error?.message}`);
          if (importResult.data.errors.length > 0) {
            console.log(`   ❌ Import errors: ${importResult.data.errors.join(', ')}`);
          }
        }
      } catch (error: any) {
        console.log(`   ❌ Message ${i + 1}: import threw error - ${error.message}`);
      }
    }

    await dataService.close();

  } catch (error: any) {
    console.error('❌ DEBUG FAILED:', error.message);
    console.error(error.stack);
  }
}

// Run debug
debugMessageImport();