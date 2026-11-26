// In-place learning's native boundary (IN-PROCESS-LEARNING.md, L2), in our own file: llama.cpp
// common's one fresh-adapter writer (common_lora_write_fresh, which llama-server's /train also uses),
// the native measurements as one row shape, and a shadow's optimizer state written to a path.
// llama.h's measurement callbacks differ per source (an adapter's rows carry no role; an optimizer's
// carry its shadow and role); here each is adapted to one `continuum_row`, so Rust reads one shape.
// Exceptions terminate on this side of the Rust FFI boundary.
#include "common.h"

#include <cstdio>
#include <exception>
#include <memory>
#include <new>
#include <string>
#include <vector>

// One measured row: where it lives, what it is held for (null for an adapter's weights), its size.
struct continuum_row {
    const char *device;
    const char *buffer_type;
    const char *role;
    size_t bytes;
    bool host;
};

// Where measured rows go: Rust's one callback and its data.
struct continuum_rows_sink {
    void (*row)(const continuum_row *row, void *data);
    void *data;
};

// A fresh shadow's geometry: the targets it adapts (comma separated) and how.
struct continuum_fresh {
    int32_t rank;
    float alpha;
    const char *targets;
    uint32_t seed;
    int32_t top_layers;
};

// What a boundary call did, as Rust reads it (safe/learn/shadow.rs `Native`). A failure native code
// throws is caught HERE: an exception through these noexcept functions is std::terminate (the 5090,
// 2026-11-26: std::bad_alloc while measuring aborted the core), and a caught one is told apart from a
// refusal, so Rust never reads a stale native failure string for it.
enum continuum_outcome : int {
    CONTINUUM_DONE = 0,
    CONTINUUM_REFUSED = 1, // native code refused; its own failure string is this call's
    CONTINUUM_NO_MEMORY = 2, // an allocation failed (std::bad_alloc)
    CONTINUUM_THREW = 3, // any other exception
};

// Run `call`, its bool answer as done or refused, a throw as the failure it is.
template <typename F> static int guarded(F call) noexcept {
    try {
        return call() ? CONTINUUM_DONE : CONTINUUM_REFUSED;
    } catch (const std::bad_alloc &) {
        return CONTINUUM_NO_MEMORY;
    } catch (...) {
        return CONTINUUM_THREW;
    }
}

static void adapter_row(const char *device, const char *buffer_type, size_t bytes, bool host, void *data) {
    const auto *sink = static_cast<const continuum_rows_sink *>(data);
    const continuum_row row{device, buffer_type, nullptr, bytes, host};
    sink->row(&row, sink->data);
}

static void optimizer_row(const llama_adapter_lora * /*shadow*/, const char *device, const char *buffer_type,
        const char *kind, size_t bytes, bool host, void *data) {
    const auto *sink = static_cast<const continuum_rows_sink *>(data);
    const continuum_row row{device, buffer_type, kind, bytes, host};
    sink->row(&row, sink->data);
}

// Write a fresh adapter at `path` for every target the model has. With a `sink` it writes nothing and
// reports each tensor's bytes there (common's dry run); without one it writes the file.
// Returns a continuum_outcome.
extern "C" int continuum_lora_write_fresh(const llama_model *model, const char *path, const continuum_fresh *fresh,
        const continuum_rows_sink *sink) noexcept {
    return guarded([&] {
        std::vector<std::string> named;
        const std::string targets(fresh->targets);
        size_t start = 0;
        while (start <= targets.size()) {
            const size_t comma = targets.find(',', start);
            const size_t end = comma == std::string::npos ? targets.size() : comma;
            if (end > start) {
                named.push_back(targets.substr(start, end - start));
            }
            if (comma == std::string::npos) {
                break;
            }
            start = comma + 1;
        }
        auto *measure = sink != nullptr ? adapter_row : nullptr;
        void *data = const_cast<continuum_rows_sink *>(sink);
        return common_lora_write_fresh(model, path, fresh->rank, fresh->alpha, named, fresh->seed, fresh->top_layers, measure, data);
    });
}

// What loading the adapter at `path` would hold, allocating nothing. Returns a continuum_outcome.
extern "C" int continuum_adapter_measure(llama_model *model, const char *path, const continuum_rows_sink *sink) noexcept {
    return guarded([&] { return llama_adapter_lora_measure(model, path, adapter_row, const_cast<continuum_rows_sink *>(sink)); });
}

// What an attached shadow learning a reply of up to `reply_rows` tokens, in a lesson of up to `lesson_rows`, would hold: its optimizer and the shared
// workspace that reply needs, allocating nothing. Always sized by the reply: a measure at the context's whole window
// reported 5,513,766,976 bytes of compute on the 0.5B at 2,048 tokens where its lesson needs 451,808,352 (2026-11-24),
// and attention priced at the served window quoted 78 to 105 GB for lessons on the 35B the M5 serves (2026-11-26).
extern "C" int continuum_opt_measure_shadow_graph(llama_context *ctx, llama_opt_params params, uint32_t reply_rows, uint32_t lesson_rows,
        const continuum_rows_sink *sink) noexcept {
    return guarded([&] { return llama_opt_measure_shadow_graph(ctx, params, reply_rows, lesson_rows, optimizer_row, const_cast<continuum_rows_sink *>(sink)); });
}

// Write `shadow`'s optimizer state to `path`, bound to `binding` (the generation's identity the caller
// also records in its manifest). Returns a continuum_outcome (refused: the reason is llama_opt_failure's).
// The file is closed whatever happens, and checked closed when it was written.
extern "C" int continuum_opt_shadow_state_write_path(llama_context *ctx, const llama_adapter_lora *shadow,
        const char *path, const char *binding) noexcept {
    std::unique_ptr<FILE, int (*)(FILE *)> file(std::fopen(path, "wb"), &std::fclose);
    if (!file) {
        return CONTINUUM_REFUSED;
    }
    return guarded([&] {
        const bool written = llama_opt_shadow_state_write(ctx, shadow, file.get(), binding);
        return std::fclose(file.release()) == 0 && written; // closed and checked on every path that returns
    });
}
