import type { KanbanViewState } from '@continuum/sdk-typescript';
import type { ChatViewModel } from './chatViewModel';
import type { ChatContentBody } from './patternProjections';

export const PROJECT_PURPOSE = 'project';

export type ProjectBoardState =
  | { readonly status: 'awaiting' }
  | { readonly status: 'rejected'; readonly reason: 'foreign-room' }
  | { readonly status: 'ready'; readonly snapshot: KanbanViewState };

/** Shared project surface: the room's authoritative board and conversation.
 * A previous room's late snapshot must never masquerade as this room's work. */
export interface ProjectContentBody {
  readonly title: string;
  readonly board: ProjectBoardState;
  readonly chat: ChatContentBody;
}

export function projectContentBody(vm: ChatViewModel, board?: KanbanViewState): ProjectContentBody {
  return {
    title: vm.roomName,
    board: board === undefined
      ? { status: 'awaiting' }
      : board.room_id === vm.roomId
        ? { status: 'ready', snapshot: board }
        : { status: 'rejected', reason: 'foreign-room' },
    chat: { messages: vm.messages, transcript: vm.transcript, isEmpty: vm.isEmpty },
  };
}
