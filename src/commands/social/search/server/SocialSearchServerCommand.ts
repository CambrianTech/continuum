/**
 * Social Search Command - Server Implementation
 *
 * Semantic search across social media platforms.
 * Returns results with AI-friendly summary.
 */

import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { transformPayload } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import { SocialSearchBaseCommand } from '../shared/SocialSearchCommand';
import type { SocialSearchParams, SocialSearchResult } from '../shared/SocialSearchTypes';
import { loadSocialContext } from '@system/social/server/SocialCommandHelper';

export class SocialSearchServerCommand extends SocialSearchBaseCommand {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  protected async executeSocialSearch(params: SocialSearchParams): Promise<SocialSearchResult> {
    const { platform, query, type, limit } = params;

    if (!platform) throw new Error('platform is required');
    if (!query?.trim()) throw new Error('query is required');

    const ctx = await loadSocialContext(platform, params.personaId, params);

    const searchResult = await ctx.provider.search({
      query: query.trim(),
      type,
      limit: limit ?? 15,
    });

    const posts = searchResult.posts;
    const total = searchResult.totalCount ?? posts.length;

    const lines = posts.map((p, i) => {
      const votes = p.votes > 0 ? `+${p.votes}` : String(p.votes);
      const community = p.community ? `m/${p.community}` : '';
      return `  ${i + 1}. [${votes}] "${p.title}" by ${p.authorName} ${community} (${p.commentCount} comments) — ${p.id}`;
    });

    const typeLabel = type ? ` (type: ${type})` : '';
    const summary = posts.length === 0
      ? `No results for "${query}" on ${platform}${typeLabel}.`
      : `Search "${query}" on ${platform}${typeLabel} — ${total} results:\n${lines.join('\n')}\n\nUse social/browse --mode=post --target=<id> to read any post in detail.`;

    return transformPayload(params, {
      success: true,
      message: `Found ${posts.length} results for "${query}" on ${platform}`,
      summary,
      posts,
      totalCount: total,
    });
  }
}
