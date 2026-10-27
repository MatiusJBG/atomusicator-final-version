use atomusicator_application::resource_admission::{ResourcePool, Resources};
use std::sync::Arc;
#[test]
fn admission_counts_all_live_leases_and_returns_capacity_after_failures() {
    let pool = Arc::new(ResourcePool::new(Resources {
        threads: 6,
        memory_bytes: 1024,
    }));
    let first = pool
        .reserve(Resources {
            threads: 3,
            memory_bytes: 700,
        })
        .unwrap();
    assert!(
        pool.reserve(Resources {
            threads: 1,
            memory_bytes: 400
        })
        .is_err()
    );
    let second = pool
        .reserve(Resources {
            threads: 3,
            memory_bytes: 300,
        })
        .unwrap();
    assert!(
        pool.reserve(Resources {
            threads: 1,
            memory_bytes: 0
        })
        .is_err()
    );
    drop(first);
    assert_eq!(
        pool.used().unwrap(),
        Resources {
            threads: 3,
            memory_bytes: 300
        }
    );
    drop(second);
    assert_eq!(
        pool.used().unwrap(),
        Resources {
            threads: 0,
            memory_bytes: 0
        }
    );
    assert!(
        pool.reserve(Resources {
            threads: 0,
            memory_bytes: 0
        })
        .is_err()
    );
}
