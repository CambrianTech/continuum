// GENERATED from the Rust command registry (core/continuum-core sdk_codegen).
// DO NOT EDIT. Source of truth: each command's CommandSpec (name + ts-rs
// Params/Result + wire shape). Regenerate after a command changes.

import type { TextGenerationResponse } from './wire/ai/TextGenerationResponse';
import type { CloseParams } from './wire/ai_inference/CloseParams';
import type { CloseResult } from './wire/ai_inference/CloseResult';
import type { GenerateParams } from './wire/ai_inference/GenerateParams';
import type { InspectParams } from './wire/ai_inference/InspectParams';
import type { InspectResult } from './wire/ai_inference/InspectResult';
import type { OpenParams } from './wire/ai_inference/OpenParams';
import type { OpenResult } from './wire/ai_inference/OpenResult';
import type { ChatPollParams } from './wire/chat/ChatPollParams';
import type { ChatPollResult } from './wire/chat/ChatPollResult';
import type { ChatSendParams } from './wire/chat/ChatSendParams';
import type { ChatSendResult } from './wire/chat/ChatSendResult';
import type { DataListParams } from './wire/data/DataListParams';
import type { DataListResult } from './wire/data/DataListResult';
import type { PingParams } from './wire/health/PingParams';
import type { PingResult } from './wire/health/PingResult';
import type { InferenceRequest } from './wire/inference_llm/InferenceRequest';
import type { InferenceResponse } from './wire/inference_llm/InferenceResponse';
import type { ScreenshotParams } from './wire/interface/ScreenshotParams';
import type { ScreenshotResult } from './wire/interface/ScreenshotResult';
import type { CommandRequest } from './wire/runtime/CommandRequest';
import type { CommandResponse } from './wire/runtime/CommandResponse';

/**
* name -> { params, result }. Generated; the contract is Rust-origin and
* models the REAL wire each handler exchanges.
*
* `Enveloped` commands ride the substrate envelope, so their params are
* `CommandRequest<P>` and results `CommandResponse<T>` (the flattened
* success/handle the handler actually emits). `Bare` substrate commands and
* `Provided` adapter commands exchange their payloads directly. Command
* FAILURE is a rejected promise (transport error), never a result field.
*/
export interface CommandMap {
  'ai/inference/close': { params: CommandRequest<CloseParams>; result: CommandResponse<CloseResult> };
  'ai/inference/generate': { params: CommandRequest<GenerateParams>; result: CommandResponse<TextGenerationResponse> };
  'ai/inference/inspect': { params: CommandRequest<InspectParams>; result: CommandResponse<InspectResult> };
  'ai/inference/open': { params: CommandRequest<OpenParams>; result: CommandResponse<OpenResult> };
  'chat/poll': { params: CommandRequest<ChatPollParams>; result: CommandResponse<ChatPollResult> };
  'chat/send': { params: CommandRequest<ChatSendParams>; result: CommandResponse<ChatSendResult> };
  'data/list': { params: DataListParams; result: DataListResult };
  'inference/llm/request': { params: InferenceRequest; result: InferenceResponse };
  'interface/screenshot': { params: ScreenshotParams; result: ScreenshotResult };
  'ping': { params: PingParams; result: PingResult };
}

export type CommandName = keyof CommandMap;
