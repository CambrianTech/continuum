// GENERATED from the Rust command registry (core/continuum-core sdk_codegen).
// DO NOT EDIT. Typed accessors — call api.<name>(params), never a string key.

import { Commands } from '../Commands';
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
* Typed command accessors. One method per command, derived from its Rust
* CommandSpec — inputs and outputs strongly typed, the command string baked
* in once here so call sites stay string-free. A Rust param/result change
* regenerates this and breaks now-wrong call sites at compile time.
*/
export class CommandApi {
  constructor(private readonly commands: Commands) {}

  /** `ai/inference/close` */
  aiInferenceClose(params: CommandRequest<CloseParams>): Promise<CommandResponse<CloseResult>> {
    return this.commands.execute('ai/inference/close', params);
  }

  /** `ai/inference/generate` */
  aiInferenceGenerate(params: CommandRequest<GenerateParams>): Promise<CommandResponse<TextGenerationResponse>> {
    return this.commands.execute('ai/inference/generate', params);
  }

  /** `ai/inference/inspect` */
  aiInferenceInspect(params: CommandRequest<InspectParams>): Promise<CommandResponse<InspectResult>> {
    return this.commands.execute('ai/inference/inspect', params);
  }

  /** `ai/inference/open` */
  aiInferenceOpen(params: CommandRequest<OpenParams>): Promise<CommandResponse<OpenResult>> {
    return this.commands.execute('ai/inference/open', params);
  }

  /** `chat/poll` */
  chatPoll(params: CommandRequest<ChatPollParams>): Promise<CommandResponse<ChatPollResult>> {
    return this.commands.execute('chat/poll', params);
  }

  /** `chat/send` */
  chatSend(params: CommandRequest<ChatSendParams>): Promise<CommandResponse<ChatSendResult>> {
    return this.commands.execute('chat/send', params);
  }

  /** `data/list` */
  dataList(params: DataListParams): Promise<DataListResult> {
    return this.commands.execute('data/list', params);
  }

  /** `inference/llm/request` */
  inferenceLlmRequest(params: InferenceRequest): Promise<InferenceResponse> {
    return this.commands.execute('inference/llm/request', params);
  }

  /** `interface/screenshot` */
  interfaceScreenshot(params: ScreenshotParams): Promise<ScreenshotResult> {
    return this.commands.execute('interface/screenshot', params);
  }

  /** `ping` */
  ping(params: PingParams): Promise<PingResult> {
    return this.commands.execute('ping', params);
  }
}
