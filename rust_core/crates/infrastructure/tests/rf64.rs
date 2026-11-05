use atomusicator_domain::{AudioAssetMetadata, AudioFormat};
use atomusicator_infrastructure::{
    audio_io::{
        wav_codec::WavBlockReader,
        wav_stream::{WaveContainer, float_header},
    },
    audio_tools::TemporaryDirectory,
};
use std::io::Write;

#[test]
fn rf64_small_float_fixture_roundtrips_and_rejects_inconsistent_sizes() {
    let root = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
    let metadata = AudioAssetMetadata {
        format: AudioFormat::new(44100, 2).unwrap(),
        frame_count: 2,
    };
    let mut bytes = float_header(metadata, WaveContainer::Rf64).unwrap();
    for value in [0.25f32, -0.5, 0.75, -1.25] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    let path = root.0.join("small.wav");
    std::fs::write(&path, &bytes).unwrap();
    let mut reader = WavBlockReader::open(&path).unwrap();
    assert_eq!(reader.metadata(), metadata);
    let mut samples = [0.0; 4];
    assert_eq!(reader.read_frames(0, &mut samples).unwrap(), 2);
    assert_eq!(samples, [0.25, -0.5, 0.75, -1.25]);
    bytes[36..44].copy_from_slice(&3u64.to_le_bytes());
    std::fs::write(&path, &bytes).unwrap();
    assert!(WavBlockReader::open(&path).is_err());
}

#[test]
fn rf64_non_sentinel_sizes_take_precedence_over_ds64() {
    let root = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
    let metadata = AudioAssetMetadata {
        format: AudioFormat::new(44100, 2).unwrap(),
        frame_count: 2,
    };
    let mut bytes = float_header(metadata, WaveContainer::Rf64).unwrap();
    for sample in [0.25f32, -0.5, 0.75, -1.25] {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    let riff = bytes.len() as u32 - 8;
    bytes[4..8].copy_from_slice(&riff.to_le_bytes());
    bytes[20..28].copy_from_slice(&u64::MAX.to_le_bytes());
    bytes[28..36].copy_from_slice(&u64::MAX.to_le_bytes());
    bytes[36..44].copy_from_slice(&3u64.to_le_bytes());
    bytes[82..86].copy_from_slice(&2u32.to_le_bytes());
    bytes[90..94].copy_from_slice(&16u32.to_le_bytes());
    let path = root.0.join("overrides.wav");
    std::fs::write(&path, bytes).unwrap();
    assert_eq!(WavBlockReader::open(&path).unwrap().metadata(), metadata);
}

#[test]
fn rf64_sparse_file_seeks_beyond_u32_frames_without_large_allocations() {
    use std::io::{Seek, SeekFrom};
    let root = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
    let path = root.0.join("large.wav");
    let metadata = AudioAssetMetadata {
        format: AudioFormat::new(44100, 2).unwrap(),
        frame_count: u64::from(u32::MAX) + 17,
    };
    let header = float_header(metadata, WaveContainer::Auto).unwrap();
    assert_eq!(&header[..4], b"RF64");
    let mut file = std::fs::File::create(&path).unwrap();
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::System::{IO::DeviceIoControl, Ioctl::FSCTL_SET_SPARSE};
        let mut returned = 0;
        // SAFETY: live file handle, documented no-buffer sparse control request,
        // and a valid output byte-count pointer; this touches only the test file.
        let ok = unsafe {
            DeviceIoControl(
                file.as_raw_handle(),
                FSCTL_SET_SPARSE,
                std::ptr::null(),
                0,
                std::ptr::null_mut(),
                0,
                &mut returned,
                std::ptr::null_mut(),
            )
        };
        assert_ne!(ok, 0, "{}", std::io::Error::last_os_error());
    }
    file.write_all(&header).unwrap();
    file.set_len(header.len() as u64 + metadata.frame_count * 8)
        .unwrap();
    file.seek(SeekFrom::End(-8)).unwrap();
    file.write_all(&0.25f32.to_le_bytes()).unwrap();
    file.write_all(&(-0.5f32).to_le_bytes()).unwrap();
    drop(file);
    let mut reader = WavBlockReader::open(&path).unwrap();
    assert_eq!(reader.metadata(), metadata);
    let mut samples = [9.0; 4];
    assert_eq!(
        reader
            .read_frames(metadata.frame_count - 1, &mut samples)
            .unwrap(),
        1
    );
    assert_eq!(samples, [0.25, -0.5, 0.0, 0.0]);
}
