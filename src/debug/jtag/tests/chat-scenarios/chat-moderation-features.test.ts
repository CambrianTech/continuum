#!/usr/bin/env tsx
/**
 * Chat Moderation Features Tests - Safety and Control Systems
 * 
 * Tests chat moderation capabilities essential for safe community management:
 * - User blocking and unblocking
 * - Message deletion and bulk moderation
 * - Admin and moderator controls
 * - Content filtering and auto-moderation
 * - Report system and investigation tools
 * - Timeout and ban management
 * - Role-based permissions
 */

import { JTAGClientServer } from '../../system/core/client/server/JTAGClientServer';
import type { ChatMessage } from '../../widgets/chat/shared/ChatModuleTypes';

console.log('🧪 CHAT MODERATION AND SAFETY FEATURES TESTS');

interface ModerationAction {
  readonly actionId: string;
  readonly actionType: 'block' | 'delete' | 'timeout' | 'ban' | 'warn' | 'filter';
  readonly targetUserId?: string;
  readonly targetMessageId?: string;
  readonly moderatorId: string;
  readonly reason: string;
  readonly timestamp: string;
  readonly duration?: number; // in seconds, for timeouts
  readonly severity: 'low' | 'medium' | 'high' | 'critical';
}

interface UserPermissions {
  readonly userId: string;
  readonly role: 'user' | 'moderator' | 'admin' | 'owner';
  readonly permissions: ReadonlyArray<string>;
  readonly restrictions: ReadonlyArray<string>;
}

interface ContentFilter {
  readonly filterId: string;
  readonly pattern: string | RegExp;
  readonly action: 'warn' | 'delete' | 'flag' | 'auto-timeout';
  readonly severity: 'low' | 'medium' | 'high';
  readonly enabled: boolean;
}

interface TestResults {
  blocking: Array<{ test: string; success: boolean; details: string }>;
  deletion: Array<{ test: string; success: boolean; details: string }>;
  adminControls: Array<{ test: string; success: boolean; details: string }>;
  filtering: Array<{ test: string; success: boolean; details: string }>;
  reporting: Array<{ test: string; success: boolean; details: string }>;
  timeouts: Array<{ test: string; success: boolean; details: string }>;
  permissions: Array<{ test: string; success: boolean; details: string }>;
}

class ChatModerationTest {
  private client: any;
  private testRoomId: string = 'moderation-test-room';
  private adminUserId: string = 'admin-user';
  private moderatorUserId: string = 'moderator-user';
  private regularUserId: string = 'regular-user';
  private troubleUserId: string = 'trouble-user';
  private results: TestResults = {
    blocking: [],
    deletion: [],
    adminControls: [],
    filtering: [],
    reporting: [],
    timeouts: [],
    permissions: []
  };

  async initialize(): Promise<void> {
    console.log('🔗 Connecting to JTAG system for chat moderation testing...');
    
    try {
      const result = await JTAGClientServer.connect();
      this.client = result.client;
      
      if (!result.listResult.success) {
        throw new Error('Failed to connect to JTAG system');
      }
      
      console.log(`✅ Connected to JTAG system with ${result.listResult.commands.length} commands`);
      
      // Set up test room with proper permissions
      await this.setupTestEnvironment();
      
    } catch (error) {
      console.error('❌ Failed to initialize chat moderation test:', error);
      throw error;
    }
  }

  private async setupTestEnvironment(): Promise<void> {
    console.log('🏗️ Setting up moderation test environment...');
    
    // Create test room
    await this.client.executeCommand('chat/create-room', {
      name: 'Moderation Test Room',
      roomId: this.testRoomId,
      category: 'testing',
      moderationEnabled: true
    });
    
    // Set up user roles
    await this.client.executeCommand('chat/set-user-role', {
      userId: this.adminUserId,
      roomId: this.testRoomId,
      role: 'admin'
    });
    
    await this.client.executeCommand('chat/set-user-role', {
      userId: this.moderatorUserId,
      roomId: this.testRoomId,
      role: 'moderator'
    });
  }

  /**
   * TEST 1: User Blocking and Unblocking
   */
  async testUserBlocking(): Promise<void> {
    console.log('\n🚫 TEST 1: User Blocking and Unblocking');
    
    try {
      // Regular user blocks troublesome user
      const blockUser = await this.client.executeCommand('chat/block-user', {
        userId: this.regularUserId,
        blockedUserId: this.troubleUserId,
        roomId: this.testRoomId,
        reason: 'Inappropriate behavior'
      });
      
      this.results.blocking.push({
        test: 'Block troublesome user',
        success: blockUser.success,
        details: blockUser.success ?
          `User ${this.troubleUserId} blocked successfully` :
          `Failed: ${blockUser.error || 'Unknown error'}`
      });
      
      // Verify blocked user cannot send messages to blocker
      const blockedMessage = await this.client.executeCommand('chat/send-message', {
        roomId: this.testRoomId,
        userId: this.troubleUserId,
        content: 'This message should be blocked',
        targetUserId: this.regularUserId
      });
      
      this.results.blocking.push({
        test: 'Blocked user message prevention',
        success: !blockedMessage.success || blockedMessage.blocked,
        details: !blockedMessage.success || blockedMessage.blocked ?
          `Blocked message properly prevented` :
          `Security issue: blocked user can still message`
      });
      
      // Get block list
      const blockList = await this.client.executeCommand('chat/get-blocked-users', {
        userId: this.regularUserId,
        roomId: this.testRoomId
      });
      
      this.results.blocking.push({
        test: 'Retrieve block list',
        success: blockList.success && blockList.blockedUsers?.includes(this.troubleUserId),
        details: blockList.success ?
          `Block list contains ${blockList.blockedUsers?.length || 0} users` :
          `Failed: ${blockList.error || 'Unknown error'}`
      });
      
      // Unblock user
      const unblockUser = await this.client.executeCommand('chat/unblock-user', {
        userId: this.regularUserId,
        unblockedUserId: this.troubleUserId,
        roomId: this.testRoomId
      });
      
      this.results.blocking.push({
        test: 'Unblock user',
        success: unblockUser.success,
        details: unblockUser.success ?
          `User ${this.troubleUserId} unblocked successfully` :
          `Failed: ${unblockUser.error || 'Unknown error'}`
      });
      
      // Verify unblocked user can send messages again
      const unblockedMessage = await this.client.executeCommand('chat/send-message', {
        roomId: this.testRoomId,
        userId: this.troubleUserId,
        content: 'This message should work after unblocking'
      });
      
      this.results.blocking.push({
        test: 'Unblocked user message restoration',
        success: unblockedMessage.success,
        details: unblockedMessage.success ?
          `Unblocked user can send messages again` :
          `Failed: ${unblockedMessage.error || 'Unknown error'}`
      });
      
    } catch (error) {
      this.results.blocking.push({
        test: 'User blocking test',
        success: false,
        details: `Exception: ${error instanceof Error ? error.message : 'Unknown error'}`
      });
    }
  }

  /**
   * TEST 2: Message Deletion and Bulk Moderation
   */
  async testMessageDeletion(): Promise<void> {
    console.log('\n🗑️ TEST 2: Message Deletion and Bulk Moderation');
    
    try {
      // Send problematic message
      const problematicMessage = await this.client.executeCommand('chat/send-message', {
        roomId: this.testRoomId,
        userId: this.troubleUserId,
        content: 'This is a problematic message that should be deleted'
      });
      
      this.results.deletion.push({
        test: 'Send problematic message',
        success: problematicMessage.success,
        details: problematicMessage.success ?
          `Problematic message sent: ${problematicMessage.messageId}` :
          `Failed: ${problematicMessage.error || 'Unknown error'}`
      });
      
      if (problematicMessage.success) {
        // Moderator deletes message
        const deleteMessage = await this.client.executeCommand('chat/delete-message', {
          messageId: problematicMessage.messageId,
          roomId: this.testRoomId,
          moderatorId: this.moderatorUserId,
          reason: 'Inappropriate content'
        });
        
        this.results.deletion.push({
          test: 'Moderator delete message',
          success: deleteMessage.success,
          details: deleteMessage.success ?
            `Message deleted by moderator` :
            `Failed: ${deleteMessage.error || 'Unknown error'}`
        });
        
        // Verify message is no longer visible
        const getMessage = await this.client.executeCommand('chat/get-message', {
          messageId: problematicMessage.messageId,
          roomId: this.testRoomId
        });
        
        this.results.deletion.push({
          test: 'Verify message deletion',
          success: !getMessage.success || getMessage.message?.deleted === true,
          details: !getMessage.success || getMessage.message?.deleted ?
            `Message properly deleted from view` :
            `Issue: deleted message still visible`
        });
      }
      
      // Send multiple problematic messages for bulk deletion
      const bulkMessageIds: string[] = [];
      for (let i = 0; i < 3; i++) {
        const bulkMessage = await this.client.executeCommand('chat/send-message', {
          roomId: this.testRoomId,
          userId: this.troubleUserId,
          content: `Problematic bulk message ${i + 1}`
        });
        
        if (bulkMessage.success) {
          bulkMessageIds.push(bulkMessage.messageId);
        }
      }
      
      this.results.deletion.push({
        test: 'Send multiple problematic messages',
        success: bulkMessageIds.length === 3,
        details: `Sent ${bulkMessageIds.length}/3 messages for bulk deletion test`
      });
      
      // Bulk delete messages
      if (bulkMessageIds.length > 0) {
        const bulkDelete = await this.client.executeCommand('chat/bulk-delete-messages', {
          messageIds: bulkMessageIds,
          roomId: this.testRoomId,
          moderatorId: this.adminUserId,
          reason: 'Spam cleanup'
        });
        
        this.results.deletion.push({
          test: 'Bulk delete messages',
          success: bulkDelete.success,
          details: bulkDelete.success ?
            `Bulk deleted ${bulkMessageIds.length} messages` :
            `Failed: ${bulkDelete.error || 'Unknown error'}`
        });
      }
      
      // Test unauthorized deletion (regular user trying to delete)
      const anotherMessage = await this.client.executeCommand('chat/send-message', {
        roomId: this.testRoomId,
        userId: this.regularUserId,
        content: 'A normal message'
      });
      
      if (anotherMessage.success) {
        const unauthorizedDelete = await this.client.executeCommand('chat/delete-message', {
          messageId: anotherMessage.messageId,
          roomId: this.testRoomId,
          moderatorId: this.troubleUserId, // Non-moderator trying to delete
          reason: 'Unauthorized attempt'
        });
        
        this.results.deletion.push({
          test: 'Unauthorized deletion (should fail)',
          success: !unauthorizedDelete.success,
          details: !unauthorizedDelete.success ?
            `Unauthorized deletion properly rejected` :
            `Security issue: non-moderator can delete messages`
        });
      }
      
    } catch (error) {
      this.results.deletion.push({
        test: 'Message deletion test',
        success: false,
        details: `Exception: ${error instanceof Error ? error.message : 'Unknown error'}`
      });
    }
  }

  /**
   * TEST 3: Admin and Moderator Controls
   */
  async testAdminControls(): Promise<void> {
    console.log('\n👮 TEST 3: Admin and Moderator Controls');
    
    try {
      // Admin promotes user to moderator
      const promoteModerator = await this.client.executeCommand('chat/set-user-role', {
        userId: this.regularUserId,
        roomId: this.testRoomId,
        role: 'moderator',
        adminId: this.adminUserId
      });
      
      this.results.adminControls.push({
        test: 'Promote user to moderator',
        success: promoteModerator.success,
        details: promoteModerator.success ?
          `User promoted to moderator` :
          `Failed: ${promoteModerator.error || 'Unknown error'}`
      });
      
      // Check user permissions after promotion
      const checkPermissions = await this.client.executeCommand('chat/get-user-permissions', {
        userId: this.regularUserId,
        roomId: this.testRoomId
      });
      
      this.results.adminControls.push({
        test: 'Verify moderator permissions',
        success: checkPermissions.success && checkPermissions.role === 'moderator',
        details: checkPermissions.success ?
          `User role: ${checkPermissions.role} with ${checkPermissions.permissions?.length || 0} permissions` :
          `Failed: ${checkPermissions.error || 'Unknown error'}`
      });
      
      // Set room-level moderation settings
      const roomSettings = await this.client.executeCommand('chat/update-room-settings', {
        roomId: this.testRoomId,
        adminId: this.adminUserId,
        settings: {
          slowMode: 30, // 30 second cooldown
          requireApproval: true,
          autoModeration: true,
          maxMessageLength: 2000
        }
      });
      
      this.results.adminControls.push({
        test: 'Update room moderation settings',
        success: roomSettings.success,
        details: roomSettings.success ?
          `Room settings updated successfully` :
          `Failed: ${roomSettings.error || 'Unknown error'}`
      });
      
      // Admin views moderation log
      const moderationLog = await this.client.executeCommand('chat/get-moderation-log', {
        roomId: this.testRoomId,
        adminId: this.adminUserId,
        limit: 50
      });
      
      this.results.adminControls.push({
        test: 'View moderation log',
        success: moderationLog.success,
        details: moderationLog.success ?
          `Moderation log contains ${moderationLog.actions?.length || 0} actions` :
          `Failed: ${moderationLog.error || 'Unknown error'}`
      });
      
      // Admin demotes moderator
      const demoteModerator = await this.client.executeCommand('chat/set-user-role', {
        userId: this.regularUserId,
        roomId: this.testRoomId,
        role: 'user',
        adminId: this.adminUserId
      });
      
      this.results.adminControls.push({
        test: 'Demote moderator to user',
        success: demoteModerator.success,
        details: demoteModerator.success ?
          `Moderator demoted to regular user` :
          `Failed: ${demoteModerator.error || 'Unknown error'}`
      });
      
    } catch (error) {
      this.results.adminControls.push({
        test: 'Admin controls test',
        success: false,
        details: `Exception: ${error instanceof Error ? error.message : 'Unknown error'}`
      });
    }
  }

  /**
   * TEST 4: Content Filtering and Auto-Moderation
   */
  async testContentFiltering(): Promise<void> {
    console.log('\n🤖 TEST 4: Content Filtering and Auto-Moderation');
    
    try {
      // Set up content filter
      const setupFilter = await this.client.executeCommand('chat/create-content-filter', {
        roomId: this.testRoomId,
        adminId: this.adminUserId,
        filter: {
          name: 'Profanity Filter',
          pattern: '\\b(badword1|badword2|spam)\\b',
          action: 'delete',
          severity: 'medium',
          enabled: true
        }
      });
      
      this.results.filtering.push({
        test: 'Create content filter',
        success: setupFilter.success,
        details: setupFilter.success ?
          `Content filter created: ${setupFilter.filterId}` :
          `Failed: ${setupFilter.error || 'Unknown error'}`
      });
      
      // Send message that triggers filter
      const filteredMessage = await this.client.executeCommand('chat/send-message', {
        roomId: this.testRoomId,
        userId: this.troubleUserId,
        content: 'This message contains badword1 and should be filtered'
      });
      
      this.results.filtering.push({
        test: 'Send message triggering filter',
        success: !filteredMessage.success || filteredMessage.filtered === true,
        details: !filteredMessage.success || filteredMessage.filtered ?
          `Message properly filtered by auto-moderation` :
          `Issue: filtered message was allowed through`
      });
      
      // Set up spam detection
      const spamFilter = await this.client.executeCommand('chat/create-content-filter', {
        roomId: this.testRoomId,
        adminId: this.adminUserId,
        filter: {
          name: 'Spam Detection',
          pattern: 'rapid_repeat',
          action: 'auto-timeout',
          severity: 'high',
          enabled: true,
          parameters: {
            maxMessagesPerMinute: 10,
            similarityThreshold: 0.8
          }
        }
      });
      
      this.results.filtering.push({
        test: 'Create spam filter',
        success: spamFilter.success,
        details: spamFilter.success ?
          `Spam filter created: ${spamFilter.filterId}` :
          `Failed: ${spamFilter.error || 'Unknown error'}`
      });
      
      // Simulate spam (rapid messages)
      let spamDetected = false;
      for (let i = 0; i < 12; i++) {
        const spamMessage = await this.client.executeCommand('chat/send-message', {
          roomId: this.testRoomId,
          userId: this.troubleUserId,
          content: `Spam message number ${i}`
        });
        
        if (!spamMessage.success && spamMessage.reason === 'spam_detected') {
          spamDetected = true;
          break;
        }
        
        // Small delay to simulate rapid sending
        await new Promise(resolve => setTimeout(resolve, 50));
      }
      
      this.results.filtering.push({
        test: 'Spam detection activation',
        success: spamDetected,
        details: spamDetected ?
          `Spam properly detected and prevented` :
          `Issue: spam detection did not activate`
      });
      
      // Get filter statistics
      const filterStats = await this.client.executeCommand('chat/get-filter-stats', {
        roomId: this.testRoomId,
        adminId: this.adminUserId
      });
      
      this.results.filtering.push({
        test: 'View filter statistics',
        success: filterStats.success,
        details: filterStats.success ?
          `Filter stats: ${filterStats.totalFiltered || 0} messages filtered` :
          `Failed: ${filterStats.error || 'Unknown error'}`
      });
      
      // Disable filter
      if (setupFilter.success) {
        const disableFilter = await this.client.executeCommand('chat/update-content-filter', {
          filterId: setupFilter.filterId,
          roomId: this.testRoomId,
          adminId: this.adminUserId,
          enabled: false
        });
        
        this.results.filtering.push({
          test: 'Disable content filter',
          success: disableFilter.success,
          details: disableFilter.success ?
            `Content filter disabled` :
            `Failed: ${disableFilter.error || 'Unknown error'}`
        });
      }
      
    } catch (error) {
      this.results.filtering.push({
        test: 'Content filtering test',
        success: false,
        details: `Exception: ${error instanceof Error ? error.message : 'Unknown error'}`
      });
    }
  }

  /**
   * TEST 5: Report System and Investigation Tools
   */
  async testReportSystem(): Promise<void> {
    console.log('\n🚨 TEST 5: Report System and Investigation Tools');
    
    try {
      // Send message that will be reported
      const reportableMessage = await this.client.executeCommand('chat/send-message', {
        roomId: this.testRoomId,
        userId: this.troubleUserId,
        content: 'This is a message that will be reported for harassment'
      });
      
      this.results.reporting.push({
        test: 'Send reportable message',
        success: reportableMessage.success,
        details: reportableMessage.success ?
          `Reportable message sent: ${reportableMessage.messageId}` :
          `Failed: ${reportableMessage.error || 'Unknown error'}`
      });
      
      if (reportableMessage.success) {
        // Regular user reports the message
        const reportMessage = await this.client.executeCommand('chat/report-message', {
          messageId: reportableMessage.messageId,
          roomId: this.testRoomId,
          reporterId: this.regularUserId,
          reason: 'harassment',
          category: 'inappropriate_behavior',
          description: 'This message contains harassment and should be reviewed'
        });
        
        this.results.reporting.push({
          test: 'Report message',
          success: reportMessage.success,
          details: reportMessage.success ?
            `Message reported: ${reportMessage.reportId}` :
            `Failed: ${reportMessage.error || 'Unknown error'}`
        });
        
        // Another user also reports the same message
        const secondReport = await this.client.executeCommand('chat/report-message', {
          messageId: reportableMessage.messageId,
          roomId: this.testRoomId,
          reporterId: 'another-user',
          reason: 'harassment',
          category: 'inappropriate_behavior',
          description: 'I also find this message inappropriate'
        });
        
        this.results.reporting.push({
          test: 'Multiple reports on same message',
          success: secondReport.success,
          details: secondReport.success ?
            `Second report filed: ${secondReport.reportId}` :
            `Failed: ${secondReport.error || 'Unknown error'}`
        });
      }
      
      // Report user for overall behavior
      const reportUser = await this.client.executeCommand('chat/report-user', {
        userId: this.troubleUserId,
        roomId: this.testRoomId,
        reporterId: this.regularUserId,
        reason: 'repeated_harassment',
        category: 'behavioral_issues',
        description: 'This user consistently engages in harassment'
      });
      
      this.results.reporting.push({
        test: 'Report user behavior',
        success: reportUser.success,
        details: reportUser.success ?
          `User reported: ${reportUser.reportId}` :
          `Failed: ${reportUser.error || 'Unknown error'}`
      });
      
      // Moderator views reports queue
      const reportsQueue = await this.client.executeCommand('chat/get-reports-queue', {
        roomId: this.testRoomId,
        moderatorId: this.moderatorUserId,
        status: 'pending',
        limit: 20
      });
      
      this.results.reporting.push({
        test: 'View reports queue',
        success: reportsQueue.success && reportsQueue.reports?.length > 0,
        details: reportsQueue.success ?
          `Reports queue contains ${reportsQueue.reports?.length || 0} pending reports` :
          `Failed: ${reportsQueue.error || 'Unknown error'}`
      });
      
      // Moderator investigates report
      if (reportsQueue.success && reportsQueue.reports?.length > 0) {
        const investigateReport = await this.client.executeCommand('chat/investigate-report', {
          reportId: reportsQueue.reports[0].reportId,
          moderatorId: this.moderatorUserId,
          roomId: this.testRoomId,
          action: 'under_review'
        });
        
        this.results.reporting.push({
          test: 'Investigate report',
          success: investigateReport.success,
          details: investigateReport.success ?
            `Report investigation started` :
            `Failed: ${investigateReport.error || 'Unknown error'}`
        });
        
        // Resolve report
        const resolveReport = await this.client.executeCommand('chat/resolve-report', {
          reportId: reportsQueue.reports[0].reportId,
          moderatorId: this.moderatorUserId,
          roomId: this.testRoomId,
          resolution: 'confirmed',
          action: 'message_deleted',
          notes: 'Message violated community guidelines'
        });
        
        this.results.reporting.push({
          test: 'Resolve report',
          success: resolveReport.success,
          details: resolveReport.success ?
            `Report resolved with action taken` :
            `Failed: ${resolveReport.error || 'Unknown error'}`
        });
      }
      
    } catch (error) {
      this.results.reporting.push({
        test: 'Report system test',
        success: false,
        details: `Exception: ${error instanceof Error ? error.message : 'Unknown error'}`
      });
    }
  }

  /**
   * TEST 6: Timeout and Ban Management
   */
  async testTimeoutsAndBans(): Promise<void> {
    console.log('\n⏰ TEST 6: Timeout and Ban Management');
    
    try {
      // Moderator gives user a timeout
      const timeoutUser = await this.client.executeCommand('chat/timeout-user', {
        userId: this.troubleUserId,
        roomId: this.testRoomId,
        moderatorId: this.moderatorUserId,
        duration: 300, // 5 minutes
        reason: 'Disruptive behavior'
      });
      
      this.results.timeouts.push({
        test: 'Timeout user',
        success: timeoutUser.success,
        details: timeoutUser.success ?
          `User timed out for 5 minutes` :
          `Failed: ${timeoutUser.error || 'Unknown error'}`
      });
      
      // Verify timed out user cannot send messages
      const timeoutMessage = await this.client.executeCommand('chat/send-message', {
        roomId: this.testRoomId,
        userId: this.troubleUserId,
        content: 'This message should be blocked due to timeout'
      });
      
      this.results.timeouts.push({
        test: 'Timed out user message prevention',
        success: !timeoutMessage.success || timeoutMessage.reason === 'user_timed_out',
        details: !timeoutMessage.success || timeoutMessage.reason === 'user_timed_out' ?
          `Timed out user properly prevented from messaging` :
          `Issue: timed out user can still send messages`
      });
      
      // Check timeout status
      const timeoutStatus = await this.client.executeCommand('chat/get-user-status', {
        userId: this.troubleUserId,
        roomId: this.testRoomId
      });
      
      this.results.timeouts.push({
        test: 'Check timeout status',
        success: timeoutStatus.success && timeoutStatus.status === 'timed_out',
        details: timeoutStatus.success ?
          `User status: ${timeoutStatus.status}, expires: ${timeoutStatus.expiresAt || 'unknown'}` :
          `Failed: ${timeoutStatus.error || 'Unknown error'}`
      });
      
      // Remove timeout early
      const removeTimeout = await this.client.executeCommand('chat/remove-timeout', {
        userId: this.troubleUserId,
        roomId: this.testRoomId,
        moderatorId: this.moderatorUserId,
        reason: 'Early release for good behavior'
      });
      
      this.results.timeouts.push({
        test: 'Remove timeout early',
        success: removeTimeout.success,
        details: removeTimeout.success ?
          `Timeout removed successfully` :
          `Failed: ${removeTimeout.error || 'Unknown error'}`
      });
      
      // Admin bans user
      const banUser = await this.client.executeCommand('chat/ban-user', {
        userId: this.troubleUserId,
        roomId: this.testRoomId,
        adminId: this.adminUserId,
        duration: 86400, // 24 hours
        reason: 'Repeated violations',
        banType: 'temporary'
      });
      
      this.results.timeouts.push({
        test: 'Ban user temporarily',
        success: banUser.success,
        details: banUser.success ?
          `User banned for 24 hours` :
          `Failed: ${banUser.error || 'Unknown error'}`
      });
      
      // Verify banned user cannot join room
      const bannedJoin = await this.client.executeCommand('chat/join-room', {
        roomId: this.testRoomId,
        userId: this.troubleUserId
      });
      
      this.results.timeouts.push({
        test: 'Banned user join prevention',
        success: !bannedJoin.success || bannedJoin.reason === 'user_banned',
        details: !bannedJoin.success || bannedJoin.reason === 'user_banned' ?
          `Banned user properly prevented from joining` :
          `Security issue: banned user can join room`
      });
      
      // Get ban list
      const banList = await this.client.executeCommand('chat/get-ban-list', {
        roomId: this.testRoomId,
        adminId: this.adminUserId
      });
      
      this.results.timeouts.push({
        test: 'Retrieve ban list',
        success: banList.success && banList.bannedUsers?.includes(this.troubleUserId),
        details: banList.success ?
          `Ban list contains ${banList.bannedUsers?.length || 0} users` :
          `Failed: ${banList.error || 'Unknown error'}`
      });
      
      // Unban user
      const unbanUser = await this.client.executeCommand('chat/unban-user', {
        userId: this.troubleUserId,
        roomId: this.testRoomId,
        adminId: this.adminUserId,
        reason: 'Ban period completed'
      });
      
      this.results.timeouts.push({
        test: 'Unban user',
        success: unbanUser.success,
        details: unbanUser.success ?
          `User unbanned successfully` :
          `Failed: ${unbanUser.error || 'Unknown error'}`
      });
      
    } catch (error) {
      this.results.timeouts.push({
        test: 'Timeouts and bans test',
        success: false,
        details: `Exception: ${error instanceof Error ? error.message : 'Unknown error'}`
      });
    }
  }

  /**
   * TEST 7: Role-Based Permissions
   */
  async testRoleBasedPermissions(): Promise<void> {
    console.log('\n🔐 TEST 7: Role-Based Permissions');
    
    try {
      // Test user permissions
      const userPermissions = await this.client.executeCommand('chat/check-permission', {
        userId: this.regularUserId,
        roomId: this.testRoomId,
        permission: 'delete_messages'
      });
      
      this.results.permissions.push({
        test: 'Check user delete permission (should be false)',
        success: userPermissions.success && !userPermissions.hasPermission,
        details: userPermissions.success ?
          `User delete permission: ${userPermissions.hasPermission}` :
          `Failed: ${userPermissions.error || 'Unknown error'}`
      });
      
      // Test moderator permissions
      const moderatorPermissions = await this.client.executeCommand('chat/check-permission', {
        userId: this.moderatorUserId,
        roomId: this.testRoomId,
        permission: 'delete_messages'
      });
      
      this.results.permissions.push({
        test: 'Check moderator delete permission (should be true)',
        success: moderatorPermissions.success && moderatorPermissions.hasPermission,
        details: moderatorPermissions.success ?
          `Moderator delete permission: ${moderatorPermissions.hasPermission}` :
          `Failed: ${moderatorPermissions.error || 'Unknown error'}`
      });
      
      // Test admin permissions
      const adminPermissions = await this.client.executeCommand('chat/check-permission', {
        userId: this.adminUserId,
        roomId: this.testRoomId,
        permission: 'ban_users'
      });
      
      this.results.permissions.push({
        test: 'Check admin ban permission (should be true)',
        success: adminPermissions.success && adminPermissions.hasPermission,
        details: adminPermissions.success ?
          `Admin ban permission: ${adminPermissions.hasPermission}` :
          `Failed: ${adminPermissions.error || 'Unknown error'}`
      });
      
      // Create custom role with specific permissions
      const customRole = await this.client.executeCommand('chat/create-role', {
        roomId: this.testRoomId,
        adminId: this.adminUserId,
        role: {
          name: 'helper',
          permissions: ['delete_own_messages', 'use_reactions', 'send_messages'],
          restrictions: ['cannot_mention_all']
        }
      });
      
      this.results.permissions.push({
        test: 'Create custom role',
        success: customRole.success,
        details: customRole.success ?
          `Custom role 'helper' created` :
          `Failed: ${customRole.error || 'Unknown error'}`
      });
      
      // Assign custom role to user
      const assignRole = await this.client.executeCommand('chat/set-user-role', {
        userId: this.regularUserId,
        roomId: this.testRoomId,
        role: 'helper',
        adminId: this.adminUserId
      });
      
      this.results.permissions.push({
        test: 'Assign custom role',
        success: assignRole.success,
        details: assignRole.success ?
          `Custom role assigned to user` :
          `Failed: ${assignRole.error || 'Unknown error'}`
      });
      
      // Test permission inheritance
      const inheritedPermission = await this.client.executeCommand('chat/check-permission', {
        userId: this.regularUserId,
        roomId: this.testRoomId,
        permission: 'delete_own_messages'
      });
      
      this.results.permissions.push({
        test: 'Check inherited permission',
        success: inheritedPermission.success && inheritedPermission.hasPermission,
        details: inheritedPermission.success ?
          `Inherited permission working: ${inheritedPermission.hasPermission}` :
          `Failed: ${inheritedPermission.error || 'Unknown error'}`
      });
      
      // Test permission restriction
      const restrictedPermission = await this.client.executeCommand('chat/check-permission', {
        userId: this.regularUserId,
        roomId: this.testRoomId,
        permission: 'mention_all'
      });
      
      this.results.permissions.push({
        test: 'Check restricted permission (should be false)',
        success: restrictedPermission.success && !restrictedPermission.hasPermission,
        details: restrictedPermission.success ?
          `Restriction working: ${restrictedPermission.hasPermission}` :
          `Failed: ${restrictedPermission.error || 'Unknown error'}`
      });
      
    } catch (error) {
      this.results.permissions.push({
        test: 'Role-based permissions test',
        success: false,
        details: `Exception: ${error instanceof Error ? error.message : 'Unknown error'}`
      });
    }
  }

  /**
   * Run all moderation tests
   */
  async runAllTests(): Promise<void> {
    await this.initialize();
    
    await this.testUserBlocking();
    await this.testMessageDeletion();
    await this.testAdminControls();
    await this.testContentFiltering();
    await this.testReportSystem();
    await this.testTimeoutsAndBans();
    await this.testRoleBasedPermissions();
  }

  /**
   * Display comprehensive test results
   */
  displayResults(): void {
    console.log(`\n🎯 CHAT MODERATION AND SAFETY FEATURES TEST RESULTS`);
    console.log('=====================================================');
    
    const categories = [
      { name: 'User Blocking', tests: this.results.blocking, icon: '🚫' },
      { name: 'Message Deletion', tests: this.results.deletion, icon: '🗑️' },
      { name: 'Admin Controls', tests: this.results.adminControls, icon: '👮' },
      { name: 'Content Filtering', tests: this.results.filtering, icon: '🤖' },
      { name: 'Report System', tests: this.results.reporting, icon: '🚨' },
      { name: 'Timeouts & Bans', tests: this.results.timeouts, icon: '⏰' },
      { name: 'Role Permissions', tests: this.results.permissions, icon: '🔐' }
    ];
    
    let totalTests = 0;
    let totalPassed = 0;
    
    categories.forEach(category => {
      const passed = category.tests.filter(test => test.success).length;
      const total = category.tests.length;
      totalTests += total;
      totalPassed += passed;
      
      console.log(`\n${category.icon} ${category.name.toUpperCase()}: ${passed}/${total} tests passed`);
      category.tests.forEach(test => {
        const status = test.success ? '✅' : '❌';
        console.log(`  ${status} ${test.test}: ${test.details}`);
      });
    });
    
    console.log(`\n📊 OVERALL SUMMARY: ${totalPassed}/${totalTests} moderation tests passed`);
    console.log(`Success Rate: ${((totalPassed / totalTests) * 100).toFixed(1)}%`);
    
    if (totalPassed === totalTests) {
      console.log('🎉 ✅ ALL CHAT MODERATION AND SAFETY FEATURES WORKING!');
      console.log('🛡️ Chat system has comprehensive safety and moderation tools');
      console.log('👮 Ready for safe community management at scale');
      console.log('⚖️ Proper role-based access controls implemented');
    } else {
      console.log('⚠️  Some moderation features need implementation');
      console.log('🔧 Focus on failing tests to ensure community safety');
      console.log('🛡️ These features are critical for preventing abuse');
      console.log('📋 Essential for any public-facing chat application');
    }
  }
}

// Main execution
async function runChatModerationTests(): Promise<void> {
  const testRunner = new ChatModerationTest();
  
  try {
    await testRunner.runAllTests();
    testRunner.displayResults();
    
  } catch (error) {
    console.error('💥 Chat moderation test execution failed:', error);
    process.exit(1);
  }
}

// Run tests if this file is executed directly
if (require.main === module) {
  runChatModerationTests().catch(error => {
    console.error('💥 Test runner failed:', error);
    process.exit(1);
  });
}

export { runChatModerationTests, ChatModerationTest };