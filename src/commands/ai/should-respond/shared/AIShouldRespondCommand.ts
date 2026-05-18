/**
 * AI Should-Respond Command - Shared base class
 *
 * Sentinel/Coordinator pattern: Use AI to intelligently gate persona responses.
 *
 * Per continuum#1420 (oxidizer) the actual gating logic — prompt
 * assembly, model call, decision parsing — lives in Rust at
 * `cognition/should_respond.rs::evaluate_gating`. The Server impl
 * delegates via `RustCoreIPCClient.cognitionShouldRespond`. This base
 * class is the shared shell that Server + Browser commands extend.
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';

export abstract class AIShouldRespondCommand extends CommandBase<CommandParams, CommandResult> {
  static readonly commandName = 'ai/should-respond';
}
