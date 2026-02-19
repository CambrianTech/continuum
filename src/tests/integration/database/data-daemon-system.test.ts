/**
 * Data Daemon System Testing
 * 
 * Test the existing data infrastructure to understand:
 * 1. How DataDaemon works with different adapters
 * 2. What paths are actually used for storage
 * 3. How ChatDaemonServer integrates with DataDaemon
 * 4. Identify inconsistencies and issues
 */

import { DataDaemon } from '../../../daemons/data-daemon/shared/DataDaemon';
import type { DataOperationContext } from '../../../daemons/data-daemon/shared/DataDaemon';
import { generateUUID } from '../../../system/core/types/CrossPlatformUUID';
import * as fs from 'fs/promises';
import * as path from 'path';

// Test data structures
interface TestUser {
  name: string;
  email: string;
  type: 'human' | 'ai';
}

interface TestChatRoom {
  name: string;
  description: string;
  isPrivate: boolean;
}

interface TestMessage {
  roomId: string;
  userId: string;
  content: string;
  timestamp: string;
}

async function testDataDaemonWithFileAdapter() {
  console.log('\n🧪 TESTING: DataDaemon with File Adapter');
  
  // Create DataDaemon with file adapter (like ChatDaemonServer uses)
  const dataDaemon = new DataDaemon({
    strategy: 'file',
    backend: 'file', 
    namespace: 'test-system',
    options: {
      basePath: '.continuum/jtag/test-data',
      createDirectories: true,
      atomicWrites: true
    }
  });

  await dataDaemon.initialize();

  const context: DataOperationContext = {
    sessionId: generateUUID(),
    timestamp: new Date().toISOString(),
    source: 'data-daemon-test',
    consistency: 'strong'
  };

  try {
    // Test 1: Create user data
    console.log('\n📝 Test 1: Create user data');
    const userData: TestUser = {
      name: 'Claude AI',
      email: 'claude@anthropic.com',
      type: 'ai'
    };
    
    const userResult = await dataDaemon.create('users', userData, context);
    console.log('User create result:', userResult.success ? '✅ SUCCESS' : '❌ FAILED', userResult.error || '');
    
    if (userResult.success) {
      console.log('User ID:', userResult.data?.id);
      console.log('User data:', userResult.data?.data);
    }

    // Test 2: Create chat room data
    console.log('\n🏠 Test 2: Create chat room data');
    const roomData: TestChatRoom = {
      name: 'General Discussion',
      description: 'Main chat room for general topics',
      isPrivate: false
    };
    
    const roomResult = await dataDaemon.create('chat-rooms', roomData, context);
    console.log('Room create result:', roomResult.success ? '✅ SUCCESS' : '❌ FAILED', roomResult.error || '');
    
    if (roomResult.success) {
      console.log('Room ID:', roomResult.data?.id);
    }

    // Test 3: Create message data
    console.log('\n💬 Test 3: Create message data');
    const messageData: TestMessage = {
      roomId: roomResult.data?.id || 'unknown',
      userId: userResult.data?.id || 'unknown',
      content: 'Hello! Testing the data daemon system.',
      timestamp: new Date().toISOString()
    };
    
    const messageResult = await dataDaemon.create('messages', messageData, context);
    console.log('Message create result:', messageResult.success ? '✅ SUCCESS' : '❌ FAILED', messageResult.error || '');

    // Test 4: Query data back
    console.log('\n🔍 Test 4: Query data back');
    
    const usersQuery = await dataDaemon.query({
      collection: 'users',
      limit: 10
    }, context);
    console.log('Users query result:', usersQuery.success ? '✅ SUCCESS' : '❌ FAILED', usersQuery.error || '');
    console.log('Users found:', usersQuery.data?.length || 0);

    const roomsQuery = await dataDaemon.query({
      collection: 'chat-rooms',
      limit: 10
    }, context);
    console.log('Rooms query result:', roomsQuery.success ? '✅ SUCCESS' : '❌ FAILED', roomsQuery.error || '');
    console.log('Rooms found:', roomsQuery.data?.length || 0);

    const messagesQuery = await dataDaemon.query({
      collection: 'messages',
      limit: 10
    }, context);
    console.log('Messages query result:', messagesQuery.success ? '✅ SUCCESS' : '❌ FAILED', messagesQuery.error || '');
    console.log('Messages found:', messagesQuery.data?.length || 0);

    // Test 5: Check actual file paths created
    console.log('\n📁 Test 5: Check actual file paths created');
    const basePath = '.continuum/jtag/test-data';
    
    try {
      const dirs = await fs.readdir(basePath);
      console.log('Data directories:', dirs);
      
      for (const dir of dirs) {
        const collectionPath = path.join(basePath, dir);
        try {
          const files = await fs.readdir(collectionPath);
          console.log(`${dir}/ contains:`, files.length, 'files');
          
          if (files.length > 0) {
            // Read first file to see structure
            const firstFile = path.join(collectionPath, files[0]);
            const content = await fs.readFile(firstFile, 'utf-8');
            const record = JSON.parse(content);
            console.log(`  Sample record structure:`, Object.keys(record));
            console.log(`  Sample record:`, record);
          }
        } catch (dirError) {
          console.log(`  Error reading ${dir}:`, dirError);
        }
      }
    } catch (error) {
      console.log('Error reading data directory:', error);
    }

  } finally {
    await dataDaemon.close();
  }
}

async function testCurrentDataCommands() {
  console.log('\n🧪 TESTING: Current Data Commands');
  
  // Test the actual data commands that are currently hardcoded
  console.log('\n📝 Testing data/create command');
  
  // Create test data using data/create command
  const testData = {
    name: 'Test Record',
    value: 'Command Test',
    timestamp: new Date().toISOString()
  };
  
  // This would use the hardcoded file paths in DataCreateServerCommand
  console.log('Test data prepared:', testData);
  
  // Test data/list command
  console.log('\n📝 Testing data/list command');
  console.log('This would use DataListServerCommand hardcoded path: .continuum/database/');
  
  // Test data/read command  
  console.log('\n📝 Testing data/read command');
  console.log('This would use DataReadServerCommand hardcoded path: .continuum/jtag/sessions/user/{sessionId}/data/');
  
  console.log('\n⚠️  PATH INCONSISTENCY IDENTIFIED:');
  console.log('   - data/create → .continuum/database/{collection}/');
  console.log('   - data/list   → .continuum/database/{collection}/');  
  console.log('   - data/read   → .continuum/jtag/sessions/user/{sessionId}/data/{collection}/');
  console.log('   - data/update → .continuum/jtag/sessions/user/{sessionId}/data/{collection}/');
  console.log('   - data/delete → .continuum/jtag/sessions/user/{sessionId}/data/{collection}/');
  
  console.log('\n💡 CONCLUSION: Create/List commands use global path, Read/Update/Delete use session path!');
}

async function examineExistingDataFiles() {
  console.log('\n🧪 EXAMINING: Existing Data Files');
  
  const paths = [
    '.continuum/database',
    '.continuum/jtag/sessions',
    '.continuum/jtag/test-data'
  ];
  
  for (const basePath of paths) {
    console.log(`\n📁 Checking: ${basePath}`);
    try {
      const exists = await fs.access(basePath).then(() => true, () => false);
      if (!exists) {
        console.log('  ❌ Path does not exist');
        continue;
      }
      
      const items = await fs.readdir(basePath);
      console.log(`  📊 Contains ${items.length} items:`, items);
      
      // Look for collections
      for (const item of items.slice(0, 3)) { // Limit to first 3 items
        const itemPath = path.join(basePath, item);
        const stats = await fs.stat(itemPath);
        
        if (stats.isDirectory()) {
          try {
            const subItems = await fs.readdir(itemPath);
            console.log(`    📂 ${item}/ contains:`, subItems.length, 'items');
            
            // Look for JSON files
            const jsonFiles = subItems.filter(f => f.endsWith('.json'));
            if (jsonFiles.length > 0) {
              console.log(`      📄 JSON files:`, jsonFiles.slice(0, 2)); // First 2 files
            }
          } catch (subError) {
            console.log(`    ❌ Error reading ${item}:`, subError);
          }
        }
      }
    } catch (error) {
      console.log(`  ❌ Error reading ${basePath}:`, error);
    }
  }
}

async function main() {
  console.log('🚀 COMPREHENSIVE DATA DAEMON SYSTEM TEST');
  console.log('==========================================');
  
  try {
    await examineExistingDataFiles();
    await testDataDaemonWithFileAdapter(); 
    await testCurrentDataCommands();
    
    console.log('\n✅ DATA DAEMON SYSTEM TEST COMPLETED');
    console.log('\nNEXT STEPS:');
    console.log('1. Fix path inconsistency in data commands');
    console.log('2. Create unified data abstraction layer');
    console.log('3. Replace hardcoded file operations with DataDaemon');
    console.log('4. Add SQLite adapter option');
    
  } catch (error) {
    console.error('❌ TEST FAILED:', error);
  }
}

main().catch(console.error);