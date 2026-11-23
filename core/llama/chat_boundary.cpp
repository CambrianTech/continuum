// The model's own chat handling (llama.cpp common/chat): render a conversation through the GGUF's
// template, with its tools and reasoning, and parse what the model writes back into content,
// reasoning and tool calls. A render is a native handle that keeps its parser state, so a streamed
// update parses with it directly (Codex on #6142: no re-serialized render per partial). Exceptions
// terminate on this side of the Rust FFI boundary.
#include "chat.h"
#include "sampling.h"

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

// What the caller needs, once: the prompt, its generation prompt and the extra stops. The grammar,
// its triggers, the preserved tokens and the thinking tags stay native: the render builds her
// sampler from them (continuum_turn_sampler_from_render), as it keeps its parser.
extern "C" int continuum_chat_render_describe(const continuum_chat_render *render, continuum_chat_reply *reply) noexcept {
    return guarded([&] {
        const auto &params = render->params;
        const common_json description = {
            {"prompt", params.prompt},
            {"generation_prompt", params.generation_prompt},
            {"additional_stops", params.additional_stops},
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

// A turn's sampler: llama.cpp's common_sampler, the object llama-server sampled every turn with. It
// owns the grammar (applied only outside her reasoning, accepted only for what it applies to), the
// generation-prompt prefill, the reasoning budget, and the distribution chain, so the engine never
// re-implements any of them (the M5, 2026-11-21: residents moved in-process sampled with no tool
// grammar, wrote calls with undeclared parameters, and the render's parser dropped every one).
struct continuum_turn_sampler {
    common_sampler *sampler;
    ~continuum_turn_sampler() { common_sampler_free(sampler); }
};

// The knobs a request states; everything else is llama.cpp's default, as llama-server ran it.
struct continuum_sampling_knobs {
    float temperature;
    int32_t top_k;
    float top_p;
    float repeat_penalty;
};

static common_params_sampling knobbed(const continuum_sampling_knobs *knobs) {
    common_params_sampling params;
    params.temp = knobs->temperature;
    params.top_k = knobs->top_k;
    params.top_p = knobs->top_p;
    params.penalty_repeat = knobs->repeat_penalty;
    return params;
}

static void made(const llama_model *model, common_params_sampling &params, continuum_chat_reply *reply) {
    common_sampler *sampler = common_sampler_init(model, params);
    if (sampler == nullptr) {
        throw std::runtime_error("llama.cpp made no sampler for these parameters");
    }
    reply->out = new continuum_turn_sampler{sampler};
}

// The sampler `render` describes for its turn, as llama-server derives one from the same chat
// params (tools/server/server-common.cpp and server-schema.cpp): the tool-call grammar with its lazy
// triggers (a one-token trigger word becomes a token trigger and must be preserved), the generation
// prompt, and a reasoning budget of `turn->think_budget` tokens bounded by the template's thinking tags
// (< 0: the model's default, unbounded).
struct continuum_turn_sampling {
    continuum_sampling_knobs knobs;
    int32_t think_budget;
};

extern "C" int continuum_turn_sampler_from_render(const llama_model *model, const continuum_chat_render *render,
        const continuum_turn_sampling *turn, continuum_chat_reply *reply) noexcept {
    return guarded([&] {
        const llama_vocab *vocab = llama_model_get_vocab(model);
        const auto &chat = render->params;
        auto params = knobbed(&turn->knobs);
        if (!chat.grammar.empty()) {
            params.grammar = common_grammar(COMMON_GRAMMAR_TYPE_TOOL_CALLS, chat.grammar);
        }
        params.grammar_lazy = chat.grammar_lazy;
        for (const auto &text : chat.preserved_tokens) {
            const auto ids = common_tokenize(vocab, text, false, true);
            if (ids.size() == 1) {
                params.preserved_tokens.insert(ids[0]);
            }
        }
        for (const auto &trigger : chat.grammar_triggers) {
            if (trigger.type != COMMON_GRAMMAR_TRIGGER_TYPE_WORD) {
                params.grammar_triggers.push_back(trigger);
                continue;
            }
            const auto ids = common_tokenize(vocab, trigger.value, false, true);
            if (ids.size() != 1) {
                params.grammar_triggers.push_back(trigger);
                continue;
            }
            if (params.preserved_tokens.count(ids[0]) == 0) {
                throw std::runtime_error("grammar trigger word is not a preserved token: " + trigger.value);
            }
            params.grammar_triggers.push_back({COMMON_GRAMMAR_TRIGGER_TYPE_TOKEN, trigger.value, ids[0]});
        }
        if (params.grammar_lazy && params.grammar_triggers.empty()) {
            throw std::runtime_error("the template's lazy grammar names no trigger");
        }
        params.generation_prompt = chat.generation_prompt;
        if (!chat.thinking_end_tags.empty()) {
            params.reasoning_budget_tokens = turn->think_budget;
            params.reasoning_budget_start = common_tokenize(vocab, chat.thinking_start_tag, false, true);
            for (const auto &tag : chat.thinking_end_tags) {
                if (!tag.empty()) {
                    params.reasoning_budget_end.push_back(common_tokenize(vocab, tag, false, true));
                }
            }
            if (!params.reasoning_budget_end.empty()) {
                params.reasoning_budget_forced = params.reasoning_budget_end.front();
            }
        }
        made(model, params, reply);
    }, reply);
}

// A sampler for a turn with no chat render (the media path): the knobs, and `grammar` when the
// request demands a shape (nullable).
extern "C" int continuum_turn_sampler_plain(const llama_model *model, const continuum_sampling_knobs *knobs,
        const char *grammar, continuum_chat_reply *reply) noexcept {
    return guarded([&] {
        auto params = knobbed(knobs);
        if (grammar != nullptr && grammar[0] != '\0') {
            params.grammar = common_grammar(COMMON_GRAMMAR_TYPE_OUTPUT_FORMAT, grammar);
        }
        made(model, params, reply);
    }, reply);
}

// Her next token from output `idx` of the last decode, accepted into every stage that tracks what
// she wrote (the grammar, the reasoning budget, the penalties). LLAMA_TOKEN_NULL when a stage
// refused, with why in `reply`.
extern "C" llama_token continuum_turn_sampler_sample(continuum_turn_sampler *turn, llama_context *ctx, int32_t idx,
        continuum_chat_reply *reply) noexcept {
    llama_token next = LLAMA_TOKEN_NULL;
    const int refused = guarded([&] {
        next = common_sampler_sample(turn->sampler, ctx, idx);
        common_sampler_accept(turn->sampler, next, true);
    }, reply);
    return refused == 0 ? next : LLAMA_TOKEN_NULL;
}

extern "C" void continuum_turn_sampler_free(continuum_turn_sampler *turn) noexcept {
    delete turn;
}
