// Her genome's native edge: writing a fresh shadow (a LoRA layer she learns into). Exceptions stay
// on the C++ side of the Rust FFI boundary.
#include "common.h"
#include "llama.h"

#include <cstdio>
#include <exception>
#include <string>
#include <vector>

// The shape of a fresh shadow, laid out as Rust's `NativeShadowShape`.
struct continuum_shadow_shape {
    int32_t rank;
    float alpha;
    const char *const *targets;
    size_t n_targets;
    uint32_t seed;
    int32_t top_layers;
};

// Where a thrown message is written, laid out as Rust's `NativeMessage`.
struct continuum_message {
    char *text;
    size_t capacity;
};

// A fresh adapter for every `blk.<i>.<target>.weight` the model has: A uniform in +-1/sqrt(n_in),
// B zero, both F32, so the model's output is unchanged until the first optimizer step moves B.
// `top_layers` > 0 adapts only the last `top_layers` blocks. Returns 1 when written, 0 when native
// code refused, -1 when it threw (the message in `error`).
extern "C" int continuum_lora_write_fresh(const llama_model *model, const char *path,
        const continuum_shadow_shape *shape, continuum_message *error) noexcept {
    try {
        std::vector<std::string> names(shape->targets, shape->targets + shape->n_targets);
        return common_lora_write_fresh(model, path, shape->rank, shape->alpha, names, shape->seed, shape->top_layers) ? 1 : 0;
    } catch (const std::exception &failure) {
        std::snprintf(error->text, error->capacity, "%s", failure.what());
    } catch (...) {
        std::snprintf(error->text, error->capacity, "%s", "unknown C++ exception");
    }
    return -1;
}
