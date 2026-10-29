use atomusicator_application::compute_policy::ComputeBudget;

#[test]
fn profile_names_are_explicit_and_unknown_settings_fail() {
    use atomusicator_application::compute_policy::ComputeProfile;
    assert_eq!(
        "eco".parse::<ComputeProfile>().unwrap(),
        ComputeProfile::Eco
    );
    assert_eq!(
        "balanced".parse::<ComputeProfile>().unwrap(),
        ComputeProfile::Balanced
    );
    assert_eq!(
        "throughput".parse::<ComputeProfile>().unwrap(),
        ComputeProfile::Throughput
    );
    assert!("maximum".parse::<ComputeProfile>().is_err());
    assert!("".parse::<ComputeProfile>().is_err());
}

#[test]
fn profiles_reserve_capacity_and_realtime_playback_caps_throughput() {
    use atomusicator_application::compute_policy::ComputeProfile;
    for cpus in [0, 1, 2, 4, 8, 16, 128] {
        for profile in [
            ComputeProfile::Eco,
            ComputeProfile::Balanced,
            ComputeProfile::Throughput,
        ] {
            let idle = ComputeBudget::for_profile(profile, cpus, false).threads_per_inference();
            let playing = ComputeBudget::for_profile(profile, cpus, true).threads_per_inference();
            assert!((1..=cpus.clamp(1, 12)).contains(&idle));
            assert!(playing <= idle && playing <= 6);
        }
    }
    assert_eq!(
        ComputeBudget::for_profile(ComputeProfile::Eco, 16, false).threads_per_inference(),
        2
    );
    assert_eq!(
        ComputeBudget::for_profile(ComputeProfile::Balanced, 16, false).threads_per_inference(),
        6
    );
    assert_eq!(
        ComputeBudget::for_profile(ComputeProfile::Throughput, 16, false).threads_per_inference(),
        12
    );
    assert_eq!(
        ComputeBudget::for_profile(ComputeProfile::Throughput, 16, true).threads_per_inference(),
        6
    );
    assert_eq!(
        ComputeBudget::for_profile(ComputeProfile::Balanced, 16, true).threads_per_inference(),
        3
    );
}

#[test]
fn interactive_budget_is_bounded_on_small_and_large_hosts() {
    assert_eq!(ComputeBudget::interactive(1).threads_per_inference(), 1);
    assert_eq!(ComputeBudget::interactive(2).threads_per_inference(), 1);
    assert_eq!(ComputeBudget::interactive(8).threads_per_inference(), 6);
    assert_eq!(ComputeBudget::interactive(128).threads_per_inference(), 6);
}

#[test]
fn concurrent_branches_share_one_budget_instead_of_multiplying_it() {
    let budget = ComputeBudget::interactive(16);
    assert_eq!(budget.threads_for_branches(2).unwrap(), 3);
    assert!(budget.threads_for_branches(0).is_err());
    assert!(budget.threads_for_branches(7).is_err());
}
