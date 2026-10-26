#![cfg(target_os = "linux")]
use crowsi_process_adapter::{Launch, ProcessBudget, systemd::Service};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
#[tokio::test(flavor = "current_thread")]
#[ignore = "requires a running local systemd user manager with resource controllers"]
async fn user_service_has_kernel_limits_and_clean_environment_before_payload() {
    let launch=Launch::new("/bin/sh").args(["-c",r#"read line; test "$line" = synthetic-stdin; test -z "$UNSELECTED_CREDENTIAL"; cg=$(cut -d: -f3 /proc/self/cgroup); cat /sys/fs/cgroup$cg/memory.max; cat /sys/fs/cgroup$cg/pids.max; cat /sys/fs/cgroup$cg/cpu.max"#]).env("PATH","/usr/bin:/bin").budget(ProcessBudget{max_memory_bytes:Some(536870912),max_processes:Some(32),cpu_quota:Some(1.0),max_file_bytes:Some(8388608)});
    let mut p = Service::spawn_public_environment(launch, Duration::from_secs(10))
        .await
        .unwrap();
    let mut input = p.take_stdin().unwrap();
    input.write_all(b"synthetic-stdin\n").await.unwrap();
    input.shutdown().await.unwrap();
    let mut output = String::new();
    p.take_stdout()
        .unwrap()
        .take(4096)
        .read_to_string(&mut output)
        .await
        .unwrap();
    assert_eq!(p.wait().await.unwrap().code(), Some(0));
    assert!(
        output.contains("536870912\n32\n100000 100000"),
        "unexpected enforced limits: {output}"
    );
}
#[tokio::test(flavor = "current_thread")]
async fn incomplete_service_budget_fails_before_launch() {
    assert!(
        Service::spawn_public_environment(
            Launch::new("/bin/false").budget(ProcessBudget {
                max_memory_bytes: Some(1),
                ..Default::default()
            }),
            Duration::from_secs(1)
        )
        .await
        .is_err()
    );
}
#[tokio::test(flavor = "current_thread")]
#[ignore = "requires a running local systemd user manager with resource controllers"]
async fn service_lifetime_terminates_the_payload_without_client_retry() {
    let b = ProcessBudget {
        max_memory_bytes: Some(536870912),
        max_processes: Some(32),
        cpu_quota: Some(1.0),
        max_file_bytes: Some(8388608),
    };
    let start = std::time::Instant::now();
    let mut p = Service::spawn_public_environment(
        Launch::new("/bin/sleep").args(["60"]).budget(b),
        Duration::from_millis(200),
    )
    .await
    .unwrap();
    p.take_stdin();
    p.take_stdout();
    p.take_stderr();
    assert_ne!(p.wait().await.unwrap().code(), Some(0));
    assert!(start.elapsed() < Duration::from_secs(5));
}
