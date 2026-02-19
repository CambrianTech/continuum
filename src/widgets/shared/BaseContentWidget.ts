/**
 * BaseContentWidget - Dynamic Content-Aware Widget Foundation
 *
 * Extends BaseWidget with dynamic content management capabilities
 * Enables widgets to respond to content switching, maintain context, and sync state
 * Provides the contract for content-driven UI components
 */

import { BaseWidget, type WidgetConfig } from './BaseWidget';
import { EventsDaemonBrowser } from '../../daemons/events-daemon/browser/EventsDaemonBrowser';
import type { UUID } from '../../system/core/types/CrossPlatformUUID';
import type {
  ContentItem,
  ContentType
} from '../../system/data/entities/UserStateEntity';
import type { ContentTypeEntity } from '../../system/data/entities/ContentTypeEntity';
import type { ContentService } from '../../system/core/client/shared/services/ContentService';

// Content widget specific configuration
export interface ContentWidgetConfig extends WidgetConfig {
  // Content management settings
  supportedContentTypes: ContentType[];    // Which content types this widget can display
  allowMultipleContent?: boolean;         // Can display multiple content items simultaneously
  autoSwitchContent?: boolean;           // Should automatically switch when new content opens
  persistContentState?: boolean;        // Should remember scroll position, filters, etc.

  // Context requirements
  requiresEntityId?: boolean;           // Does this widget need an entityId to function
  requiresContentType?: boolean;       // Does this widget need contentType metadata

  // Integration settings
  enableContentEvents?: boolean;       // Listen for content switching events
  enableStateSync?: boolean;          // Sync state changes back to ContentService
}

// Content widget state extends base widget state
export interface ContentWidgetState {
  currentContent?: ContentItem;         // Currently displayed content
  availableContent: ContentItem[];     // All content available to this widget
  contentMetadata: Record<string, unknown>; // Widget-specific content metadata
  lastContentUpdate: string;           // When content was last changed
}

// Content change event types
export type ContentEventName =
  | 'content:opened'
  | 'content:closed'
  | 'content:switched'
  | 'content:updated'
  | 'content:state-changed';

export interface ContentEventData {
  contentItem: ContentItem;
  previousContent?: ContentItem;
  source: 'user' | 'system' | 'widget';
  metadata?: Record<string, unknown>;
}

/**
 * BaseContentWidget - Foundation for all content-aware widgets
 *
 * Provides automatic content management, state persistence, and event coordination
 * Subclasses just implement content rendering and don't worry about the plumbing
 */
export abstract class BaseContentWidget extends BaseWidget {
  protected contentConfig: ContentWidgetConfig;
  protected contentState: ContentWidgetState;
  protected contentService?: ContentService;

  constructor(config: Partial<ContentWidgetConfig> = {}) {
    super(config);

    // Default content configuration - merge with base config
    const baseConfig = config as WidgetConfig;
    this.contentConfig = {
      widgetId: baseConfig.widgetId || 'content-widget',
      widgetName: baseConfig.widgetName || 'ContentWidget',
      supportedContentTypes: ['chat'], // Default to chat content
      allowMultipleContent: false,
      autoSwitchContent: true,
      persistContentState: true,
      requiresEntityId: true,
      requiresContentType: true,
      enableContentEvents: true,
      enableStateSync: true,
      ...config
    };

    // Initialize content state
    this.contentState = {
      availableContent: [],
      contentMetadata: {},
      lastContentUpdate: new Date().toISOString()
    };
  }

  /**
   * Override BaseWidget initialization to add content management
   */
  protected async onWidgetInitialize(): Promise<void> {
    this.verbose() && console.log(`🎯 ${this.config.widgetName}: Initializing content management...`);

    try {
      // Initialize content service (this would get from JTAGClient in real implementation)
      await this.initializeContentService();

      // Set up content event listeners
      if (this.contentConfig.enableContentEvents) {
        this.setupContentEventListeners();
      }

      // Load initial content state
      await this.loadInitialContent();

      // Let subclass initialize its content-specific logic
      await this.onContentInitialize();

      this.verbose() && console.log(`✅ ${this.config.widgetName}: Content management initialized`);
    } catch (error) {
      console.error(`❌ ${this.config.widgetName}: Content initialization failed:`, error);
      throw error;
    }
  }

  /**
   * Override BaseWidget cleanup to handle content cleanup
   */
  protected async onWidgetCleanup(): Promise<void> {
    this.verbose() && console.log(`🎯 ${this.config.widgetName}: Cleaning up content management...`);

    try {
      // Persist current content state
      if (this.contentConfig.persistContentState) {
        await this.persistContentState();
      }

      // Let subclass clean up its content-specific resources
      await this.onContentCleanup();

      // Clear content state
      this.contentState.availableContent = [];
      this.contentState.currentContent = undefined;

      this.verbose() && console.log(`✅ ${this.config.widgetName}: Content cleanup complete`);
    } catch (error) {
      console.error(`❌ ${this.config.widgetName}: Content cleanup failed:`, error);
    }
  }

  // === ABSTRACT CONTENT METHODS - Subclasses must implement ===

  /**
   * Initialize content-specific logic (called after content service is ready)
   */
  protected abstract onContentInitialize(): Promise<void>;

  /**
   * Render content in the widget (called when content changes)
   */
  protected abstract renderContent(contentItem: ContentItem): Promise<void>;

  /**
   * Handle content switching (called when user switches to different content)
   */
  protected abstract onContentSwitched(newContent: ContentItem, previousContent?: ContentItem): Promise<void>;

  /**
   * Clean up content-specific resources
   */
  protected abstract onContentCleanup(): Promise<void>;

  // === CONTENT MANAGEMENT METHODS - Available to subclasses ===

  /**
   * Get currently displayed content
   */
  protected getCurrentContent(): ContentItem | undefined {
    return this.contentState.currentContent;
  }

  /**
   * Get all available content for this widget
   */
  protected getAvailableContent(): ContentItem[] {
    return this.contentState.availableContent.filter(content =>
      this.contentConfig.supportedContentTypes.includes(content.type)
    );
  }

  /**
   * Switch to specific content by ID
   */
  protected async switchToContent(contentId: UUID): Promise<boolean> {
    const content = this.contentState.availableContent.find(c => c.id === contentId);
    if (!content) {
      console.warn(`⚠️ ${this.config.widgetName}: Content ${contentId} not found`);
      return false;
    }

    const previousContent = this.contentState.currentContent;
    this.contentState.currentContent = content;
    this.contentState.lastContentUpdate = new Date().toISOString();

    try {
      // Render new content
      await this.renderContent(content);

      // Notify subclass of content switch
      await this.onContentSwitched(content, previousContent);

      // Emit content switched event
      await this.emitContentEvent('content:switched', {
        contentItem: content,
        previousContent,
        source: 'widget'
      });

      // Sync state back to service if enabled
      if (this.contentConfig.enableStateSync && this.contentService) {
        // This would update the user's content state in the database
        await this.syncContentState();
      }

      return true;
    } catch (error) {
      console.error(`❌ ${this.config.widgetName}: Content switch failed:`, error);
      // Restore previous content on error
      this.contentState.currentContent = previousContent;
      return false;
    }
  }

  /**
   * Update content metadata (scroll position, filters, etc.)
   */
  protected async updateContentMetadata(contentId: UUID, metadata: Record<string, unknown>): Promise<boolean> {
    const content = this.contentState.availableContent.find(c => c.id === contentId);
    if (!content) {
      return false;
    }

    // Merge metadata
    content.metadata = { ...content.metadata, ...metadata };

    // Store in widget state
    this.contentState.contentMetadata[contentId] = content.metadata;

    // Sync if enabled
    if (this.contentConfig.enableStateSync) {
      await this.syncContentState();
    }

    return true;
  }

  /**
   * Check if widget supports specific content type
   */
  protected supportsContentType(contentType: ContentType): boolean {
    return this.contentConfig.supportedContentTypes.includes(contentType);
  }

  /**
   * Get content type configuration
   */
  protected async getContentTypeConfig(contentType: string): Promise<ContentTypeEntity | null> {
    if (!this.contentService) {
      return null;
    }

    return await this.contentService.getContentTypeConfig(contentType);
  }

  // === PRIVATE CONTENT METHODS ===

  /**
   * Initialize content service connection
   */
  private async initializeContentService(): Promise<void> {
    // In real implementation, this would get ContentService from JTAGClient
    // For now, we'll prepare the interface
    this.verbose() && console.log(`🎯 ${this.config.widgetName}: Content service would be initialized here`);

    // TODO: Get from JTAGClient when service integration is complete
    // const client = await JTAGClient.sharedInstance;
    // this.contentService = client.services.content;
  }

  /**
   * Set up event listeners for content changes
   */
  private setupContentEventListeners(): void {
    this.verbose() && console.log(`🎯 ${this.config.widgetName}: Setting up content event listeners...`);

    // Register DOM interest for content events (filter-first pattern)
    EventsDaemonBrowser.registerDOMInterest('content:switched');
    EventsDaemonBrowser.registerDOMInterest('content:opened');
    EventsDaemonBrowser.registerDOMInterest('content:closed');

    // Listen for global content events
    document.addEventListener('content:switched', (event: Event) => {
      const customEvent = event as Event & { detail: ContentEventData };
      this.handleContentEvent('content:switched', customEvent.detail);
    });

    document.addEventListener('content:opened', (event: Event) => {
      const customEvent = event as Event & { detail: ContentEventData };
      this.handleContentEvent('content:opened', customEvent.detail);
    });

    document.addEventListener('content:closed', (event: Event) => {
      const customEvent = event as Event & { detail: ContentEventData };
      this.handleContentEvent('content:closed', customEvent.detail);
    });
  }

  /**
   * Handle incoming content events
   */
  private async handleContentEvent(eventName: ContentEventName, data: ContentEventData): Promise<void> {
    this.verbose() && console.log(`🎯 ${this.config.widgetName}: Received content event ${eventName}:`, data);

    // Check if this content is relevant to this widget
    if (!this.supportsContentType(data.contentItem.type)) {
      return;
    }

    switch (eventName) {
      case 'content:opened':
        await this.handleContentOpened(data);
        break;
      case 'content:closed':
        await this.handleContentClosed(data);
        break;
      case 'content:switched':
        await this.handleContentSwitched(data);
        break;
    }
  }

  /**
   * Handle content opened event
   */
  private async handleContentOpened(data: ContentEventData): Promise<void> {
    // Add to available content if not already there
    const existing = this.contentState.availableContent.find(c => c.id === data.contentItem.id);
    if (!existing) {
      this.contentState.availableContent.push(data.contentItem);
    }

    // Auto-switch if configured and no current content
    if (this.contentConfig.autoSwitchContent && !this.contentState.currentContent) {
      await this.switchToContent(data.contentItem.id);
    }
  }

  /**
   * Handle content closed event
   */
  private async handleContentClosed(data: ContentEventData): Promise<void> {
    // Remove from available content
    this.contentState.availableContent = this.contentState.availableContent.filter(
      c => c.id !== data.contentItem.id
    );

    // If this was current content, switch to another or clear
    if (this.contentState.currentContent?.id === data.contentItem.id) {
      const nextContent = this.contentState.availableContent[0];
      if (nextContent) {
        await this.switchToContent(nextContent.id);
      } else {
        this.contentState.currentContent = undefined;
        await this.renderEmptyState();
      }
    }
  }

  /**
   * Handle content switched event (from another widget or user action)
   */
  private async handleContentSwitched(data: ContentEventData): Promise<void> {
    // Only respond if this content is for us and auto-switch is enabled
    if (this.contentConfig.autoSwitchContent && data.source !== 'widget') {
      await this.switchToContent(data.contentItem.id);
    }
  }

  /**
   * Load initial content based on user state
   */
  private async loadInitialContent(): Promise<void> {
    this.verbose() && console.log(`🎯 ${this.config.widgetName}: Loading initial content...`);

    // TODO: In real implementation, load from ContentService
    // For now, we'll prepare the interface

    // Example of what this would look like:
    // if (this.contentService) {
    //   const userId = await this.getCurrentUserId();
    //   const openContent = await this.contentService.getOpenContent(userId);
    //   this.contentState.availableContent = openContent.filter(content =>
    //     this.supportsContentType(content.type)
    //   );
    //
    //   const currentContent = await this.contentService.getCurrentContent(userId);
    //   if (currentContent && this.supportsContentType(currentContent.type)) {
    //     this.contentState.currentContent = currentContent;
    //     await this.renderContent(currentContent);
    //   }
    // }
  }

  /**
   * Persist current content state
   */
  private async persistContentState(): Promise<void> {
    this.verbose() && console.log(`🎯 ${this.config.widgetName}: Persisting content state...`);

    // Store content state in widget data
    await this.storeData('content_state', {
      currentContentId: this.contentState.currentContent?.id,
      contentMetadata: this.contentState.contentMetadata,
      lastContentUpdate: this.contentState.lastContentUpdate
    });
  }

  /**
   * Sync content state back to ContentService
   */
  private async syncContentState(): Promise<void> {
    // TODO: Implement when ContentService integration is complete
    this.verbose() && console.log(`🎯 ${this.config.widgetName}: Content state sync would happen here`);
  }

  /**
   * Emit content event to other widgets
   */
  private async emitContentEvent(eventName: ContentEventName, data: ContentEventData): Promise<void> {
    const event = new CustomEvent(eventName, { detail: data });
    document.dispatchEvent(event);
  }

  /**
   * Render empty state when no content is available
   */
  private async renderEmptyState(): Promise<void> {
    this.verbose() && console.log(`🎯 ${this.config.widgetName}: Rendering empty state...`);

    // Default empty state - subclasses should override
    this.shadowRoot.innerHTML = `
      <div class="content-empty-state">
        <div class="empty-icon">📭</div>
        <div class="empty-title">No Content Available</div>
        <div class="empty-message">Select content to display in this widget</div>
      </div>
      <style>
        .content-empty-state {
          display: flex;
          flex-direction: column;
          align-items: center;
          justify-content: center;
          height: 100%;
          padding: 2rem;
          text-align: center;
          color: #666;
        }
        .empty-icon {
          font-size: 3rem;
          margin-bottom: 1rem;
        }
        .empty-title {
          font-size: 1.2rem;
          font-weight: bold;
          margin-bottom: 0.5rem;
        }
        .empty-message {
          opacity: 0.8;
        }
      </style>
    `;
  }
}