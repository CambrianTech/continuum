#include "prepared_chat.h"
#include "chat.h"
#include "sampling.h"
#include <algorithm>
#include <cmath>
#include <memory>

// Request-local ownership, mirroring server task_result_state. The selected model
// is borrowed; no new model, context, worker or identity is created here.
struct continuum_chat {
    const llama_model * model = nullptr;
    common_chat_params params;
    common_chat_templates_ptr templates;
    common_chat_parser_params parser;
    common_chat_msg previous;
    std::vector<std::string> tool_ids;
    std::string generated;
    std::string metadata;
    std::string deltas;
    std::string request_id;
    bool finished = false;
    bool failed = false;
};

extern "C" const char * continuum_chat_prepare(const llama_model * model, const char * request_json,
                                               continuum_chat ** out) {
    if (!out) return "invalid chat output handle";
    *out = nullptr;
    try {
        const auto request = common_json::parse(request_json);
        auto chat = std::make_unique<continuum_chat>();
        chat->model = model;
        chat->request_id = request.at("request_id").get<std::string>();
        const auto template_override = request.at("template_override").get<std::string>();
        if (!model && template_override.empty()) return "chat requires a bound model or explicit template";
        chat->templates = common_chat_templates_init(model, template_override, "", "", false);
        common_chat_templates_inputs inputs;
        inputs.messages = common_chat_msgs_parse_oaicompat(request.at("messages"));
        inputs.tools = common_chat_tools_parse_oaicompat(request.at("tools"));
        inputs.tool_choice = common_chat_tool_choice_parse_oaicompat(request.at("tool_choice").get<std::string>());
        inputs.parallel_tool_calls = request.at("parallel_tool_calls").get<bool>();
        inputs.enable_thinking = request.at("enable_thinking").get<bool>();
        inputs.reasoning_format = COMMON_REASONING_FORMAT_AUTO;
        const auto & kwargs = request.at("template_kwargs");
        if (!kwargs.is_object()) return "template kwargs must be an object";
        for (const auto & entry : kwargs.items()) {
            inputs.chat_template_kwargs.emplace(entry.key(), entry.value().get<std::string>());
        }
        inputs.grammar = request.at("grammar").get<std::string>();
        inputs.json_schema = request.at("json_schema").get<std::string>();
        inputs.log_content = false;
        const auto caps = common_chat_templates_get_caps(chat->templates.get());
        if (!inputs.tools.empty() && (!caps.at("supports_tools") || !caps.at("supports_tool_calls")))
            return "template does not natively support requested tools";
        if (inputs.parallel_tool_calls && !caps.at("supports_parallel_tool_calls"))
            return "template does not support parallel tool calls";
        // Media needs the mtmd owner, not a text projection that drops payloads.
        for (const auto & msg : inputs.messages) {
            if ((!msg.tool_calls.empty() || msg.role == "tool") && !caps.at("supports_tool_calls"))
                return "template cannot preserve tool history";
            if (!msg.reasoning_content.empty() && !caps.at("supports_preserve_reasoning"))
                return "template cannot preserve requested reasoning history";
            if (msg.contains_media()) return "prepared chat media requires mtmd integration";
            for (const auto & part : msg.content_parts) {
                if (part.type != "text") return "unsupported prepared chat content part";
            }
        }
        const auto params = common_chat_templates_apply(chat->templates.get(), inputs);
        if (params.parser.empty()) return "template did not provide an explicit output parser";
        chat->parser = common_chat_parser_params(params);
        chat->parser.parser.load(params.parser);
        if (chat->parser.parser.empty()) return "template output parser is empty";
        chat->parser.reasoning_format = COMMON_REASONING_FORMAT_AUTO;
        chat->parser.reasoning_in_content = false;
        chat->parser.parse_tool_calls = true;
        chat->parser.debug = false;
        chat->parser.log_content = false;
        chat->parser.require_complete_final = true;
        auto triggers = common_json::array();
        for (const auto & trigger : params.grammar_triggers) {
            triggers.push_back({{"kind", static_cast<int>(trigger.type)}, {"value", trigger.value}, {"token", trigger.token}});
        }
        chat->metadata = common_json({
            {"prompt", params.prompt}, {"generation_prompt", params.generation_prompt},
            {"format", common_chat_format_name(params.format)}, {"parser", params.parser},
            {"grammar", params.grammar}, {"grammar_lazy", params.grammar_lazy},
            {"grammar_triggers", triggers}, {"preserved_tokens", params.preserved_tokens},
            {"additional_stops", params.additional_stops}, {"parser_closers", params.parser_closers}, {"supports_thinking", params.supports_thinking},
            {"thinking_start_tag", params.thinking_start_tag}, {"thinking_end_tags", params.thinking_end_tags},
            {"message_delimiters", params.message_delimiters.to_json()}
        }).dump();
        chat->params = params;
        *out = chat.release();
        return nullptr;
    } catch (...) {
        return "native chat preparation failed";
    }
}

extern "C" const char * continuum_chat_metadata(const continuum_chat * chat) {
    return chat ? chat->metadata.c_str() : nullptr;
}

extern "C" const char * continuum_chat_append(continuum_chat * chat, const char * text, size_t len,
                                             int final_chunk, size_t hidden_final_bytes, const char ** deltas_json) {
    if (!chat || !deltas_json || (!text && len)) return "invalid chat append arguments";
    *deltas_json = nullptr;
    if (chat->finished || chat->failed) return "chat parser is already terminal";
    try {
        if (hidden_final_bytes) {
            if (!final_chunk || hidden_final_bytes > len) throw std::runtime_error("Invalid hidden suffix");
            const std::string observed(text + len - hidden_final_bytes, hidden_final_bytes);
            const auto & closers = chat->params.parser_closers;
            if (std::find(closers.begin(), closers.end(), observed) == closers.end()) {
                throw std::runtime_error("Unmapped hidden suffix");
            }
        }
        chat->parser.hidden_final_bytes = hidden_final_bytes;
        if (len) chat->generated.append(text, len);
        auto next = common_chat_parse(chat->generated, !final_chunk, chat->parser);
        auto deltas = common_json::array();
        if (!next.empty()) {
            next.set_tool_call_ids(chat->tool_ids, [&]() {
                return chat->request_id + "-tool-" + std::to_string(chat->tool_ids.size());
            });
            for (const auto & diff : common_chat_msg_diff::compute_diffs(chat->previous, next, false)) {
                if (!diff.content_delta.empty()) deltas.push_back({{"kind", "content"}, {"text", diff.content_delta}});
                if (!diff.reasoning_content_delta.empty()) deltas.push_back({{"kind", "reasoning"}, {"text", diff.reasoning_content_delta}});
                if (diff.tool_call_index != std::string::npos) {
                    deltas.push_back({{"kind", "tool"}, {"index", diff.tool_call_index},
                        {"id", diff.tool_call_delta.id}, {"name", diff.tool_call_delta.name},
                        {"arguments", diff.tool_call_delta.arguments}});
                }
            }
            chat->previous = std::move(next);
        }
        chat->deltas = deltas.dump();
        chat->finished = final_chunk != 0;
        *deltas_json = chat->deltas.c_str();
        return nullptr;
    } catch (...) {
        chat->failed = true;
        return "native chat parsing failed";
    }
}

extern "C" void continuum_chat_free(continuum_chat * chat) {
    delete chat;
}

struct continuum_chat_sampler {
    const llama_model * model;
    std::set<llama_token> preserved_tokens;
    std::unique_ptr<common_sampler, decltype(&common_sampler_free)> sampler { nullptr, common_sampler_free };
    bool failed = false;
};

extern "C" const char * continuum_chat_sampler_create(const continuum_chat * chat,
        const continuum_chat_sampling * settings, continuum_chat_sampler ** out) {
    if (!out) return "invalid sampler output handle";
    *out = nullptr;
    if (!settings) return "invalid prepared chat sampling settings";
    try {
        if (!std::isfinite(settings->temperature) || !std::isfinite(settings->top_p) ||
            settings->top_p < 0 || settings->top_p > 1 || settings->top_k < 0)
            return "invalid prepared chat sampling settings";
        if (!std::isfinite(settings->repeat_penalty) || settings->repeat_penalty <= 0 ||
            !std::isfinite(1.0f / settings->repeat_penalty))
            return "invalid prepared chat repeat penalty";
        if (!chat || !chat->model) return "sampler requires the prepared chat's bound model";
        const auto & native = chat->params;
        // This bridge supports native GBNF, never the optional external grammar engine.
        if (native.grammar.compare(0, 11, "%llguidance") == 0)
            return "prepared chat llguidance constraints are unsupported";
        if (native.grammar.find('\0') != std::string::npos)
            return "prepared grammar contains an embedded NUL";
        for (const auto & trigger : native.grammar_triggers) {
            if (trigger.value.find('\0') != std::string::npos)
                return "prepared grammar trigger contains an embedded NUL";
        }
        const auto * vocab = llama_model_get_vocab(chat->model);
        common_params_sampling params;
        params.log_content = false;
        params.seed = settings->seed;
        params.temp = settings->temperature;
        params.top_k = settings->top_k;
        params.top_p = settings->top_p;
        params.min_keep = 1;
        params.penalty_repeat = settings->repeat_penalty;
        params.samplers.clear();
        if (settings->top_k > 0) params.samplers.push_back(COMMON_SAMPLER_TYPE_TOP_K);
        if (settings->top_p > 0 && settings->top_p < 1) params.samplers.push_back(COMMON_SAMPLER_TYPE_TOP_P);
        params.samplers.push_back(COMMON_SAMPLER_TYPE_PENALTIES);
        params.samplers.push_back(COMMON_SAMPLER_TYPE_TEMPERATURE);
        // Match the server's template-produced grammar ownership and prefill semantics.
        if (!native.grammar.empty()) params.grammar = { COMMON_GRAMMAR_TYPE_TOOL_CALLS, native.grammar };
        params.generation_prompt = native.generation_prompt;
        params.grammar_lazy = native.grammar_lazy;
        params.preserved_tokens = common_sampler_preserved_tokens(vocab, native.preserved_tokens);
        params.grammar_triggers = common_sampler_grammar_triggers(
            vocab, native.grammar_triggers, params.preserved_tokens, native.grammar_lazy);
        params.reasoning_budget_start = common_tokenize(vocab, native.thinking_start_tag, false, true);
        for (const auto & end : native.thinking_end_tags) {
            params.reasoning_budget_end.push_back(common_tokenize(vocab, end, false, true));
        }
        // Unlimited reasoning still needs the common owner's lazy-grammar suppression.
        // Stops and output projection stay in scheduler/parser ownership, not sampling.
        auto result = std::make_unique<continuum_chat_sampler>();
        result->model = chat->model;
        result->preserved_tokens = params.preserved_tokens;
        result->sampler.reset(common_sampler_init(chat->model, params));
        if (!result->sampler) return "native prepared sampler returned no handle";
        *out = result.release();
        return nullptr;
    } catch (...) {
        return "native prepared sampler creation failed";
    }
}

extern "C" const char * continuum_chat_sampler_sample(continuum_chat_sampler * sampler,
        llama_context * context, int logit_index, int * token) {
    if (!sampler || !context || !token) return "invalid prepared sampler arguments";
    if (sampler->failed) return "prepared sampler is already terminal";
    if (llama_get_model(context) != sampler->model) return "prepared sampler model binding mismatch";
    try {
        *token = common_sampler_sample(sampler->sampler.get(), context, logit_index);
        common_sampler_accept(sampler->sampler.get(), *token, true);
        return nullptr;
    } catch (...) {
        sampler->failed = true;
        return "native prepared sampling failed";
    }
}

extern "C" int continuum_chat_sampler_preserves_token(const continuum_chat_sampler * sampler, int token) {
    return sampler && sampler->preserved_tokens.count(token) != 0;
}

extern "C" void continuum_chat_sampler_free(continuum_chat_sampler * sampler) {
    delete sampler;
}
