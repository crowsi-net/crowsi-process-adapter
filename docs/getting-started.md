# Using crowsi-process-adapter

Supervise a process tree and clean it up through explicit start and shutdown operations.

## Before you start

The caller decides which executable may start and whether a restart is permitted. Restart does not replay an application command.

## First steps

Run from the repository root:

```sh
cargo test --locked
```

## How to assess the result

- Track process lifecycle and signals.
- Apply graceful termination followed by bounded cleanup.

A passing source-level check establishes only what that check observes. Keep missing configuration, unavailable services and unverified deployment paths visible.

## Continue reading

[Repository overview](../README.md)
