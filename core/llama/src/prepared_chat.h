#pragma once
#include <stddef.h>
#ifdef __cplusplus
extern "C" {
#endif
struct llama_model;
struct continuum_chat;
struct llama_context;
struct continuum_chat_sampler;
struct continuum_chat_sampling {
    float temperature;
    float repeat_penalty;
    int top_k;
    float top_p;
    unsigned int seed;
};
// Returned buffers are borrowed until the next call on this handle. Error text
// is a static category, never exception text or generated private content.
const char * continuum_chat_prepare(const struct llama_model * model, const char * request_json,
                                    struct continuum_chat ** out);
const char * continuum_chat_metadata(const struct continuum_chat * chat);
const char * continuum_chat_append(struct continuum_chat * chat, const char * text, size_t len,
                                  int final_chunk, size_t hidden_final_bytes, const char ** deltas_json);
void continuum_chat_free(struct continuum_chat * chat);
const char * continuum_chat_sampler_create(const struct continuum_chat * chat,
    const struct continuum_chat_sampling * settings, struct continuum_chat_sampler ** out);
const char * continuum_chat_sampler_sample(struct continuum_chat_sampler * sampler,
    struct llama_context * context, int logit_index, int * token);
int continuum_chat_sampler_preserves_token(const struct continuum_chat_sampler * sampler, int token);
void continuum_chat_sampler_free(struct continuum_chat_sampler * sampler);
#ifdef __cplusplus
}
#endif
