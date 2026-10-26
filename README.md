# crowsi-process-adapter

Supervise a process tree and clean it up through explicit start and shutdown operations.

## What you can do

- Track process lifecycle and signals.
- Apply graceful termination followed by bounded cleanup.

## Current scope

The caller decides which executable may start and whether a restart is permitted. Restart does not replay an application command.

This is an independently packaged Rust library. Cargo dependencies are resolved from crates.io; runtime authority, network and storage configuration remain caller-owned.

## Getting started

Install Rust 1.97.0 or newer and make the declared dependencies available. No private dependency registry or sibling source checkout is required. Run from this repository:

```sh
cargo test --locked
```

## Documentation and source

[Usage guide](docs/getting-started.md)

[Implementation and public interfaces](src) · [Verification cases](tests) · [Contributing](CONTRIBUTING.md) · [Security reporting](SECURITY.md) · [License](LICENSE) · [Attribution notices](NOTICE)

[OSS responsibility and source completion](docs/oss-source-contract.md) defines the supported package boundary and required acceptance. It does not declare unexecuted source, distribution or production checks complete.

## Explicit resource budgets

`Launch::budget(ProcessBudget { max_memory_bytes, max_processes, cpu_quota, max_file_bytes })` requests limits through the pinned processkit mechanism. The whole-tree axes require a supported, delegated OS container and fail before execution if enforcement is unavailable. Crowsi does not silently retry an unbounded launch. Budgetless launch is a separate explicit owner choice. Empty/invalid/infinite budgets are refused before spawning.

`max_file_bytes` is an inherited per-process, per-file soft/hard limit, not a total directory or filesystem quota. Zero prevents file growth. CPU is a fraction of a core, not a wall-clock deadline; callers still own finite wait/shutdown. Linux memory controller limits do not configure swap, caller allocations or other trees.

`OwnedProcess::limit_evidence()` returns the underlying kernel evidence for requested whole-tree axes. An exit code or signal is not proof of which limit fired, and per-file limit events are not included in that evidence. Linux cgroup controller setup/delegation is an explicit runtime-environment responsibility; no host cgroup/controller provisioning is performed by the source tests.

Tests use only temporary files and owned Linux children to verify finite file growth and unchanged parent limits. They do not claim whole-tree memory/CPU enforcement in an undelegated development environment. No Cloud account, service activation or registry release is performed. Source consumers need the matching accepted source; old published bytes with the same manifest version do not acquire these APIs.

Dependency updates: [repository policy](docs/dependency-updates.md).
Registry releases: [version tags and GitHub OIDC](docs/registry-release.md).

Version 0.11.0 adds the explicit Linux `systemd::Service` backend for unprivileged
user managers where processkit cannot enforce controller limits. It requires all
budget axes and a 50 ms–120 s service lifetime. systemd applies MemoryMax,
MemorySwapMax=0, TasksMax, CPUQuota, LimitFSIZE and LimitCORE before ExecStart;
KillMode=control-group and RuntimeMaxSec bound the service independently of the
client process. There is no automatic fallback from strict processkit limits.

`spawn_public_environment` takes **public environment values only** (configuration
is visible in the service's ExecStart); credentials must travel through stdin.
`env -i` clears manager environment before the payload. Pipes are attached rather
than journal output. A random per-invocation unit name is the ownership handle;
shutdown addresses that exact unit, never a persisted PID or a global scan.
Ordinary drop schedules best-effort stop; host crash/runtime destruction can leave
the service running until RuntimeMaxSec plus its one-second stop timeout. This is
an explicit bounded-lifetime guarantee, not immediate parent-death termination.
Local systemd user-manager access and controller availability remain prerequisites.
The ignored acceptance test reads actual cgroup controller files before payload
completion; it must be run explicitly on the target host.
