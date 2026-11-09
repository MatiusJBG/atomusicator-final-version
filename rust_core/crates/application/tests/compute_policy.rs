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

#[test]
fn compute_limits_defaults_and_profiles_are_bounded() {
    use atomusicator_application::compute_policy::{ComputeLimits, ComputeProfile};

    let desktop = ComputeLimits::desktop();
    assert_eq!(desktop.eco, 2);
    assert_eq!(desktop.balanced, 6);
    assert_eq!(desktop.throughput, 12);
    assert_eq!(desktop, ComputeLimits::default());

    let mobile = ComputeLimits::mobile();
    assert_eq!(mobile.eco, 2);
    assert_eq!(mobile.balanced, 4);
    assert_eq!(mobile.throughput, 4);

    assert_eq!(mobile.limit_for(ComputeProfile::Eco), 2);
    assert_eq!(mobile.limit_for(ComputeProfile::Balanced), 4);
    assert_eq!(mobile.limit_for(ComputeProfile::Throughput), 4);
}

#[test]
fn mobile_compute_limits_cap_heterogeneous_core_usage() {
    use atomusicator_application::compute_policy::{ComputeLimits, ComputeProfile};

    let mobile = ComputeLimits::mobile();
    for cpus in [1, 2, 4, 8, 16] {
        let eco = ComputeBudget::for_profile_with_limits(ComputeProfile::Eco, cpus, false, mobile)
            .threads_per_inference();
        let balanced = ComputeBudget::for_profile_with_limits(
            ComputeProfile::Balanced,
            cpus,
            false,
            mobile,
        )
        .threads_per_inference();
        let throughput = ComputeBudget::for_profile_with_limits(
            ComputeProfile::Throughput,
            cpus,
            false,
            mobile,
        )
        .threads_per_inference();

        assert!(eco <= 2);
        assert!(balanced <= 4);
        assert!(throughput <= 4);
    }

    // On standard 8-core ARM SoC (4 LITTLE + 4 big cores)
    assert_eq!(
        ComputeBudget::for_profile_with_limits(ComputeProfile::Eco, 8, false, mobile)
            .threads_per_inference(),
        2
    );
    assert_eq!(
        ComputeBudget::for_profile_with_limits(ComputeProfile::Balanced, 8, false, mobile)
            .threads_per_inference(),
        4
    );
    assert_eq!(
        ComputeBudget::for_profile_with_limits(ComputeProfile::Throughput, 8, false, mobile)
            .threads_per_inference(),
        4
    );

    // During playback, throughput is halved to prevent audio stutter
    assert_eq!(
        ComputeBudget::for_profile_with_limits(ComputeProfile::Throughput, 8, true, mobile)
            .threads_per_inference(),
        2
    );
    assert_eq!(
        ComputeBudget::for_profile_with_limits(ComputeProfile::Balanced, 8, true, mobile)
            .threads_per_inference(),
        2
    );

    // Interactive budget with mobile limits
    assert_eq!(
        ComputeBudget::interactive_with_limits(8, mobile).threads_per_inference(),
        4
    );
    assert_eq!(
        ComputeBudget::interactive_with_limits(16, mobile).threads_per_inference(),
        4
    );
}

