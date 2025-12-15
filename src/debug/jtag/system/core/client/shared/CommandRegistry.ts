/**
 * Command Registry - Type-safe command name to types mapping
 *
 * This provides IntelliSense and compile-time safety for Commands.execute()
 * Instead of: Commands.execute<ScreenshotParams, ScreenshotResult>('screenshot', params)
 * You get:     Commands.execute('screenshot', params) with full type inference
 */

import type { CommandParams, CommandResult } from '../../types/JTAGTypes';

// Import all command types (will be auto-generated eventually)
import type { ScreenshotParams, ScreenshotResult } from '../../../../commands/screenshot/shared/ScreenshotTypes';
import type { FileSaveParams, FileSaveResult } from '../../../../commands/file/save/shared/FileSaveTypes';
import type { FileLoadParams, FileLoadResult } from '../../../../commands/file/load/shared/FileLoadTypes';
import type { PingParams, PingResult } from '../../../../commands/ping/shared/PingTypes';
import type { DataCreateParams, DataCreateResult } from '../../../../commands/data/create/shared/DataCreateTypes';
import type { DataReadParams, DataReadResult } from '../../../../commands/data/read/shared/DataReadTypes';
import type { DataUpdateParams, DataUpdateResult } from '../../../../commands/data/update/shared/DataUpdateTypes';
import type { DataDeleteParams, DataDeleteResult } from '../../../../commands/data/delete/shared/DataDeleteTypes';
import type { DataQueryOpenParams, DataQueryOpenResult } from '../../../../commands/data/query-open/shared/QueryOpenTypes';
import type { DataQueryNextParams, DataQueryNextResult } from '../../../../commands/data/query-next/shared/QueryNextTypes';
import type { DataQueryCloseParams, DataQueryCloseResult } from '../../../../commands/data/query-close/shared/QueryCloseTypes';
import type { StateCreateParams, StateCreateResult } from '../../../../commands/state/create/shared/StateCreateTypes';
import type { StateGetParams, StateGetResult } from '../../../../commands/state/get/shared/StateGetTypes';
import type { StateUpdateParams, StateUpdateResult } from '../../../../commands/state/update/shared/StateUpdateTypes';
import type { AdapterTestParams, AdapterTestResult, AllAdaptersTestResult } from '../../../../commands/ai/adapter/test/shared/AdapterTestTypes';
import type { EmbeddingGenerateParams, EmbeddingGenerateResult } from '../../../../commands/ai/embedding/generate/shared/EmbeddingGenerateTypes';
import type { IndexCreateParams, IndexCreateResult } from '../../../../commands/ai/rag/index/create/shared/IndexCreateTypes';
import type { RagQueryOpenParams, RagQueryOpenResult } from '../../../../commands/ai/rag/query-open/shared/RagQueryOpenTypes';
import type { RagQueryFetchParams, RagQueryFetchResult } from '../../../../commands/ai/rag/query-fetch/shared/RagQueryFetchTypes';
import type { RagQueryCloseParams, RagQueryCloseResult } from '../../../../commands/ai/rag/query-close/shared/RagQueryCloseTypes';
import type { TreeParams, TreeResult } from '@commands/workspace/tree/shared/TreeTypes';
import type { ContentOpenParams, ContentOpenResult } from '../../../../commands/content/open/shared/ContentOpenTypes';

/**
 * Command Registry - Maps command names to their param/result types
 * This enables type inference in Commands.execute()
 */
export interface CommandRegistry {
  'screenshot': {
    params: ScreenshotParams;
    result: ScreenshotResult;
  };
  'file/save': {
    params: FileSaveParams;
    result: FileSaveResult;
  };
  'file/load': {
    params: FileLoadParams;
    result: FileLoadResult;
  };
  'ping': {
    params: PingParams;
    result: PingResult;
  };
  'data/create': {
    params: DataCreateParams;
    result: DataCreateResult;
  };
  'data/read': {
    params: DataReadParams;
    result: DataReadResult;
  };
  'data/update': {
    params: DataUpdateParams;
    result: DataUpdateResult;
  };
  'data/delete': {
    params: DataDeleteParams;
    result: DataDeleteResult;
  };
  'data/query-open': {
    params: DataQueryOpenParams;
    result: DataQueryOpenResult;
  };
  'data/query-next': {
    params: DataQueryNextParams;
    result: DataQueryNextResult;
  };
  'data/query-close': {
    params: DataQueryCloseParams;
    result: DataQueryCloseResult;
  };
  'state/create': {
    params: StateCreateParams;
    result: StateCreateResult<any>;
  };
  'state/get': {
    params: StateGetParams;
    result: StateGetResult<any>;
  };
  'state/update': {
    params: StateUpdateParams;
    result: StateUpdateResult<any>;
  };
  'ai/adapter/test': {
    params: AdapterTestParams;
    result: AdapterTestResult | AllAdaptersTestResult;
  };
  'ai/embedding/generate': {
    params: EmbeddingGenerateParams;
    result: EmbeddingGenerateResult;
  };
  'ai/rag/index/create': {
    params: IndexCreateParams;
    result: IndexCreateResult;
  };
  'ai/rag/query-open': {
    params: RagQueryOpenParams;
    result: RagQueryOpenResult;
  };
  'ai/rag/query-fetch': {
    params: RagQueryFetchParams;
    result: RagQueryFetchResult;
  };
  'ai/rag/query-close': {
    params: RagQueryCloseParams;
    result: RagQueryCloseResult;
  };
  'tree': {
    params: TreeParams;
    result: TreeResult;
  };
  'content/open': {
    params: ContentOpenParams;
    result: ContentOpenResult;
  };
  // Add more commands here...
}

/**
 * Type helpers for command execution
 */
export type CommandName = keyof CommandRegistry;
export type CommandParamsFor<T extends CommandName> = CommandRegistry[T]['params'];
export type CommandResultFor<T extends CommandName> = CommandRegistry[T]['result'];

/**
 * Helper to get params without context/sessionId (for external callers)
 */
export type CommandInputFor<T extends CommandName> = Omit<CommandParamsFor<T>, 'context' | 'sessionId'>;