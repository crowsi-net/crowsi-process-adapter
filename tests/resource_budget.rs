//! Real owned Linux children; no cgroup controller provisioning or customer data.
#![cfg(target_os = "linux")]
use crowsi_process_adapter::{Error, Launch, OwnedProcess, ProcessBudget};
use std::time::Duration;
#[tokio::test(flavor = "current_thread")]
async fn invalid_or_infinite_budget_is_refused_before_execution() {
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("must-not-exist");
    for budget in [
        ProcessBudget::default(),
        ProcessBudget {
            max_memory_bytes: Some(0),
            ..Default::default()
        },
        ProcessBudget {
            max_memory_bytes: Some(u64::MAX),
            ..Default::default()
        },
        ProcessBudget {
            max_processes: Some(0),
            ..Default::default()
        },
        ProcessBudget {
            cpu_quota: Some(f64::NAN),
            ..Default::default()
        },
        ProcessBudget {
            cpu_quota: Some(f64::INFINITY),
            ..Default::default()
        },
        ProcessBudget {
            cpu_quota: Some(0.0),
            ..Default::default()
        },
        ProcessBudget {
            max_file_bytes: Some(u64::MAX),
            ..Default::default()
        },
    ] {
        let launch = Launch::new("/usr/bin/touch")
            .args([marker.as_os_str()])
            .budget(budget);
        assert!(matches!(
            OwnedProcess::spawn(launch).await,
            Err(Error::InvalidLaunch)
        ));
        assert!(!marker.exists());
    }
}
#[tokio::test(flavor = "current_thread")]
async fn file_growth_cap_is_installed_before_exec_and_does_not_change_parent_limits() {
    let dir = tempfile::tempdir().unwrap();
    let bounded = dir.path().join("bounded");
    let script = "import sys\ntry:\n with open(sys.argv[1], 'wb', buffering=0) as f: f.write(b'x'*65536); f.write(b'x')\nexcept OSError:\n sys.exit(17)\nsys.exit(0)";
    let launch = Launch::new("/usr/bin/python3")
        .args([
            std::ffi::OsString::from("-c"),
            script.into(),
            bounded.as_os_str().to_owned(),
        ])
        .budget(ProcessBudget {
            max_file_bytes: Some(4096),
            ..Default::default()
        });
    let mut child = OwnedProcess::spawn(launch).await.unwrap();
    let status = tokio::time::timeout(Duration::from_secs(4), child.wait())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(status.code(), Some(17));
    assert!(std::fs::metadata(&bounded).unwrap().len() <= 4096);
    // The parent's fixture writes remain unconstrained by the child's rlimit.
    std::fs::write(dir.path().join("parent"), vec![0; 65536]).unwrap();
}
#[tokio::test(flavor = "current_thread")]
async fn zero_file_growth_budget_is_real_and_unbounded_launch_remains_explicit() {
    let dir = tempfile::tempdir().unwrap();
    let bounded = dir.path().join("zero");
    let script = "import sys\ntry:\n with open(sys.argv[1], 'wb', buffering=0) as f: f.write(b'x')\nexcept OSError:\n sys.exit(17)\nsys.exit(0)";
    let mut child = OwnedProcess::spawn(
        Launch::new("/usr/bin/python3")
            .args([
                std::ffi::OsString::from("-c"),
                script.into(),
                bounded.as_os_str().to_owned(),
            ])
            .budget(ProcessBudget {
                max_file_bytes: Some(0),
                ..Default::default()
            }),
    )
    .await
    .unwrap();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(4), child.wait())
            .await
            .unwrap()
            .unwrap()
            .code(),
        Some(17)
    );
    assert_eq!(std::fs::metadata(&bounded).unwrap().len(), 0);
    let mut ordinary = OwnedProcess::spawn(Launch::new("/bin/true")).await.unwrap();
    assert_eq!(ordinary.wait().await.unwrap().code(), Some(0));
}
