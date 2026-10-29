use atomusicator_domain::{AudioAssetMetadata, AudioFormat};
use atomusicator_infrastructure::audio_io::wav_codec::validate_block_wav_size;
#[test]
fn wav_capacity_selects_rf64_and_checks_overflow_before_inference() {
    let format = AudioFormat::new(44100, 2).unwrap();
    let maximum = (u64::from(u32::MAX) - 128) / 8;
    assert!(
        validate_block_wav_size(AudioAssetMetadata {
            format,
            frame_count: maximum
        })
        .is_ok()
    );
    assert!(
        validate_block_wav_size(AudioAssetMetadata {
            format,
            frame_count: maximum + 1
        })
        .is_ok()
    );
    assert!(
        validate_block_wav_size(AudioAssetMetadata {
            format,
            frame_count: u64::MAX
        })
        .is_err()
    );
    assert!(
        validate_block_wav_size(AudioAssetMetadata {
            format,
            frame_count: 60 * 60 * 44100
        })
        .is_ok()
    );
}
