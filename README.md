# Crowsi process adapter — 0.10.0

Exact `processkit = 3.3.4`, defaults disabled, `process-control` only.
Physical tree ownership, signals, bounded graceful shutdown and reaping are
mechanism. This package has no Work, owner identity, retries, RPC, persistence,
domain health or model policy. Callers issue their own incarnations and verify the required transport
handshake. A live PID is diagnostic evidence, never identity or Ready.

Launch accepts an absolute executable, at most 128 arguments / 64 environment
entries / 64 KiB combined launch strings. Environment inheritance is disabled.
Raw pipes are transferred through 64 KiB backpressure channels to Crowsi framing
callers. Diagnostic capture retains zero lines and bounds assembly at 64 KiB.
Caller-owned validation includes executable content identity and permitted effects.

The raw Child + setpriv candidate failed the abrupt parent-death race test and
was removed. `ProcessGroup::start(Command::kill_on_parent_death())` arms protection
before exec. TERM/KILL/tree containment are processkit operations. One owned Tokio
task drives bounded output pumps and joins the direct child; another bounded byte
bridge connects AsyncWrite to processkit's typed stdin API. It is aborted on child
exit and joined during wait. Drop aborts both and processkit tears down the group.
No task is intentionally detached. Abandoned output readers are closed only after
physical shutdown; higher-level Crowsi/application drain must happen beforehand.
`shutdown` uses a caller-selected grace (maximum 30 s) then a 2 s reap bound;
failure is typed, not falsely reported as stopped. Dropping the handle hard-kills
its group and hands the direct-child reaping obligation to processkit/Tokio.

Containment is reported from the actual group. POSIX groups do not prevent a
descendant escaping with `setsid`; cgroup v2 does. Abrupt parent death protects
only the direct child on Linux. Never advertise whole-tree abrupt-death containment
or use this mechanism as a security sandbox. Supervised owner chains require
their own verified parent-death links; arbitrary external descendants require a
separately proven containment policy before legacy cleanup is retired.

The real Linux tests cover spawn, natural exit, TERM escalation, hard kill, direct
child reap, descendant cleanup, repeated shutdown, invalid bounds, Drop and abrupt
parent death. They do not prove the final Hatter topology or replace its owner
handshake / restart / domain no-replay acceptance.
