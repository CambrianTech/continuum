#!/usr/bin/env tsx
/**
 * Delete all anonymous users
 *
 * Anonymous users are created when browsers open without a stored userId.
 * This script deletes them all and clears any stale device associations.
 *
 * Run after: npm start
 */

import { Commands } from '../system/core/shared/Commands';
import type { UserEntity } from '../system/user/entities/UserEntity';

async function main() {
	console.log('🗑️  Deleting all anonymous users...\n');

	// Get all users
	const usersResult = await Commands.execute('data/list', {
		collection: 'users',
		limit: 1000,
	});

	if (!usersResult.success || !usersResult.data) {
		console.error('❌ Failed to list users:', usersResult.error);
		process.exit(1);
	}

	const users = usersResult.data as UserEntity[];

	// Filter anonymous users (uniqueId starts with "anon-" or displayName is "Anonymous User")
	const anonymousUsers = users.filter(
		(u) => u.uniqueId?.startsWith('anon-') || u.displayName === 'Anonymous User'
	);

	console.log(`Found ${anonymousUsers.length} anonymous users to delete:\n`);

	if (anonymousUsers.length === 0) {
		console.log('✅ No anonymous users found!');
		process.exit(0);
	}

	// Show what will be deleted
	anonymousUsers.forEach((u) => {
		console.log(`  - ${u.displayName} (${u.uniqueId}) - ID: ${u.id.slice(0, 8)}...`);
	});

	console.log('\n🔄 Deleting...\n');

	let deleted = 0;
	let failed = 0;

	for (const user of anonymousUsers) {
		try {
			const result = await Commands.execute('data/delete', {
				collection: 'users',
				id: user.id,
			});

			if (result.success) {
				console.log(`  ✅ Deleted: ${user.displayName} (${user.id.slice(0, 8)}...)`);
				deleted++;
			} else {
				console.error(`  ❌ Failed: ${user.displayName} - ${result.error}`);
				failed++;
			}
		} catch (e: any) {
			console.error(`  ❌ Error deleting ${user.displayName}: ${e.message}`);
			failed++;
		}
	}

	console.log(`\n📊 Results:`);
	console.log(`  ✅ Deleted: ${deleted}`);
	console.log(`  ❌ Failed: ${failed}`);

	if (deleted > 0) {
		console.log('\n✅ Sessions for deleted users have been cleaned up automatically.');
		console.log('   Browser tabs will get fresh identities on next reload.');
	}

	process.exit(failed > 0 ? 1 : 0);
}

main().catch((e) => {
	console.error('❌ Script failed:', e);
	process.exit(1);
});
