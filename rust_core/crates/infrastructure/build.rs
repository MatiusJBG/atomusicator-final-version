fn main() {
    println!("cargo:rerun-if-changed=native");
    // Preserve the pinned upstream headers. A narrowly scoped, recorded build
    // patch exposes Linear's existing zero-padding parameter independently of
    // the audio window, avoiding extra algorithm latency for finer FFT mapping.
    let original = std::fs::read_to_string("native/vendor/signalsmith-stretch.h").unwrap();
    let signature = "void configure(int nChannels, int blockSamples, int intervalSamples, bool splitComputation=false) {";
    let setup = "stft.configure(channels, channels, blockSamples, intervalSamples + 1);";
    assert_eq!(
        original.matches(signature).count(),
        1,
        "upstream configure signature changed"
    );
    assert_eq!(
        original.matches(setup).count(),
        1,
        "upstream STFT setup changed"
    );
    let patched=original.replace(signature,"void configure(int nChannels, int blockSamples, int intervalSamples, bool splitComputation=false, int fftMinSize=0) {")
        .replace(setup,"stft.configure(channels, channels, blockSamples, intervalSamples + 1, 0, 0, fftMinSize);");
    let generated = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    std::fs::write(generated.join("signalsmith-stretch.h"), patched).unwrap();
    cc::Build::new()
        .cpp(true)
        .std("c++17")
        .opt_level(3)
        .debug(false)
        .warnings(false)
        .file("native/live_pitch.cpp")
        .include(generated)
        .include("native/vendor")
        .compile("atomusicator_live_pitch");
}
