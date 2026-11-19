// The model's own chat handling (llama.cpp common/chat): render a conversation through the GGUF's
// template, with its tools and reasoning, and parse what the model writes back into content,
// reasoning and tool calls. A render is a native handle that keeps its parser state, so a streamed
// update parses with it directly (Codex on #6142: no re-serialized render per partial). Exceptions
// terminate on this side of the Rust FFI boundary.
#include "chat.h"
#include "reasoning-budget.h"

#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <exception>
#include <stdexcept>
#include <string>
#include <vector>

struct continuum_chat {
    common_chat_templates_ptr templates;
};

// What every call hands back: its result (a handle or an owned string) or the native error text.
struct continuum_chat_reply {
    void *out;
    char error[512];
};

// What the model has written so far, by length (no terminator), and whether the stream continues.
struct continuum_chat_utterance {
    const char *text;
    size_t length;
    int partial;
};

// One rendered request: what the sampler and the caller need once, and the parser every update uses.
struct continuum_chat_render {
    common_chat_params params;
    common_chat_parser_params parser;
};

static void report(continuum_chat_reply *reply, const char *what) noexcept {
    std::snprintf(reply->error, sizeof(reply->error), "%s", what);
}

template<class Work>
static int guarded(Work work, continuum_chat_reply *reply) noexcept {
    try {
        work();
        return 0;
    } catch (const std::exception &failure) {
        report(reply, failure.what());
    } catch (...) {
        report(reply, "unknown C++ exception");
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

// The reasoning formats the Rust side names; anything else is refused, never defaulted.
static common_reasoning_format reasoning_format(int code) {
    switch (code) {
        case 0: return COMMON_REASONING_FORMAT_NONE;
        case 1: return COMMON_REASONING_FORMAT_AUTO;
        case 2: return COMMON_REASONING_FORMAT_DEEPSEEK;
        case 3: return COMMON_REASONING_FORMAT_DEEPSEEK_LEGACY;
        default: throw std::invalid_argument("unknown reasoning format");
    }
}

// The model's templates; `override_template` (nullable) replaces the GGUF's own.
extern "C" int continuum_chat_init(const llama_model *model, const char *override_template,
        continuum_chat_reply *reply) noexcept {
    return guarded([&] {
        auto templates = common_chat_templates_init(model, override_template ? override_template : "");
        reply->out = new continuum_chat{std::move(templates)};
    }, reply);
}

extern "C" void continuum_chat_free(continuum_chat *chat) noexcept {
    delete chat;
}

extern "C" void continuum_chat_string_free(char *text) noexcept {
    std::free(text);
}

// Render an OpenAI-shaped request: {"messages":[...], "tools":[...], "tool_choice":"auto",
// "json_schema":{...}|null, "grammar":"", "enable_thinking":bool, "add_generation_prompt":bool,
// "parallel_tool_calls":bool}, under `reasoning` (see reasoning_format).
extern "C" int continuum_chat_render_new(const continuum_chat *chat, const char *request, int reasoning,
        continuum_chat_reply *reply) noexcept {
    return guarded([&] {
        const auto in = common_json::parse(request);
        common_chat_templates_inputs inputs;
        inputs.messages = common_chat_msgs_parse_oaicompat(in.at("messages"));
        if (in.contains("tools") && !in.at("tools").is_null()) {
            inputs.tools = common_chat_tools_parse_oaicompat(in.at("tools"));
        }
        if (in.contains("tail_tools") && !in.at("tail_tools").is_null()) {
            inputs.tail_tools = common_chat_tools_parse_oaicompat(in.at("tail_tools"));
        }
        inputs.tool_choice = common_chat_tool_choice_parse_oaicompat(in.value("tool_choice", std::string("auto")));
        if (in.contains("json_schema") && !in.at("json_schema").is_null()) {
            inputs.json_schema = in.at("json_schema").dump();
        }
        inputs.grammar = in.value("grammar", std::string());
        inputs.enable_thinking = in.value("enable_thinking", true);
        inputs.reasoning_format = reasoning_format(reasoning);
        inputs.add_generation_prompt = in.value("add_generation_prompt", true);
        inputs.parallel_tool_calls = in.value("parallel_tool_calls", false);
        inputs.use_jinja = true;

        auto render = std::make_unique<continuum_chat_render>();
        render->params = common_chat_templates_apply(chat->templates.get(), inputs);
        render->parser = common_chat_parser_params(render->params);
        render->parser.reasoning_format = inputs.reasoning_format;
        render->parser.parse_tool_calls = true;
        if (!render->params.parser.empty()) {
            render->parser.parser.load(render->params.parser);
        }
        reply->out = render.release();
    }, reply);
}

extern "C" void continuum_chat_render_free(continuum_chat_render *render) noexcept {
    delete render;
}

// What the sampler and the caller need, once: the prompt, the grammar and its lazy triggers, the
// preserved tokens, the extra stops and the thinking tags (no parser state: it stays native).
extern "C" int continuum_chat_render_describe(const continuum_chat_render *render, continuum_chat_reply *reply) noexcept {
    return guarded([&] {
        const auto &params = render->params;
        common_json triggers = common_json::array();
        for (const auto &trigger : params.grammar_triggers) {
            triggers.push_back({{"type", static_cast<int>(trigger.type)}, {"value", trigger.value}, {"token", trigger.token}});
        }
        const common_json description = {
            {"prompt", params.prompt},
            {"generation_prompt", params.generation_prompt},
            {"grammar", params.grammar},
            {"grammar_lazy", params.grammar_lazy},
            {"grammar_triggers", triggers},
            {"preserved_tokens", params.preserved_tokens},
            {"additional_stops", params.additional_stops},
            {"supports_thinking", params.supports_thinking},
            {"thinking_start_tag", params.thinking_start_tag},
            {"thinking_end_tags", params.thinking_end_tags},
        };
        reply->out = owned(description.dump());
    }, reply);
}

// Parse what the model has written so far (`length` bytes of `text`) with the render's own parser:
// the assistant message in the OpenAI shape (content, reasoning_content, tool_calls).
extern "C" int continuum_chat_render_parse(const continuum_chat_render *render, const continuum_chat_utterance *said,
        continuum_chat_reply *reply) noexcept {
    return guarded([&] {
        const auto message = common_chat_parse(std::string(said->text, said->length), said->partial != 0, render->parser);
        reply->out = owned(message.to_json_oaicompat().dump());
    }, reply);
}

// A content-only render without a model, for tests of the boundary itself.
extern "C" int continuum_chat_render_content_only(int reasoning, continuum_chat_reply *reply) noexcept {
    return guarded([&] {
        auto render = std::make_unique<continuum_chat_render>();
        render->params.format = COMMON_CHAT_FORMAT_CONTENT_ONLY;
        render->parser = common_chat_parser_params(render->params);
        render->parser.reasoning_format = reasoning_format(reasoning);
        reply->out = render.release();
    }, reply);
}

// llama.cpp's reasoning-budget sampler (what llama-server ran for reasoning_budget_tokens): counts
// tokens after `start_tag` and, once `budget` is spent, forces the first of `end_tags`
// (`n_end_tags` strings). The sampler returned is owned by the chain the caller adds it to.
extern "C" int continuum_chat_reasoning_budget(const llama_model *model, const char *start_tag,
        const char *const *end_tags, size_t n_end_tags, int32_t budget, continuum_chat_reply *reply) noexcept {
    return guarded([&] {
        const llama_vocab *vocab = llama_model_get_vocab(model);
        std::vector<llama_tokens> starts = { common_tokenize(vocab, start_tag, false, true) };
        std::vector<llama_tokens> ends;
        for (size_t i = 0; i < n_end_tags; ++i) {
            ends.push_back(common_tokenize(vocab, end_tags[i], false, true));
        }
        if (starts.front().empty() || ends.empty() || ends.front().empty()) {
            throw std::invalid_argument("the template names no thinking start or end tag to bound");
        }
        llama_sampler *sampler = common_reasoning_budget_init(vocab, starts, ends, ends.front(), budget);
        if (sampler == nullptr) {
            throw std::runtime_error("llama.cpp made no reasoning-budget sampler");
        }
        reply->out = sampler;
    }, reply);
}
