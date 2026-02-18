/**
 * Embedding Generate Command - Shared Base
 *
 * Low-level primitive for generating embeddings from text
 */

import { CommandBase } from '../../../../../daemons/command-daemon/shared/CommandBase';
import type { EmbeddingGenerateParams, EmbeddingGenerateResult } from './EmbeddingGenerateTypes';

export abstract class EmbeddingGenerateCommand extends CommandBase<EmbeddingGenerateParams, EmbeddingGenerateResult> {
  getDescription(): string {
    return 'Generate embeddings from text using AI provider';
  }
}
