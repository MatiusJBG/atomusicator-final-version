#include "signalsmith-stretch.h"
#include <cstdint>

namespace {
struct Input {
    const float *data;
    struct Channel { const float *data; float operator[](int i) const { return data[i * 2]; } };
    Channel operator[](int c) const { return {data + c}; }
};
struct Output {
    float *data;
    struct Channel { float *data; float &operator[](int i) const { return data[i * 2]; } };
    Channel operator[](int c) const { return {data + c}; }
};
struct Pitch {
    signalsmith::stretch::SignalsmithStretch<float> engine{42};
    Pitch(uint32_t rate, uint32_t window, uint32_t interval) {
        engine.configure(2, window, interval, false, 8192);
        engine.setFormantBase(0);
        (void)rate;
    }
};
}
extern "C" {
void *atom_pitch_create(uint32_t rate, uint32_t window, uint32_t interval) noexcept {
    try { return new Pitch(rate, window, interval); } catch (...) { return nullptr; }
}
void atom_pitch_destroy(void *pitch) noexcept { delete static_cast<Pitch *>(pitch); }
uint32_t atom_pitch_delay(void *pitch) noexcept {
    auto &engine = static_cast<Pitch *>(pitch)->engine;
    return engine.inputLatency() + engine.outputLatency();
}
uint32_t atom_pitch_output_delay(void *pitch) noexcept {
    return static_cast<Pitch *>(pitch)->engine.outputLatency();
}
void atom_pitch_set(void *pitch, float semitones, bool preserve) noexcept {
    auto &engine = static_cast<Pitch *>(pitch)->engine;
    engine.setTransposeSemitones(semitones);
    engine.setFormantFactor(1, preserve);
}
bool atom_pitch_process(void *pitch, const float *input, float *output, uint32_t frames) noexcept {
    try {
        static_cast<Pitch *>(pitch)->engine.process(Input{input}, frames, Output{output}, frames);
        return true;
    } catch (...) { return false; }
}
void atom_pitch_reset(void *pitch) noexcept { static_cast<Pitch *>(pitch)->engine.reset(); }
}
