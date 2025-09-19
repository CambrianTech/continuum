/**
 * Rust-Level TypeScript Type System for Message Content Adapters
 *
 * Uses modern TypeScript patterns:
 * - Const assertions for exhaustive type checking
 * - Branded types to prevent type confusion
 * - Generic constraints with distributive conditional types
 * - Template literal types for compile-time string validation
 * - Phantom types to encode invariants in the type system
 */

import type { ChatMessageData } from '../../../system/data/domains/ChatMessage';
import type { ChatContentType } from '../shared/ChatMessagePayload';

// ============================================================================
// BRANDED TYPES - Prevent type confusion at compile time
// ============================================================================

declare const __brand: unique symbol;
declare const __contentType: unique symbol;
declare const __elementID: unique symbol;
declare const __adapterVersion: unique symbol;

export type AdapterID = string & { readonly [__brand]: 'AdapterID' };
export type ContentTypeID = ChatContentType & { readonly [__contentType]: true };
export type ElementID = string & { readonly [__elementID]: 'ElementID' };
export type AdapterVersion = string & { readonly [__adapterVersion]: 'AdapterVersion' };

// Branded type constructors
export const AdapterID = (id: string): AdapterID => id as AdapterID;
export const ElementID = (id: string): ElementID => id as ElementID;
export const AdapterVersion = (version: string): AdapterVersion => version as AdapterVersion;

// ============================================================================
// CONST ASSERTIONS - Exhaustive type checking like Rust enums
// ============================================================================

export const INTERACTION_TYPES = [
  'click', 'hover', 'focus', 'fullscreen', 'download', 'share',
  'ai_describe', 'ai_summarize', 'open_link', 'retry_load'
] as const;

export const LOADING_STATES = ['idle', 'loading', 'loaded', 'error', 'retry'] as const;

export const AI_EDIT_ACTIONS = [
  'improve_alt_text', 'generate_caption', 'summarize_content',
  'enhance_description', 'verify_link', 'optimize_image', 'translate_text'
] as const;

export const ADAPTER_ERROR_TYPES = [
  'parse_error', 'render_error', 'load_error', 'interaction_error', 'initialization_error'
] as const;

// Derive types from const assertions - like Rust derive macros
export type InteractionType = typeof INTERACTION_TYPES[number];
export type LoadingState = typeof LOADING_STATES[number];
export type AIEditAction = typeof AI_EDIT_ACTIONS[number];
export type AdapterErrorType = typeof ADAPTER_ERROR_TYPES[number];

// ============================================================================
// CONFIGURATION WITH COMPILE-TIME VALIDATION
// ============================================================================

export interface AdapterRenderOptions {
  readonly enableIntersectionObserver?: boolean;
  readonly lazyLoadContent?: boolean;
  readonly enableInteractions?: boolean;
  readonly customClassNames?: readonly string[];
  readonly aiEditingEnabled?: boolean;
  readonly debugMode?: boolean;
}

// Template literal types for CSS class validation
export type CSSClassName = `${string}-${string}` | `${string}-${string}-${string}`;
export type ValidatedClassName<T extends string> = T extends CSSClassName ? T : never;

/**
 * Adapter lifecycle hooks - strongly typed callbacks
 */
export interface AdapterLifecycleHooks {
  readonly onContentReady?: () => void;
  readonly onContentError?: (error: AdapterError) => void;
  readonly onUserInteraction?: (interaction: UserInteraction) => void;
  readonly onAIEdit?: (editData: AIEditInstruction) => void;
  readonly onLoadingStateChange?: (state: LoadingState) => void;
}

/**
 * User interaction events - strongly typed
 */
export interface UserInteraction {
  readonly type: InteractionType;
  readonly contentType: ChatContentType;
  readonly target: HTMLElement;
  readonly data: InteractionData;
  readonly timestamp: number;
}

export interface InteractionData {
  readonly [key: string]: unknown;
  readonly url?: string;
  readonly position?: { x: number; y: number };
  readonly metadata?: Record<string, unknown>;
}

/**
 * AI editing instructions - strongly typed for future AI capabilities
 */
export interface AIEditInstruction {
  readonly action: AIEditAction;
  readonly target: AIEditTarget;
  readonly parameters: Record<string, unknown>;
  readonly confidence: number; // 0-1 confidence score
}

export type AIEditTarget =
  | 'content'
  | 'metadata'
  | 'styling'
  | 'accessibility'
  | 'performance';

/**
 * Adapter-specific error types
 */
export class AdapterError extends Error {
  constructor(
    message: string,
    public readonly contentType: ChatContentType,
    public readonly errorType: AdapterErrorType,
    public readonly originalError?: Error
  ) {
    super(message);
    this.name = 'AdapterError';
  }
}

/**
 * Content data interfaces - strongly typed per content type
 */
export interface BaseContentData {
  readonly contentType: ChatContentType;
  readonly originalText: string;
  readonly metadata?: Record<string, unknown>;
}

export interface TextContentData extends BaseContentData {
  readonly contentType: 'text';
  readonly text: string;
  readonly formatting?: TextFormatting;
}

export interface ImageContentData extends BaseContentData {
  readonly contentType: 'image';
  readonly url: string;
  readonly altText?: string;
  readonly width?: number;
  readonly height?: number;
  readonly caption?: string;
  readonly thumbnail?: string;
  readonly fileSize?: number;
  readonly mimeType?: string;
}

export interface URLCardContentData extends BaseContentData {
  readonly contentType: 'url_card';
  readonly url: string;
  readonly title?: string;
  readonly description?: string;
  readonly siteName?: string;
  readonly favicon?: string;
  readonly imageUrl?: string;
  readonly domain: string;
  readonly isSecure: boolean;
}

export interface VideoContentData extends BaseContentData {
  readonly contentType: 'video';
  readonly url: string;
  readonly thumbnail?: string;
  readonly duration?: number;
  readonly width?: number;
  readonly height?: number;
  readonly mimeType?: string;
  readonly autoplay?: boolean;
}

export interface FileContentData extends BaseContentData {
  readonly contentType: 'file_attachment';
  readonly fileId: string;
  readonly fileName: string;
  readonly fileSize: number;
  readonly mimeType: string;
  readonly downloadUrl: string;
  readonly icon?: string;
}

/**
 * Union type for all content data types
 */
export type ContentData =
  | TextContentData
  | ImageContentData
  | URLCardContentData
  | VideoContentData
  | FileContentData;

/**
 * Text formatting options
 */
export interface TextFormatting {
  readonly bold?: boolean;
  readonly italic?: boolean;
  readonly underline?: boolean;
  readonly strikethrough?: boolean;
  readonly code?: boolean;
  readonly codeBlock?: string; // Language for syntax highlighting
  readonly links?: ReadonlyArray<LinkInfo>;
  readonly mentions?: ReadonlyArray<MentionInfo>;
  readonly emojis?: ReadonlyArray<EmojiInfo>;
}

export interface LinkInfo {
  readonly url: string;
  readonly text: string;
  readonly start: number;
  readonly end: number;
}

export interface MentionInfo {
  readonly userId: string;
  readonly userName: string;
  readonly start: number;
  readonly end: number;
}

export interface EmojiInfo {
  readonly emoji: string;
  readonly shortcode: string;
  readonly start: number;
  readonly end: number;
}

/**
 * Adapter registry types - for dynamic adapter loading
 */
export type AdapterRegistryMap = {
  readonly [K in ChatContentType]: AdapterConstructor<ContentDataFor<K>>;
};

export interface AdapterConstructor<TContentData extends ContentData = ContentData> {
  new (options?: AdapterRenderOptions, hooks?: AdapterLifecycleHooks): MessageAdapter<TContentData>;
  readonly contentType: ChatContentType;
  readonly displayName: string;
  readonly version: string;
}

// ============================================================================
// RESULT TYPES - Rust-style error handling
// ============================================================================

export type Result<T, E = AdapterError> =
  | { readonly success: true; readonly data: T }
  | { readonly success: false; readonly error: E };

export type AsyncResult<T, E = AdapterError> = Promise<Result<T, E>>;

// Result constructors
export const Ok = <T>(data: T): Result<T> => ({ success: true, data });
export const Err = <E>(error: E): Result<never, E> => ({ success: false, error });

// ============================================================================
// PHANTOM TYPES - Encode adapter state in the type system
// ============================================================================

declare const __adapterState: unique symbol;
export type AdapterState = 'uninitialized' | 'initializing' | 'ready' | 'error';

export interface StatefulAdapter<TState extends AdapterState = AdapterState> {
  readonly [__adapterState]: TState;
}

export type UninitializedAdapter<T> = T & StatefulAdapter<'uninitialized'>;
export type InitializingAdapter<T> = T & StatefulAdapter<'initializing'>;
export type ReadyAdapter<T> = T & StatefulAdapter<'ready'>;
export type ErrorAdapter<T> = T & StatefulAdapter<'error'>;

// ============================================================================
// GENERIC CONSTRAINTS WITH DISTRIBUTIVE CONDITIONAL TYPES
// ============================================================================

// Ensure content type matches data type at compile time
export type ConstrainedContentData<T extends ChatContentType> =
  ContentData & { readonly contentType: T };

// Distributive conditional type for mapping content types to adapters
export type AdapterFor<T extends ChatContentType> = T extends ChatContentType
  ? MessageAdapter<ConstrainedContentData<T>>
  : never;

// ============================================================================
// MESSAGE ADAPTER INTERFACE - With advanced generics
// ============================================================================

export interface MessageAdapter<TContentData extends ContentData = ContentData> {
  readonly contentType: TContentData['contentType'];
  readonly options: AdapterRenderOptions;
  readonly hooks: AdapterLifecycleHooks;

  // Rust-style Result types for error handling
  parseContent(message: ChatMessageData): Result<TContentData>;
  renderContent(data: TContentData, currentUserId: string): Result<string>;
  handleContentLoading(element: HTMLElement): AsyncResult<void>;

  // Pure functions
  getContentClasses(): readonly string[];
  getCSS(): string;

  // Main interface methods
  renderMessage(message: ChatMessageData, currentUserId: string): Result<string>;
  initializeInDOM(element: HTMLElement): AsyncResult<void>;
}

// ============================================================================
// ADAPTER REGISTRY WITH MAPPED TYPES
// ============================================================================

// Ensure registry completeness at compile time
export interface AdapterRegistry extends AdapterRegistryMap {
  // Additional methods for dynamic loading
  register<T extends ChatContentType>(
    contentType: T,
    adapter: AdapterFor<T>
  ): Result<void>;

  get<T extends ChatContentType>(
    contentType: T
  ): Result<AdapterFor<T>>;
}

/**
 * Type helpers - map content types to their data structures
 */
export type ContentDataFor<T extends ChatContentType> =
  T extends 'text' ? TextContentData :
  T extends 'image' ? ImageContentData :
  T extends 'url_card' ? URLCardContentData :
  T extends 'video' ? VideoContentData :
  T extends 'file_attachment' ? FileContentData :
  ContentData;

/**
 * Batch initialization types
 */
export interface BatchInitializationResult {
  readonly successful: ReadonlyArray<ElementID>;
  readonly failed: ReadonlyArray<{
    readonly elementId: ElementID;
    readonly error: AdapterError;
  }>;
  readonly totalTime: number;
}

export interface AdapterMap extends Map<ChatContentType, MessageAdapter> {
  get<T extends ChatContentType>(key: T): MessageAdapter<ContentDataFor<T>> | undefined;
}

/**
 * Dynamic paging types - for infinite scroll integration
 */
export interface DynamicPagingContext {
  readonly scrollContainer: HTMLElement;
  readonly messageContainer: HTMLElement;
  readonly adapters: AdapterMap;
  readonly isInfiniteScrollEnabled: boolean;
  readonly batchSize: number;
}

export interface RowInsertionOptions {
  readonly position: 'prepend' | 'append';
  readonly maintainScrollPosition: boolean;
  readonly batchInitialize: boolean;
  readonly skipDuplicates: boolean;
}

/**
 * CSS injection types
 */
export interface CSSInjectionResult {
  readonly styleElementId: string;
  readonly injectedAdapters: ReadonlyArray<ChatContentType>;
  readonly totalCSSSize: number;
  readonly success: boolean;
}

/**
 * Type guards for runtime type checking
 */
export function isImageContentData(data: ContentData): data is ImageContentData {
  return data.contentType === 'image';
}

export function isURLCardContentData(data: ContentData): data is URLCardContentData {
  return data.contentType === 'url_card';
}

export function isVideoContentData(data: ContentData): data is VideoContentData {
  return data.contentType === 'video';
}

export function isTextContentData(data: ContentData): data is TextContentData {
  return data.contentType === 'text';
}

export function isFileContentData(data: ContentData): data is FileContentData {
  return data.contentType === 'file_attachment';
}