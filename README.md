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
