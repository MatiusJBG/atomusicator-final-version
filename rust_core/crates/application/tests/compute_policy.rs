use atomusicator_application::compute_policy::ComputeBudget;

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
