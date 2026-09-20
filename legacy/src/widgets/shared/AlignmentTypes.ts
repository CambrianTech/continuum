/**
 * Generic Alignment System for Infinite Scroll Items
 *
 * Allows any infinite scroll implementation to define alignment logic
 * based on current user context and item data.
 */

/**
 * Alignment strategies for items in a list
 */
export type ItemAlignment = 'left' | 'right' | 'center' | 'full-width';

/**
 * Context needed to determine alignment
 */
export interface AlignmentContext<TUser = any> {
  readonly currentUserId: string;
  readonly currentUser?: TUser;
}

/**
 * Interface for determining item alignment based on context
 */
export interface AlignmentStrategy<TItem = any, TUser = any> {
  /**
   * Determine alignment for a specific item
   * @param item - The item to align
   * @param context - Current user context
   * @returns alignment strategy and any additional CSS classes
   */
  getAlignment(item: TItem, context: AlignmentContext<TUser>): {
    alignment: ItemAlignment;
    cssClasses: string[];
  };
}

/**
 * Chat-specific alignment strategy
 * Current user messages go right, others go left
 */
export class ChatMessageAlignment implements AlignmentStrategy<{ senderId: string }, any> {
  getAlignment(message: { senderId: string }, context: AlignmentContext): {
    alignment: ItemAlignment;
    cssClasses: string[];
  } {
    const isCurrentUser = message.senderId === context.currentUserId;
    return {
      alignment: isCurrentUser ? 'right' : 'left',
      cssClasses: [isCurrentUser ? 'current-user' : 'other-user']
    };
  }
}

/**
 * Generic left-aligned strategy (for user lists, file lists, etc.)
 */
export class LeftAlignedStrategy implements AlignmentStrategy<unknown, unknown> {
  getAlignment(): { alignment: ItemAlignment; cssClasses: string[] } {
    return {
      alignment: 'left',
      cssClasses: ['default-item']
    };
  }
}

/**
 * Generic center-aligned strategy (for notifications, status items, etc.)
 */
export class CenterAlignedStrategy implements AlignmentStrategy<unknown, unknown> {
  getAlignment(): { alignment: ItemAlignment; cssClasses: string[] } {
    return {
      alignment: 'center',
      cssClasses: ['centered-item']
    };
  }
}