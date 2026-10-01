//! Real Linux processes, exact owned PIDs only. No product migration acceptance.
#![cfg(target_os = "linux")]
use crowsi_process_adapter::{Launch, OwnedProcess};
use std::time::{Duration, Instant};
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::{AsyncBufReadExt, BufReader};

fn shell(script: &str) -> Launch {
    Launch::new("/bin/sh").args(["-c", script])
}
async fn line(process: &mut OwnedProcess) -> String {
    let mut stdout = BufReader::new(process.take_stdout().unwrap());
    let mut result = String::new();
    tokio::time::timeout(Duration::from_secs(3), stdout.read_line(&mut result))
        .await
        .unwrap()
        .unwrap();
    result.trim().to_owned()
}
fn alive(pid: u32) -> bool {
    // A dead adopted zombie is not running; direct children must additionally be reaped.
    std::fs::read_to_string(format!("/proc/{pid}/stat"))
        .ok()
        .is_some_and(|s| !s.rsplit_once(") ").unwrap().1.starts_with('Z'))
}

#[tokio::test(flavor = "current_thread")]
async fn raw_output_is_exact_beyond_capture_bound_and_unread_output_stops() {
    let mut echo = OwnedProcess::spawn(Launch::new("/bin/cat")).await.unwrap();
    let mut input = echo.take_stdin().unwrap();
    let mut output = echo.take_stdout().unwrap();
    input.write_all(b"{\"exact\":true}\n\0\xff").await.unwrap();
    input.shutdown().await.unwrap();
    drop(input);
    let mut reflected = Vec::new();
    tokio::time::timeout(Duration::from_secs(2), output.read_to_end(&mut reflected))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(reflected, b"{\"exact\":true}\n\0\xff");
    assert_eq!(echo.wait().await.unwrap().code(), Some(0));
    let mut process =
        OwnedProcess::spawn(Launch::new("/usr/bin/head").args(["-c", "200000", "/dev/zero"]))
            .await
            .unwrap();
    let mut stdout = process.take_stdout().unwrap();
    let mut bytes = Vec::new();
    tokio::time::timeout(Duration::from_secs(4), stdout.read_to_end(&mut bytes))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(bytes, vec![0; 200000]);
    assert_eq!(process.wait().await.unwrap().code(), Some(0));
    let mut process = OwnedProcess::spawn(Launch::new("/usr/bin/yes"))
        .await
        .unwrap();
    let pid = process.pid_for_diagnostics().unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    let start = Instant::now();
    let result = process.shutdown(Duration::from_millis(20)).await;
    assert!(start.elapsed() < Duration::from_secs(3));
    assert!(!alive(pid));
    result.unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn real_spawn_tree_term_kill_reap_and_no_domain_replay() {
    let mut process = OwnedProcess::spawn(shell("trap '' TERM; sleep 60 & echo $!; wait"))
        .await
        .unwrap();
    let leader = process.pid_for_diagnostics().unwrap();
    let descendant: u32 = line(&mut process).await.parse().unwrap();
    assert!(alive(leader) && alive(descendant));
    assert!(process.is_alive().unwrap());
    let start = Instant::now();
    let status = process.shutdown(Duration::from_millis(40)).await.unwrap();
    assert_ne!(status.code(), Some(0));
    assert!(start.elapsed() < Duration::from_secs(4));
    assert!(!process.is_alive().unwrap());
    assert!(!std::path::Path::new(&format!("/proc/{leader}")).exists());
    for _ in 0..100 {
        if !alive(descendant) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(!alive(descendant));
    // No implicit spawn or request retry occurs after exit or repeated shutdown.
    assert_eq!(
        process.shutdown(Duration::from_millis(40)).await.unwrap(),
        status
    );
    assert_eq!(process.pid_for_diagnostics(), None);
}

#[tokio::test(flavor = "current_thread")]
async fn natural_exit_drop_bounded_input_and_explicit_capability() {
    assert!(
        OwnedProcess::spawn(Launch::new("relative-command"))
            .await
            .is_err()
    );
    assert!(
        OwnedProcess::spawn(shell("true").args(["x".repeat(65537)]))
            .await
            .is_err()
    );
    let mut exited = OwnedProcess::spawn(shell("exit 17")).await.unwrap();
    let status = exited.wait().await.unwrap();
    assert_eq!(status.code(), Some(17));
    assert!(!exited.is_alive().unwrap());
    let mut process = OwnedProcess::spawn(shell("sleep 60 & echo $!; wait"))
        .await
        .unwrap();
    let leader = process.pid_for_diagnostics().unwrap();
    let descendant: u32 = line(&mut process).await.parse().unwrap();
    let capabilities = process.capabilities();
    assert_eq!(capabilities.parent_death_scope, "direct_child_only");
    assert!(["cgroup_v2", "process_group"].contains(&capabilities.containment));
    drop(process);
    for _ in 0..100 {
        if !alive(leader) && !alive(descendant) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(!alive(leader) && !alive(descendant));
}

#[test]
fn abrupt_parent_fixture() {
    if std::env::var_os("CROWSI_PARITY_PARENT").is_none() {
        return;
    }
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let process = OwnedProcess::spawn(Launch::new("/bin/sleep").args(["60"]))
                .await
                .unwrap();
            // stdout publication follows successful exec. No domain payload is involved.
            println!("owned-child={}", process.pid_for_diagnostics().unwrap());
            use std::io::Write;
            std::io::stdout().flush().unwrap();
            tokio::time::sleep(Duration::from_secs(60)).await;
            drop(process);
        });
}

#[tokio::test(flavor = "current_thread")]
async fn abrupt_parent_death_kills_direct_child_without_drop() {
    // Harness directly owns the parent handle. Deliberately kill only the parent,
    // not its group, so this verifies PDEATHSIG rather than normal tree shutdown.
    let mut parent = tokio::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "abrupt_parent_fixture", "--nocapture"])
        .env("CROWSI_PARITY_PARENT", "1")
        .stdout(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(parent.stdout.take().unwrap()).lines();
    let child = tokio::time::timeout(Duration::from_secs(4), async {
        while let Some(line) = lines.next_line().await.unwrap() {
            if let Some(pid) = line.strip_prefix("owned-child=") {
                return pid.parse::<u32>().unwrap();
            }
        }
        panic!("no child publication");
    })
    .await
    .unwrap();
    assert!(alive(child));
    parent.kill().await.unwrap();
    for _ in 0..100 {
        if !alive(child) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(
        !alive(child),
        "Linux direct-child parent-death guarantee failed"
    );
}
