// A fresh shadow adapter for in-place learning (IN-PROCESS-LEARNING.md, L2): llama.cpp common's one
// writer (common_lora_write_fresh, which llama-server's /train also uses), reached from Rust. The
// adapter it writes leaves the model's output unchanged until the first optimizer step. Exceptions
// terminate on this side of the Rust FFI boundary.
#include "common.h"

#include <cstdio>
#include <exception>
#include <string>
#include <vector>

// Write a fresh adapter at `path` for every target the model has (`targets`, comma separated), its
// geometry `rank`, `alpha`, `seed`, and `top_layers` (0: every block). `measure` (nullable) is told
// each tensor's bytes as it is written. Returns 0 when written, 1 when refused.
extern "C" int continuum_lora_write_fresh(const llama_model *model, const char *path, int32_t rank, float alpha,
        const char *targets, uint32_t seed, int32_t top_layers, llama_adapter_memory_callback measure,
        void *user_data) noexcept {
    try {
        std::vector<std::string> named;
        std::string rest(targets);
        size_t start = 0;
        while (start <= rest.size()) {
            const size_t comma = rest.find(',', start);
            const size_t end = comma == std::string::npos ? rest.size() : comma;
            if (end > start) {
                named.push_back(rest.substr(start, end - start));
            }
            if (comma == std::string::npos) {
                break;
            }
            start = comma + 1;
        }
        return common_lora_write_fresh(model, path, rank, alpha, named, seed, top_layers, measure, user_data) ? 0 : 1;
    } catch (...) {
        return 1;
    }
}

// Write `shadow`'s optimizer state to `path`, bound to `binding` (the generation's identity the caller
// also records in its manifest). Returns 0 when written and closed, 1 when refused (the reason is
// llama_opt_failure's).
extern "C" int continuum_opt_shadow_state_write_path(llama_context *ctx, const llama_adapter_lora *shadow,
        const char *path, const char *binding) noexcept {
    FILE *file = std::fopen(path, "wb");
    if (file == nullptr) {
        return 1;
    }
    const bool written = llama_opt_shadow_state_write(ctx, shadow, file, binding);
    const bool closed = std::fclose(file) == 0;
    return written && closed ? 0 : 1;
}
