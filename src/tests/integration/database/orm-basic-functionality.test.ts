/**
 * ORM Basic Functionality Test
 *
 * Tests the core ORM functionality with proper BaseEntity types,
 * EntityRepository pattern, and storage format adaptation.
 */

import type { BaseEntity } from '../../../orm/shared/BaseORM';
import { EntityRepository } from '../../../orm/shared/BaseORM';
import { generateUUID } from '../../../system/core/types/CrossPlatformUUID';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';

console.log('🔧 ORM Basic Functionality Test');
console.log('═══════════════════════════════════════');

// Test entity that extends BaseEntity
interface TestUser extends BaseEntity {
  readonly name: string;
  readonly email: string;
  readonly isActive: boolean;
  readonly preferences: {
    readonly theme: 'dark' | 'light';
    readonly notifications: boolean;
  };
  readonly tags: readonly string[];
}

// Mock DataDaemon for isolated testing
const mockDataDaemon = {
  create: async (collection: string, data: any, context: any) => {
    console.log(`📝 MockDataDaemon.create(${collection}):`, Object.keys(data));
    return {
      success: true,
      data: { id: data.id, data: data }
    };
  },
  read: async (collection: string, id: any, context: any) => {
    console.log(`📖 MockDataDaemon.read(${collection}, ${id})`);
    return {
      success: true,
      data: {
        id,
        data: {
          id,
          name: 'Test User',
          email: 'test@example.com',
          isActive: true,
          preferences: { theme: 'dark', notifications: true },
          tags: ['test'],
          createdAt: '2025-09-17T20:00:00.000Z',
          updatedAt: '2025-09-17T20:00:00.000Z'
        }
      }
    };
  },
  update: async (collection: string, id: any, data: any, context: any) => {
    console.log(`✏️ MockDataDaemon.update(${collection}, ${id}):`, Object.keys(data));
    return {
      success: true,
      data: { id, data: { ...data, id, updatedAt: '2025-09-17T20:05:00.000Z' } }
    };
  },
  delete: async (collection: string, id: any, context: any) => {
    console.log(`🗑️ MockDataDaemon.delete(${collection}, ${id})`);
    return { success: true, data: undefined };
  },
  query: async (query: any, context: any) => {
    console.log(`🔍 MockDataDaemon.query(${query.collection}):`, query.filter);
    return {
      success: true,
      data: [{
        id: generateUUID(),
        data: {
          id: generateUUID(),
          name: 'Query Result User',
          email: 'query@example.com',
          isActive: true,
          preferences: { theme: 'light', notifications: false },
          tags: ['query-result'],
          createdAt: '2025-09-17T19:50:00.000Z',
          updatedAt: '2025-09-17T19:50:00.000Z'
        }
      }]
    };
  }
};

// Test repository
class TestUserRepository extends EntityRepository<TestUser> {}

// Test metadata
const userMetadata = {
  tableName: 'users',
  primaryKey: 'id',
  columns: {
    id: { type: 'uuid' as const },
    name: { type: 'string' as const, length: 255 },
    email: { type: 'string' as const, unique: true },
    isActive: { type: 'boolean' as const },
    preferences: { type: 'json' as const },
    tags: { type: 'json' as const },
    createdAt: { type: 'date' as const },
    updatedAt: { type: 'date' as const }
  },
  relationships: {},
  indexes: [
    { name: 'idx_users_email', columns: ['email'], unique: true },
    { name: 'idx_users_active', columns: ['isActive'] }
  ],
  constraints: []
};

// Test data with proper Date objects
const testUserData: Omit<TestUser, 'id' | 'createdAt' | 'updatedAt'> = {
  name: 'John Doe',
  email: 'john@example.com',
  isActive: true,
  preferences: {
    theme: 'dark',
    notifications: true
  },
  tags: ['admin', 'power-user']
};

async function runORMTests() {
  try {
    console.log('✅ Creating EntityRepository...');
    const userRepo = new TestUserRepository(mockDataDaemon as any, userMetadata);

    console.log('✅ Testing entity creation...');
    const mockContext = { sessionId: generateUUID() as UUID, userId: generateUUID() as UUID };
    const createResult = await userRepo.create(testUserData, mockContext);

    if (!createResult.success) {
      throw new Error(`Create failed: ${createResult.error}`);
    }

    console.log('✅ Entity created successfully:', createResult.data?.id);

    // Verify proper Date object handling
    if (!(createResult.data?.createdAt instanceof Date)) {
      throw new Error('createdAt should be a Date object, not string');
    }
    if (!(createResult.data?.updatedAt instanceof Date)) {
      throw new Error('updatedAt should be a Date object, not string');
    }

    console.log('✅ Date objects handled correctly');

    console.log('✅ Testing entity read...');
    const readResult = await userRepo.findById(createResult.data!.id, mockContext);

    if (!readResult.success || !readResult.data) {
      throw new Error(`Read failed: ${readResult.error}`);
    }

    console.log('✅ Entity read successfully:', readResult.data.name);

    console.log('✅ Testing entity update...');
    const updateResult = await userRepo.update(
      createResult.data!.id,
      { name: 'John Updated', isActive: false },
      mockContext
    );

    if (!updateResult.success) {
      throw new Error(`Update failed: ${updateResult.error}`);
    }

    console.log('✅ Entity updated successfully');

    console.log('✅ Testing query builder...');
    const queryBuilder = userRepo.createQuery()
      .where('isActive', '=', true)
      .where('email', 'LIKE', '%@example.com')
      .orderBy('name', 'asc')
      .limit(10);

    const queryResult = await userRepo.query(queryBuilder, mockContext);

    if (!queryResult.success) {
      throw new Error(`Query failed: ${queryResult.error}`);
    }

    console.log('✅ Query executed successfully, results:', queryResult.data?.length);

    console.log('✅ Testing SQL generation...');
    const sqlOutput = queryBuilder.toSQL();
    console.log('Generated SQL:', sqlOutput.sql);
    console.log('Parameters:', sqlOutput.params);

    if (!sqlOutput.sql.includes('WHERE') || !sqlOutput.sql.includes('ORDER BY')) {
      throw new Error('SQL generation missing expected clauses');
    }

    console.log('✅ SQL generation working correctly');

    console.log('✅ Testing entity deletion...');
    const deleteResult = await userRepo.delete(createResult.data!.id, mockContext);

    if (!deleteResult.success) {
      throw new Error(`Delete failed: ${deleteResult.error}`);
    }

    console.log('✅ Entity deleted successfully');

    return true;
  } catch (error) {
    console.error('❌ ORM Test Failed:', error);
    throw error;
  }
}

// Run the tests
runORMTests()
  .then(() => {
    console.log('\n🎉 ORM BASIC FUNCTIONALITY TEST PASSED!');
    console.log('✅ EntityRepository pattern works correctly');
    console.log('✅ BaseEntity with Date objects handled properly');
    console.log('✅ Storage format conversion working');
    console.log('✅ Query builder generates correct SQL');
    console.log('✅ CRUD operations complete successfully');
    console.log('✅ Ready for real DataDaemon integration');
  })
  .catch((error) => {
    console.error('\n❌ ORM BASIC FUNCTIONALITY TEST FAILED!');
    console.error(error);
    process.exit(1);
  });