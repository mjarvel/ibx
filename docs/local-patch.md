# Local patch: bounded login, connection control and strict history

> Historical record of the 2026-10-06 source and branch arrangement.
> Current development uses main; main-upstream is the pristine mirror.
> Shared Git rules and [the centralized ibx changelog](https://github.com/mjarvel/rust-library1/blob/main/docs/ibx-changelog.md)
> govern current branch roles, checkpoints, contributions and later validation.
> Earlier main/dev commands below must not be used for the migrated fork.

Our fork's dev branch reconciles the local patch with upstream
`e491575a59d9ef5d069a1e0c4afa132bce32a281`. Tested source checkpoint:
`ba9cb81ec902e3822fd5be73e3db742aec4c6840`, following merge `cb0d10d`.
The original patch `f391500b55af8893a803c1c7c279edf1171579c9` against
`53cfa34b9813b480f311a36c458f0a35aa4d31e2` remains on
`codex/patch-ibx-bounded-lifecycle`. Both upstream snapshots are package 0.7.1.
Offline fixtures run on Windows MSVC Rust 1.97.1. No broker login or production
acceptance has been performed. Legacy entry points are retained.
See [fork workflow](fork-workflow.md) for branch roles and contribution guidance.
## Controlled connection contract

`Gateway::connect_once` and `EClient::connect_once(config, control)` perform one paper or live authentication attempt. Controlled live authentication accepts broker-selected mobile push or no-factor login.
Run it on an owned blocking worker. `ConnectionControl::new(timeout, addresses,
hw_info)` requires a positive login budget, at most 32 explicitly resolved host
names with at most 8 IPs each, and prepared hardware information (1..=1024 bytes).
No DNS, hardware discovery, subprocess or autonomous reconnect is performed by
this path. Redirect and farm hosts must be present in the supplied address map.
Hostnames are at most 253 bytes; usernames at most 256 bytes and passwords at most
4096 bytes. Disabled certificate validation and challenge-response callbacks are rejected before effects.

The control is single-use for physical login. All sockets are registered with an
owned watchdog. Each TCP address has a five-second connection allowance (or the
remaining login budget), separate from 100ms stop checks and blocking I/O polls.
`cancel()` signals stop without queue admission or synchronous socket shutdown.
The watchdog aborts registered sockets. Short read timeouts preserve partial
frame reads while allowing cancellation checks; idle drains expose those timeouts.
These are cooperative polling bounds, not hard real-time scheduler guarantees.

Controlled TLS uses pinned `rustls = 0.23.45` with an explicit Ring provider and
`webpki-roots = 1.0.9`. It verifies hostnames and chains against bundled Mozilla
roots, with no platform root lookup or certificate URL retrieval. This changes
trust-store semantics relative to legacy native TLS: machine-installed roots do
not affect controlled login. TLS 1.2/1.3 use Rustls safe defaults. No global crypto
provider, certificate bypass, machine trust changes or key logging is installed.
Legacy `connect` retains upstream native-TLS/plain selection. Controlled auth always uses verified TLS; controlled farms follow the upstream SSL farm list, using registered raw sockets and bounded NS encryption where appropriate. Existing dependency versions remain pinned
in Cargo.lock; the patch adds the Rustls dependency graph.

Login limits: NS/XYZ payloads at most 256KiB; retained stage buffers and aggregate
input per controlled stream at most 4MiB; at most 16 registered sockets; SRP groups
at most 8192 bits, with operand validation before big-integer construction.
FIX/FIXCOMP acknowledgements use declared-length framing, validate FIX checksums,
retain coalesced suffixes, and reject malformed/oversized frames. Compressed login
frames are inflated under output limits. Caller cancellation remains effective
through raw farm token/SRP exchanges. Controlled auth follows the new upstream protocol: no redundant DH exchange inside TLS, no encryption-refusal downgrade or automatic resend. It skips detached misc-URL discovery and derives farm LAN identity from the registered TCP socket. Missing account identity, unsupported route
ports and malformed bounded input fail rather than manufacturing a fallback.
These primitives do not establish complete broker account validation.

A successfully initialized EClient disarms only the login deadline; explicit
cancellation remains effective. Controlled hot loops reject pre-existing/on-demand farm pools, release pool transports during stop, and never spawn native reconnect
workers and check stop independently of command queues. `try_send_control` is
nonblocking; acceptance does not prove wire transmission. A controlled Shutdown stops its command batch before later issuance. Queued Rustls writes retain pending ciphertext and accepted plaintext offsets across backpressure; they never replay already accepted bytes. Full queues reject
admission. Controlled panic recovery preserves the panic for the joining caller.

`disconnect_checked(timeout)` confirms the actual engine, retained engine handles
and watchdog are joined. Timeout retains handles for a later check; an observed
panic remains a cleanup failure. Run checked cleanup on an owned worker. The caller
must also join its own login worker before replacing a session, including failed
login attempts. `ConnectionControl::join_workers` alone cannot certify disposal of
an independently owned login worker. Controlled `disconnect()` and Drop signal
stop; unfinished engine handles transfer to the control. Drop/reaper behavior is
best effort and never constitutes confirmed cleanup.

## Additive low-level bounded APIs

`protocol::ns::ns_recv_limited(reader, max_payload_bytes)` checks an announced
payload size before allocation. `protocol::fixcomp::fixcomp_decompress_limited`
checks inflated bytes and rejects lost prefixes/tails. Authentication adds
`recv_auth_start_limited`, `recv_msg_limited`, `do_srp_bounded`,
`do_srp_farm_bounded` and `do_soft_token_bounded`. These take explicit frame/group
limits and avoid diagnostic echoes of peer content. Low-level helpers do not
create cancellation themselves: callers must supply a controlled transport.
Legacy unbounded helper signatures and legacy authentication dispatch remain.

## Strict finite historical parser

`control::historical::parse_bar_response_strict(xml, max_rows, max_bytes)` is
opt-in. Byte/row admission precedes owned response allocation; XML structural
validation uses a fixed-depth borrowed stack. Unsupported XML syntax, unknown or
duplicate fields, invalid completion and missing/nonfinite OHLC fail explicitly.
`StrictHistoryError` distinguishes byte/row limits, malformed XML, unexpected,
missing, duplicate and invalid fields without echoing response text.

`StrictHistoricalResponse` contains original query ID, timezone, bars and explicit
completion. Only `eoq=true` proves a final frame. `StrictHistoricalBar` contains
original time text, optional original end-time text (`endTime`), optional finite time average (`timeAvg`, distinct from WAP), finite f64 OHLC, optional i64 volume, optional f64 WAP and
optional u32 count. Missing/empty statistics and volume/count -1 are absent; real
zero remains present. WAP -1 remains raw because its interpretation depends on
the requested price family. No range or timestamp conversion is invented.

The caller must bound multipart totals and separately delivered notices, reject
parser failures before publishing completion and preserve generation ownership.
The legacy dispatcher is not rewired to this parser by this patch. New upstream start/end strings are generated from saved request bounds; they are not broker-reported coverage. Legacy missing/malformed statistics now use -1 sentinels, but missing/malformed required prices still become zero and truncated rows can disappear. Upstream also preserves explicit multiplier text, including 1, in contract callbacks.

## Remaining production gates

This is a native prerequisite patch, not a complete direct-IBX backend. Post-login
connection/decompression buffers, reference queues and startup execution/order
caches remain legacy and require bounds. Optional event channels can lose events.
Discovery metadata, loss-aware request completion, exact account validation and
full successful controlled login through a broker remain unproven. The wrapper's
production direct connection remains disabled until its driver and these gates
are implemented. Native cache or test success is not broker acceptance.

## Offline validation

All invoked native tests were inspected for broker-free behavior. Fixtures use
socket-free parsers/dispatchers or loopback peers with fixture credentials only.
Commands use an external target directory and never launch broker-facing examples:

```powershell
$env:CARGO_PROFILE_DEV_DEBUG='0'
$env:CARGO_PROFILE_TEST_DEBUG='0'
$env:CARGO_INCREMENTAL='0'
cargo +stable test --offline --locked --lib controlled_ --target-dir <target>
cargo +stable test --offline --locked --lib bounded_ --target-dir <target>
cargo +stable test --offline --locked --lib lifecycle::tests --target-dir <target>
cargo +stable test --offline --locked --lib strict_history --target-dir <target>
cargo +stable test --offline --locked --lib protocol::ns::tests --target-dir <target>
cargo +stable test --offline --locked --lib protocol::fixcomp::tests --target-dir <target>
cargo +stable test --offline --locked --lib gateway::tests --target-dir <target>
cargo +stable test --offline --locked --lib api::client::tests --target-dir <target>
cargo +stable check --offline --locked --lib --target-dir <target>
```

The original f391500 checkpoint passed controlled_ 18, bounded_ 23, lifecycle basics
4, strict history 10, NS 28, FIXCOMP 16, gateway helpers 44 and EClient API 246.
The reconciled source ba9cb81 passed these inspected groups:

| Library filter | Passed |
| --- | ---: |
| controlled_ | 24 |
| bounded_ | 24 |
| lifecycle::tests | 4 |
| strict_history | 11 |
| protocol::ns::tests | 28 |
| protocol::fixcomp::tests | 16 |
| protocol::connection::tests | 29 |
| gateway:: | 84 |
| api::client::tests | 309 |
| engine::hot_loop::pool::tests | 6 |
| engine::hot_loop::robustness_tests | 9 |

Filters overlap; counts are not a unique-test total. Additional reviewed offline
integration targets passed: scripted_peer 4; scenario_replay 27 (7 existing ignored
cases remain ignored); hot_loop_lifecycle 2; control_plane 14; error_edge_concurrency
34; protocol_vectors 22; gw_catalog 46. Use `--features test-support` explicitly for
integration targets: Cargo's self dev-dependency feature unification did not reliably
expose the gated harness/benchmark on this Windows build. That initial compile
failure was resolved by the feature flag, without changing production features.
The first catalog run also detected our local pool failure mislabeled as broker
1100; it now logs a local reason and cancels through connection health instead.

Library check and rustdoc generation passed. Existing upstream warnings remain:
unused FixSink (library), OrderStatus/event_rx (tests), nine rustdoc link/HTML/private
link warnings, and a bin/lib ibx.pdb name collision during integration builds.
No all-target/live suite or coverage percentage is claimed. The independent wrapper
native evidence package passes seven socket-free dispatcher/parser fixtures.

An early original Windows socket fixture took roughly 120s because cloned-socket
shutdown did not promptly release pending recv. The patch applies 100ms read/write
polls before registration and watchdog-owned shutdown. Final controlled fixtures
complete in under a second. TLS fixtures verify local chain/hostname checks,
reject untrusted roots, interrupt stalled handshakes, retain partial reads and
ciphertext under backpressure, and check the captured login start/no-downgrade
protocol. They do not establish full successful broker login acceptance.
## Connection-only follow-up (2026-10-06)

`auth::session::do_ib_key_push_bounded` preserves partial frames and post-approval
carry, supports server heartbeat/TestRequest during mobile push, and admits finite
frame bytes, total wire bytes and frame count. Its deadline is checked across idle
polls. Decline, malformed factors, unsupported factors and broker close fail without
echoing challenge, URL or peer diagnostics. Legacy callback authentication remains.

ControlledIo reports cancellation as terminal ConnectionAborted at transport read,
write and flush boundaries. Public ConnectionControl::check retains Interrupted.
This distinction prevents std read_exact/read_to_end from retrying cancellation
forever. The offline stage tests found and reproduced this defect before the fix.

`Connection::poll_limited(buffer_limit, frame_limit)` admits framing sizes before
body allocation, preserves partial data and returns at most one frame per poll.
Receive errors/EOF remain sticky. Unsigned FIX checksums are verified immediately;
signed FIX requires valid signing state and checksum validation after unsigning.
`send_fix_limited(fields, frame_limit)` bounds fields/output and admits only one
pending frame (including pending Rustls ciphertext), preserving sequence/signature
state on rejection. Legacy connection methods remain available.

Our wrapper's connection-only adapter uses Gateway and the returned connections
directly, without native HotLoop/SharedState caches. It owns/joins its login worker,
checks exact account identity before readiness, bounds idle reads/inflation/control
writes, and exposes no native orders or data requests. Native low-level primitives
alone do not prove that an independently owned application worker is disposed.

New offline evidence is recorded in the wrapper progress file. Stage fixtures enter
the actual controlled login primitives on registered loopback sockets and exercise
cancellation, timeout, scoped farm joins and rejected callbacks before effects.
They do not synthesize a complete successful Gateway login or claim real broker
acceptance. Existing native HotLoop data/cache bounds remain a later milestone.

## TCP connection allowance correction (2026-10-11)

The user-run downloader trace showed DNS resolving cdc1.ibllc.com, followed by
TCP timeout at 8.17.22.31:4001 after 100.37ms, before TLS/authentication. The old
100ms stop-poll interval was incorrectly also the entire address connect budget.
Controlled connect now gives each prepared IP 5,000ms, capped by the remaining
login deadline. The already-resolved socket2 dependency starts one nonblocking
connect. The owned login worker probes it across 100ms interruptible stop checks,
without an async runtime, socket restart or hostname resolution. Pending sockets
are owned locally and drop on
stop/deadline/timeout. Successful sockets become blocking before the existing
watchdog registration and 100ms read/write configuration. Legacy connect paths
and parser-only contribution source are unchanged. Offline regressions cover a
connect pending beyond 100ms, stop/deadline disposal and per-address exhaustion.
The increased allowance does not establish live server reachability or successful
login; the next user-run acceptance attempt must confirm that separately.

## Pending TCP readiness correction (2026-10-11)

The next user-run attempt at 02:27:15 exposed a defect in our preceding
five-second connector: Windows peer_addr could already report the destination
while TCP was still pending. The trace declared TCP success after 1.66ms; the
first verified-TLS I/O then failed with WSAENOTCONN 10057. No authentication
request or SRP exchange was reached. This defect was in our local correction.

The same false success was reproduced offline against a closed loopback port,
with a regression that failed before the correction. Windows now requires
select write readiness (or exception readiness for failure) and SO_ERROR;
Unix requires poll writability and SO_ERROR. A zero-time OS probe never blocks:
the existing control condvar waits between probes on the same owned socket.
The 5,000ms per-IP budget, login deadline and responsive Stop remain unchanged.
Refusal never registers a stream or starts TLS, and the loopback/TLS/cancellation
fixtures still exercise the actual connector. No live reachability is claimed.

Windows completion semantics:
https://learn.microsoft.com/en-us/windows/win32/api/winsock2/nf-winsock2-select

Validation: the refused-port regression failed on the previous connector and
passes after correction; all 20 lifecycle-related tests and the complete locked
offline native library suite pass (1,906 passed, 6 ignored). Existing warnings
remain. Windows is exercised; the Unix polling branch is reviewed but was not
compiled or executed on this Windows host.

## 2026-10-11 - observed daily historical response metadata

The user-run downloader capture reached ResultSetBar with 24 daily bars. Our
strict parser rejected approxStep; the same response also reports data, minTick,
sizeMinTick and per-bar empty markers. Main now shares the contribution's strict
parser, preserving these optional fields explicitly without defaults, scaling or
bar omission. Unknown fields, duplicates, malformed values and caller limits
still fail. See strict-history.md for the additive public result fields.

The standalone captured response failed before correction and passes offline
after it, including all 24 bars and five empty markers. The contribution's full
offline suite passes (1,828 tests, 6 ignored). Main's integrated validation and
exact revisions are recorded centrally in rust-library1/docs/ibx-changelog.md.
This is offline parsing evidence; accepted broker output files remain pending.

Integrated main validation passed offline and locked: 1,910 tests, 6 existing
ignored tests. The main and native test-chain source trees differ only in
the parser before this reconciliation and share the corrected parser afterward.
