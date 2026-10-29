use atomusicator_application::{CancellationToken, ports::AudioBlockStore};
use atomusicator_domain::{AudioAssetMetadata, AudioFormat};
use atomusicator_infrastructure::{
    audio_io::wav_codec::{read_wav_f32, write_block_wav_hashed},
    audio_tools::TemporaryDirectory,
    storage::float_spool::FloatSpoolStore,
};
use sha2::{Digest, Sha256};

#[test]
fn incremental_hash_matches_closed_wav_and_preserves_samples() {
    let root = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
    let store = FloatSpoolStore::new(root.0.clone());
    for channels in [1, 2] {
        let samples: Vec<f32> = (0..131074 * channels as usize)
            .map(|i| (i as f32 * 0.031).sin() * 1.2)
            .collect();
        let metadata = AudioAssetMetadata {
            format: AudioFormat::new(44100, channels).unwrap(),
            frame_count: 131074,
        };
        let mut writer = store.create("hash", metadata).unwrap();
        writer.write(&samples).unwrap();
        let source = writer.finish().unwrap();
        let path = root.0.join(format!("{channels}.wav"));
        let cancel = CancellationToken::default();
        let hash = write_block_wav_hashed(&path, source.as_ref(), &cancel).unwrap();
        assert_eq!(
            hash,
            format!("{:x}", Sha256::digest(std::fs::read(&path).unwrap()))
        );
        assert_eq!(read_wav_f32(&path).unwrap().samples(), samples);
    }
}
