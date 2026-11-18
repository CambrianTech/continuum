// The model's own chat contract, in process: render its jinja template WITH the offered tools, and
// parse its native tool-call output back into structured calls. These are llama.cpp's common/chat
// functions, the same ones llama-server's /v1 path ran, exposed as a thin C seam so the in-process
// engine keeps the contract residents had (no prompt-described tools, no text guessing).
//
// The boundary speaks llama.cpp's own OpenAI-compatible JSON (messages, tools, the parsed message),
// so no field is re-derived here. No C++ exception crosses the FFI: every failure is a returned
// error string, owned by the caller and released with continuum_chat_free.

#include "chat.h"
#include "json.h"
#include "reasoning-budget.h"

#include <cstdlib>
#include <cstring>
#include <string>

extern "C" {

struct continuum_chat_templates {
    common_chat_templates_ptr tmpls;
};

static char * dup(const std::string & s) {
    char * out = static_cast<char *>(std::malloc(s.size() + 1));
    if (out) {
        std::memcpy(out, s.c_str(), s.size() + 1);
    }
    return out;
}

void continuum_chat_free(char * s) { std::free(s); }

// The model's templates, built once from its GGUF metadata. NULL with *err set on failure.
continuum_chat_templates * continuum_chat_templates_init(const llama_model * model, char ** err) {
    try {
        auto * out = new continuum_chat_templates{ common_chat_templates_init(model, "") };
        if (!out->tmpls) {
            delete out;
            *err = dup("the model carries no usable chat template");
            return nullptr;
        }
        return out;
    } catch (const std::exception & e) {
        *err = dup(e.what());
        return nullptr;
    }
}

void continuum_chat_templates_free(continuum_chat_templates * t) { delete t; }

// Render `messages_json` (OpenAI messages) with `tools_json` (OpenAI tools, may be "[]") through the
// model's own template. Returns a JSON object: prompt, format, generation_prompt, parser, grammar,
// grammar_lazy, grammar_triggers [{type, value}], preserved_tokens, additional_stops. NULL + *err on
// failure.
char * continuum_chat_apply(const continuum_chat_templates * t, const char * messages_json, const char * tools_json,
                            bool enable_thinking, char ** err) {
    try {
        common_chat_templates_inputs in;
        in.messages = common_chat_msgs_parse_oaicompat(common_json::parse(messages_json));
        in.tools = common_chat_tools_parse_oaicompat(common_json::parse(tools_json));
        in.add_generation_prompt = true;
        in.use_jinja = true;
        in.enable_thinking = enable_thinking;
        in.parallel_tool_calls = false;
        common_chat_params p = common_chat_templates_apply(t->tmpls.get(), in);

        common_json out = common_json::object();
        out["prompt"] = p.prompt;
        out["format"] = static_cast<int>(p.format);
        out["generation_prompt"] = p.generation_prompt;
        out["parser"] = p.parser;
        out["grammar"] = p.grammar;
        out["grammar_lazy"] = p.grammar_lazy;
        common_json triggers = common_json::array();
        for (const auto & trig : p.grammar_triggers) {
            common_json one = common_json::object();
            one["type"] = static_cast<int>(trig.type);
            one["value"] = trig.value;
            triggers.push_back(one);
        }
        out["grammar_triggers"] = triggers;
        common_json preserved = common_json::array();
        for (const auto & tok : p.preserved_tokens) {
            preserved.push_back(tok);
        }
        out["preserved_tokens"] = preserved;
        common_json stops = common_json::array();
        for (const auto & stop : p.additional_stops) {
            stops.push_back(stop);
        }
        out["additional_stops"] = stops;
        out["supports_thinking"] = p.supports_thinking;
        out["thinking_start_tag"] = p.thinking_start_tag;
        common_json end_tags = common_json::array();
        for (const auto & tag : p.thinking_end_tags) {
            end_tags.push_back(tag);
        }
        out["thinking_end_tags"] = end_tags;
        return dup(out.dump());
    } catch (const std::exception & e) {
        *err = dup(e.what());
        return nullptr;
    }
}

// Parse a finished generation with the format and serialized parser `continuum_chat_apply` returned.
// Returns the OpenAI message JSON (content, reasoning_content, tool_calls). NULL + *err on failure.
char * continuum_chat_parse(const char * output, int format, const char * generation_prompt, const char * parser,
                            char ** err) {
    try {
        common_chat_parser_params params;
        params.format = static_cast<common_chat_format>(format);
        params.generation_prompt = generation_prompt;
        // The reasoning split residents ran with on llama-server: its default (common.h:644), which
        // serving's launch never overrode. Thinking comes back as reasoning_content, not as content.
        params.reasoning_format = COMMON_REASONING_FORMAT_DEEPSEEK;
        params.parse_tool_calls = true;
        if (parser[0] != '\0') {
            params.parser.load(parser);
        }
        common_chat_msg msg = common_chat_parse(output, false, params);
        return dup(msg.to_json_oaicompat().dump());
    } catch (const std::exception & e) {
        *err = dup(e.what());
        return nullptr;
    }
}

// llama.cpp's reasoning-budget sampler: counts tokens after `start_tag` and, once `budget` is spent,
// forces the first of `end_tags` (`n_end_tags` strings), the sequence llama-server forced
// (server-schema.cpp: the end tag, with no budget message since serving never set one). NULL + *err
// on failure; the caller adds it to a sampler chain, which takes ownership.
llama_sampler * continuum_reasoning_budget_init(const llama_model * model, const char * start_tag,
                                                const char * const * end_tags, size_t n_end_tags, int32_t budget,
                                                char ** err) {
    try {
        const llama_vocab * vocab = llama_model_get_vocab(model);
        std::vector<llama_tokens> starts = { common_tokenize(vocab, start_tag, false, true) };
        std::vector<llama_tokens> ends;
        for (size_t i = 0; i < n_end_tags; ++i) {
            ends.push_back(common_tokenize(vocab, end_tags[i], false, true));
        }
        if (starts.front().empty() || ends.empty() || ends.front().empty()) {
            *err = dup("the template names no thinking start or end tag to bound");
            return nullptr;
        }
        llama_sampler * s = common_reasoning_budget_init(vocab, starts, ends, ends.front(), budget);
        if (!s) {
            *err = dup("llama.cpp made no reasoning-budget sampler");
        }
        return s;
    } catch (const std::exception & e) {
        *err = dup(e.what());
        return nullptr;
    }
}

}  // extern "C"
