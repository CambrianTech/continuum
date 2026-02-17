/**
 * RustCoreIPC Tool Parsing Module - Tool call parsing, correction, codec
 *
 * Replaces 784 lines of TypeScript ToolFormatAdapter hierarchy with Rust IPC.
 * Five format adapters (Anthropic XML, function-style, bare JSON, markdown backtick,
 * old-style XML) + parameter correction + tool name codec.
 */

import type { RustCoreIPCClientBase } from './base';
import type {
	ToolParseResult,
	ParsedToolCall,
	CorrectedToolCall,
} from '../../../../shared/generated';

export interface ToolParsingMixin {
	/**
	 * Parse tool calls from AI response text using all 5 format adapters.
	 * Returns parsed+corrected tool calls and cleaned text with tool blocks stripped.
	 * Sub-microsecond in Rust.
	 */
	toolParsingParse(responseText: string): Promise<ToolParseResult>;

	/**
	 * Correct a single tool call: name mapping + parameter mapping + content cleaning.
	 */
	toolParsingCorrect(toolName: string, parameters: Record<string, string>): Promise<CorrectedToolCall>;

	/**
	 * Register tool names for codec reverse lookup.
	 * Call once at startup with all known tool names.
	 */
	toolParsingRegisterTools(tools: string[]): Promise<{ registered: number; total: number }>;

	/**
	 * Decode a model-produced tool name variant back to the original.
	 * Handles: code_write, code__write, $FUNCTIONS.code_write, code-write, etc.
	 */
	toolParsingDecodeName(name: string): Promise<{ decoded: string; changed: boolean }>;

	/**
	 * Encode a tool name for API transmission: slashes -> underscores.
	 */
	toolParsingEncodeName(name: string): Promise<{ encoded: string }>;
}

export function ToolParsingMixin<T extends new (...args: any[]) => RustCoreIPCClientBase>(Base: T) {
	return class extends Base implements ToolParsingMixin {
		async toolParsingParse(responseText: string): Promise<ToolParseResult> {
			const response = await this.request({
				command: 'tool-parsing/parse',
				response_text: responseText,
			});

			if (!response.success) {
				throw new Error(response.error || 'Failed to parse tool calls');
			}

			return response.result as ToolParseResult;
		}

		async toolParsingCorrect(toolName: string, parameters: Record<string, string>): Promise<CorrectedToolCall> {
			const response = await this.request({
				command: 'tool-parsing/correct',
				tool_name: toolName,
				parameters,
			});

			if (!response.success) {
				throw new Error(response.error || 'Failed to correct tool call');
			}

			return response.result as CorrectedToolCall;
		}

		async toolParsingRegisterTools(tools: string[]): Promise<{ registered: number; total: number }> {
			const response = await this.request({
				command: 'tool-parsing/register-tools',
				tools,
			});

			if (!response.success) {
				throw new Error(response.error || 'Failed to register tools');
			}

			return response.result as { registered: number; total: number };
		}

		async toolParsingDecodeName(name: string): Promise<{ decoded: string; changed: boolean }> {
			const response = await this.request({
				command: 'tool-parsing/decode-name',
				name,
			});

			if (!response.success) {
				throw new Error(response.error || 'Failed to decode tool name');
			}

			return response.result as { decoded: string; changed: boolean };
		}

		async toolParsingEncodeName(name: string): Promise<{ encoded: string }> {
			const response = await this.request({
				command: 'tool-parsing/encode-name',
				name,
			});

			if (!response.success) {
				throw new Error(response.error || 'Failed to encode tool name');
			}

			return response.result as { encoded: string };
		}
	};
}
