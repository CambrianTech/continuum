// Exceptions must terminate on the C++ side of the Rust FFI boundary.
#include "llama.h"
#include <cstdio>
#include <exception>
#include <new>
#include <stdexcept>

template<class Work>
static int guarded(Work work, int32_t *result, char *error, size_t capacity) noexcept {
    try {
        *result = work();
        return 0;
    } catch (const std::exception &failure) {
        std::snprintf(error, capacity, "%s", failure.what());
    } catch (...) {
        std::snprintf(error, capacity, "%s", "unknown C++ exception");
    }
    return 1;
}

extern "C" int continuum_llama_decode(llama_context *ctx, llama_batch batch,
        int32_t *result, char *error, size_t capacity) noexcept {
    return guarded([&] { return llama_decode(ctx, batch); }, result, error, capacity);
}

// Deterministic fault injection exercises the same guard without exhausting host RAM.
extern "C" int continuum_decode_boundary_test(int kind, int32_t *result,
        char *error, size_t capacity) noexcept {
    return guarded([&]() -> int32_t {
        switch (kind) {
            case 0: return 2;
            case 1: throw std::bad_alloc();
            case 2: throw std::runtime_error("compute allocation refused");
            default: throw 7;
        }
    }, result, error, capacity);
}
