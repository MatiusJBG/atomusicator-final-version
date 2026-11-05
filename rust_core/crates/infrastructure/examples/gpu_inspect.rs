fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut adapters = Vec::new();
    for index in 0..16 {
        let Some(adapter) = atomusicator_infrastructure::gpu_probe::adapter(index)? else {
            break;
        };
        adapters.push(adapter);
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &serde_json::json!({"scope":"DXGI budgets/current usage of the calling process; not total GPU usage", "adapters":adapters})
        )?
    );
    Ok(())
}
