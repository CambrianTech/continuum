/**
 * Adapter Search Command - Shared Types
 *
 * Search for LoRA adapters across registries (HuggingFace, local, mesh)
 */

import type { CommandParams, CommandResult, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Source types for adapter registries
 */
export type AdapterSource = 'all' | 'huggingface' | 'local' | 'mesh';

/**
 * Sort options for search results
 */
export type AdapterSortBy = 'downloads' | 'likes' | 'recent';

/**
 * A single adapter result from search
 */
export interface AdapterSearchResultItem {
  /** Unique identifier (repo_id for HF, local path for local) */
  id: string;
  /** Display name */
  name: string;
  /** Description or model card excerpt */
  description: string;
  /** Base model this adapter targets (e.g., "meta-llama/Llama-3.2-3B") */
  baseModel: string;
  /** Source registry */
  source: AdapterSource;
  /** Download count (HuggingFace) */
  downloads?: number;
  /** Likes count (HuggingFace) */
  likes?: number;
  /** Last modified date */
  lastModified?: string;
  /** Author/organization */
  author?: string;
  /** Tags (task type, language, etc.) */
  tags?: string[];
  /** LoRA rank if known */
  rank?: number;
  /** Whether adapter is already installed locally */
  installed?: boolean;
  /** Local path if installed */
  localPath?: string;
}

/**
 * Adapter Search Command Parameters
 */
export interface AdapterSearchParams extends CommandParams {
  /** Search query (adapter name, description, or capability) */
  query: string;
  /** Filter by base model (e.g., 'llama', 'qwen') */
  baseModel?: string;
  /** Maximum results to return (default: 10) */
  limit?: number;
  /** Source to search: 'all' | 'huggingface' | 'local' | 'mesh' (default: 'all') */
  source?: AdapterSource;
  /** Sort by: 'downloads' | 'likes' | 'recent' (default: 'downloads') */
  sort?: AdapterSortBy;
}

/**
 * Factory function for creating AdapterSearchParams
 */
export const createAdapterSearchParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    /** Search query (adapter name, description, or capability) */
    query: string;
    /** Filter by base model (e.g., 'llama', 'qwen') */
    baseModel?: string;
    /** Maximum results to return (default: 10) */
    limit?: number;
    /** Source to search: 'all' | 'huggingface' | 'local' | 'mesh' (default: 'all') */
    source?: AdapterSource;
    /** Sort by: 'downloads' | 'likes' | 'recent' (default: 'downloads') */
    sort?: AdapterSortBy;
  }
): AdapterSearchParams => createPayload(context, sessionId, {
  ...data
});

/**
 * Adapter Search Command Result
 */
export interface AdapterSearchResult extends CommandResult {
  success: boolean;
  /** Array of matching adapters */
  results: AdapterSearchResultItem[];
  /** Total count of matching adapters (may be more than returned if paginated) */
  totalCount: number;
  /** Query that was executed */
  query: string;
  /** Search duration in milliseconds */
  searchTimeMs: number;
  /** Sources that were searched */
  sourcesSearched: AdapterSource[];
  error?: JTAGError;
}

/**
 * Factory function for creating AdapterSearchResult with defaults
 */
export const createAdapterSearchResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    /** Array of matching adapters */
    results?: AdapterSearchResultItem[];
    /** Total count of matching adapters */
    totalCount?: number;
    /** Query that was executed */
    query?: string;
    /** Search duration in milliseconds */
    searchTimeMs?: number;
    /** Sources that were searched */
    sourcesSearched?: AdapterSource[];
    error?: JTAGError;
  }
): AdapterSearchResult => createPayload(context, sessionId, {
  results: data.results ?? [],
  totalCount: data.totalCount ?? 0,
  query: data.query ?? '',
  searchTimeMs: data.searchTimeMs ?? 0,
  sourcesSearched: data.sourcesSearched ?? [],
  ...data
});

/**
 * Smart Adapter Search-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createAdapterSearchResultFromParams = (
  params: AdapterSearchParams,
  differences: Omit<AdapterSearchResult, 'context' | 'sessionId'>
): AdapterSearchResult => transformPayload(params, differences);
