use atomusicator_application::compute_policy::ComputeProfile;
use atomusicator_infrastructure::{
    audio_tools::TemporaryDirectory, preferences::ComputePreferences,
};

#[test]
fn compute_preferences_are_durable_and_corruption_is_explicit() {
    let directory = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
    let preferences = ComputePreferences::new(directory.0.clone());
    assert_eq!(preferences.load().unwrap(), ComputeProfile::Balanced);
    preferences.save(ComputeProfile::Throughput).unwrap();
    assert_eq!(preferences.load().unwrap(), ComputeProfile::Throughput);
    preferences.save(ComputeProfile::Eco).unwrap();
    assert_eq!(preferences.load().unwrap(), ComputeProfile::Eco);
    std::fs::write(directory.0.join("compute-profile.json"), br#""unknown""#).unwrap();
    assert!(preferences.load().is_err());
    std::fs::write(directory.0.join("compute-profile.json"), b"{").unwrap();
    assert!(preferences.load().is_err());
}
