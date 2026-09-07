import type { KanbanViewState } from '@continuum/sdk-typescript';
import type { ChatViewModel } from './chatViewModel';
import type { ChatContentBody } from './patternProjections';

export const PROJECT_PURPOSE = 'project';

/** Shared project surface: the room's authoritative board and conversation.
 * A previous room's late snapshot must never masquerade as this room's work. */
export interface ProjectContentBody {
  readonly title: string;
  readonly board?: KanbanViewState;
  readonly chat: ChatContentBody;
}

export function projectContentBody(vm: ChatViewModel, board?: KanbanViewState): ProjectContentBody {
  return {
    title: vm.roomName,
    ...(board?.room_id === vm.roomId ? { board } : {}),
    chat: { messages: vm.messages, transcript: vm.transcript, isEmpty: vm.isEmpty },
  };
}
