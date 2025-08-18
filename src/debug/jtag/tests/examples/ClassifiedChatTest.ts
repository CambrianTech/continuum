#!/usr/bin/env npx tsx
/**
 * Example: Classified Chat Integration Test
 * Demonstrates the middle-out test classification system
 */

import { TestSpec, TestLevel, TestImportance, TestCategory } from '../shared/TestDecorators';
import { TestUserManager } from '../shared/TestUserManager';

/**
 * Multi-User Chat Integration Test
 * CLASSIFIED: System-level, Critical importance, Chat category
 */
@TestSpec({
  level: TestLevel.SYSTEM,
  importance: TestImportance.CRITICAL,
  category: TestCategory.CHAT,
  description: 'Multi-user chat with database persistence and real-time events',
  timeout: 60000,
  requiresSystem: true,
  dependencies: ['chat/send-message', 'data/list', 'events-daemon']
})
export class MultiUserChatTest {
  
  static async run(): Promise<boolean> {
    console.log('💬 CLASSIFIED MULTI-USER CHAT TEST');
    console.log('🏷️  Level: SYSTEM | Importance: CRITICAL | Category: CHAT');
    
    const userManager = new TestUserManager();
    
    try {
      const roomId = 'classified-test-room';
      
      // Connect three users using encapsulated manager
      console.log('👥 Connecting 3 classified test users...');
      const users = await userManager.connectStandardUsers();
      console.log('✅ All classified test users connected');
      
      // Execute critical chat test sequence using manager
      console.log('📨 Executing critical chat sequence...');
      
      await userManager.executeConversation(roomId, [
        {
          user: 'Human',
          content: 'CRITICAL TEST: Multi-user chat system validation'
        },
        {
          user: 'AIAssistant', 
          content: 'CRITICAL TEST: AI confirms message reception and database persistence'
        },
        {
          user: 'DevAssistant',
          content: 'CRITICAL TEST: DevAssistant validates real-time event system'
        }
      ]);
      
      // Validate database persistence (critical requirement)
      console.log('🗄️ Validating critical database persistence...');
      const firstUser = userManager.getUser('Human');
      if (!firstUser) {
        throw new Error('Human user not found');
      }
      
      const dbResult = await firstUser.client.commands['data/list']({
        collection: 'chat_messages',
        format: 'json'
      });
      
      const roomMessages = dbResult.items?.filter((item: any) => 
        item.data?.roomId === roomId
      ) || [];
      
      const systemWorking = roomMessages.length >= 3;
      
      console.log('📊 CLASSIFIED TEST RESULTS:');
      console.log(`🏷️  Test Classification: SYSTEM/CRITICAL/CHAT`);
      console.log(`💾 Database Messages: ${roomMessages.length}/3`);
      console.log(`✅ Critical System Status: ${systemWorking ? 'PASS' : 'FAIL'}`);
      
      return systemWorking;
      
    } catch (error) {
      console.error('❌ Classified chat test failed:', error);
      return false;
    } finally {
      // Clean disconnect using manager
      await userManager.disconnectAll();
    }
  }
}

// Auto-run if called directly
if (require.main === module) {
  MultiUserChatTest.run().then(success => {
    console.log(success ? '🎉 CLASSIFIED CHAT TEST PASSED' : '💥 CLASSIFIED CHAT TEST FAILED');
    process.exit(success ? 0 : 1);
  }).catch(error => {
    console.error('💥 Test execution failed:', error);
    process.exit(1);
  });
}