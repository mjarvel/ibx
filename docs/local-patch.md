# Local patch: bounded paper login and strict history

This checkout carries our local patch on `codex/patch-ibx-bounded-lifecycle`, based
on upstream `53cfa34b9813b480f311a36c458f0a35aa4d31e2` (0.7.1). It has been validated
with offline fixtures on Windows MSVC Rust 1.97.1. No broker login or production
acceptance has been performed. Legacy connection/parser entry points are retained.

## Controlled connection contract

`EClient::connect_once(config, control)` performs a single paper-login attempt.
Run it on an owned blocking worker. `ConnectionControl::new(timeout, addresses,
hw_info)` requires a positive login budget, at most 32 explicitly resolved host
names with at most 8 IPs each, and prepared hardware information (1..=1024 bytes).
No DNS, hardware discovery, subprocess or autonomous reconnect is performed by
this path. Redirect and farm hosts must be present in the supplied address map.
Hostnames are at most 253 bytes; usernames at most 256 bytes and passwords at most
4096 bytes. Live login and disabled certificate validation are rejected.

The control is single-use for physical login. All sockets are registered with an
owned watchdog; TCP connects and blocking reads/writes poll at most every 100ms.
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
Legacy `connect` still uses native TLS. Existing dependency versions remain pinned
in Cargo.lock; the patch adds the Rustls dependency graph.

Login limits: NS/XYZ payloads at most 256KiB; retained stage buffers and aggregate
input per controlled stream at most 4MiB; at most 16 registered sockets; SRP groups
at most 8192 bits, with operand validation before big-integer construction.
FIX/FIXCOMP acknowledgements use declared-length framing, validate FIX checksums,
retain coalesced suffixes, and reject malformed/oversized frames. Compressed login
frames are inflated under output limits. Caller cancellation remains effective
through raw farm token/SRP exchanges. Missing account identity, unsupported route
ports and malformed bounded input fail rather than manufacturing a fallback.
These primitives do not establish complete broker account validation.

A successfully initialized EClient disarms only the login deadline; explicit
cancellation remains effective. Controlled hot loops never spawn native reconnect
workers and check stop independently of command queues. `try_send_control` is
nonblocking; acceptance does not prove wire transmission. Full queues reject
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
original time text, finite f64 OHLC, optional i64 volume, optional f64 WAP and
optional u32 count. Missing/empty statistics and volume/count -1 are absent; real
zero remains present. WAP -1 remains raw because its interpretation depends on
the requested price family. No range or timestamp conversion is invented.

The caller must bound multipart totals and separately delivered notices, reject
parser failures before publishing completion and preserve generation ownership.
The legacy dispatcher is not rewired to this parser by this patch.

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

The controlled filter passed 18 tests; bounded filter passed 23 (overlapping
framing/authentication tests and one existing cache test); lifecycle basics 4;
strict history 10; NS 28; FIXCOMP 16; gateway helpers 44; existing EClient API 246.
All passed at the final checkpoint. Library check and rustdoc generation also passed; existing rustdoc link/HTML warnings remain. An existing unused `event_rx` warning remains
in the native test suite. An early Windows socket fixture took roughly 120s:
shutdown on a cloned socket did not interrupt the pending read promptly. The patch
now applies 100ms read/write polls before socket registration and keeps shutdown
on the watchdog. The final lifecycle fixture group completes in under a second.
TLS fixtures verify local certificate/hostname checks, reject untrusted roots,
interrupt stalled handshakes and preserve segmented reads across idle polls.
