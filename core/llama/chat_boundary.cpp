// The model's own chat handling (llama.cpp common/chat): render a conversation through the GGUF's
// template, with its tools and reasoning, and parse what the model wrote back into content,
// reasoning and tool calls. One JSON document crosses each way, in the OpenAI shape the adapters
// already speak; exceptions terminate on this side of the Rust FFI boundary.
#include "chat.h"

#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <exception>
#include <string>

struct continuum_chat {
    common_chat_templates_ptr templates;
};

static void report(char *error, size_t capacity, const char *what) noexcept {
    std::snprintf(error, capacity, "%s", what);
}

template<class Work>
static int guarded(Work work, char *error, size_t capacity) noexcept {
    try {
        work();
        return 0;
    } catch (const std::exception &failure) {
        report(error, capacity, failure.what());
    } catch (...) {
        report(error, capacity, "unknown C++ exception");
    }
    return 1;
}

// A heap copy the Rust side frees with continuum_chat_string_free.
static char *owned(const std::string &text) {
    char *copy = static_cast<char *>(std::malloc(text.size() + 1));
    if (copy == nullptr) {
        throw std::bad_alloc();
    }
    std::memcpy(copy, text.c_str(), text.size() + 1);
    return copy;
}

static common_reasoning_format reasoning_format(const std::string &name) {
    if (name == "none") return COMMON_REASONING_FORMAT_NONE;
    if (name == "deepseek") return COMMON_REASONING_FORMAT_DEEPSEEK;
    if (name == "deepseek-legacy") return COMMON_REASONING_FORMAT_DEEPSEEK_LEGACY;
    return COMMON_REASONING_FORMAT_AUTO;
}

// The model's templates; `override_template` (nullable) replaces the GGUF's own.
extern "C" int continuum_chat_init(const llama_model *model, const char *override_template,
        continuum_chat **out, char *error, size_t capacity) noexcept {
    return guarded([&] {
        auto templates = common_chat_templates_init(model, override_template ? override_template : "");
        *out = new continuum_chat{std::move(templates)};
    }, error, capacity);
}

extern "C" void continuum_chat_free(continuum_chat *chat) noexcept {
    delete chat;
}

extern "C" void continuum_chat_string_free(char *text) noexcept {
    std::free(text);
}

// Render. Input: {"messages":[...], "tools":[...], "tool_choice":"auto", "json_schema":{...}|null,
// "grammar":"", "enable_thinking":bool, "reasoning_format":"auto", "add_generation_prompt":bool,
// "parallel_tool_calls":bool}. Output: the prompt, the grammar and its lazy triggers, the strings
// to stop at, and the parser state the parse call needs back.
extern "C" int continuum_chat_apply(const continuum_chat *chat, const char *request,
        char **out, char *error, size_t capacity) noexcept {
    return guarded([&] {
        const auto in = common_json::parse(request);
        common_chat_templates_inputs inputs;
        inputs.messages = common_chat_msgs_parse_oaicompat(in.at("messages"));
        if (in.contains("tools") && !in.at("tools").is_null()) {
            inputs.tools = common_chat_tools_parse_oaicompat(in.at("tools"));
        }
        inputs.tool_choice = common_chat_tool_choice_parse_oaicompat(in.value("tool_choice", std::string("auto")));
        if (in.contains("json_schema") && !in.at("json_schema").is_null()) {
            inputs.json_schema = in.at("json_schema").dump();
        }
        inputs.grammar = in.value("grammar", std::string());
        inputs.enable_thinking = in.value("enable_thinking", true);
        inputs.reasoning_format = reasoning_format(in.value("reasoning_format", std::string("auto")));
        inputs.add_generation_prompt = in.value("add_generation_prompt", true);
        inputs.parallel_tool_calls = in.value("parallel_tool_calls", false);
        inputs.use_jinja = true;

        const auto params = common_chat_templates_apply(chat->templates.get(), inputs);
        common_json triggers = common_json::array();
        for (const auto &trigger : params.grammar_triggers) {
            triggers.push_back({{"type", static_cast<int>(trigger.type)}, {"value", trigger.value}, {"token", trigger.token}});
        }
        const common_json result = {
            {"prompt", params.prompt},
            {"grammar", params.grammar},
            {"grammar_lazy", params.grammar_lazy},
            {"grammar_triggers", triggers},
            {"preserved_tokens", params.preserved_tokens},
            {"additional_stops", params.additional_stops},
            {"supports_thinking", params.supports_thinking},
            {"thinking_start_tag", params.thinking_start_tag},
            {"thinking_end_tags", params.thinking_end_tags},
            {"format", static_cast<int>(params.format)},
            {"generation_prompt", params.generation_prompt},
            {"parser", params.parser},
            {"reasoning_format", in.value("reasoning_format", std::string("auto"))},
        };
        *out = owned(result.dump());
    }, error, capacity);
}

// Parse what the model wrote. `rendered` is the apply output (its format, generation prompt,
// parser and reasoning format); the result is the assistant message in the OpenAI shape
// (content, reasoning_content, tool_calls).
extern "C" int continuum_chat_parse(const char *rendered, const char *text, int is_partial,
        char **out, char *error, size_t capacity) noexcept {
    return guarded([&] {
        const auto from = common_json::parse(rendered);
        common_chat_parser_params params;
        params.format = static_cast<common_chat_format>(from.at("format").get<int>());
        params.generation_prompt = from.value("generation_prompt", std::string());
        params.reasoning_format = reasoning_format(from.value("reasoning_format", std::string("auto")));
        params.parse_tool_calls = true;
        const auto parser = from.value("parser", std::string());
        if (!parser.empty()) {
            params.parser.load(parser);
        }
        const auto message = common_chat_parse(text, is_partial != 0, params);
        *out = owned(message.to_json_oaicompat().dump());
    }, error, capacity);
}
