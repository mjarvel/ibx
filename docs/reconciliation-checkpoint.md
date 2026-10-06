# Upstream reconciliation checkpoint (2026-10-06)

The user authorized the fork at https://github.com/mjarvel/ibx, origin pointing
there, upstream pointing at deepentropy/ibx, clean main synchronization, and
reconciliation of our patch on dev. Publication of the validated dev patch is
authorized; a pull request is only under consideration.

This local merge integrates upstream e491575a59d9ef5d069a1e0c4afa132bce32a281
into dev, whose previous commit was 91e5aea98bf5be77aa3de5d9672cdc8a7d90a848.
The original tested patch f391500b55af8893a803c1c7c279edf1171579c9 remains on
codex/patch-ibx-bounded-lifecycle and is already preserved on the fork.

Reconciliation preserves upstream queued writes and Transport abstractions,
TLS/plain farm selection, routing tables, login metadata and order IDs. Controlled
login uses verified rustls TLS and the new login protocol without redundant DH
inside TLS. It skips detached misc-URL discovery and derives the farm LAN address
from its registered TCP socket. Controlled mode rejects on-demand farm creation
and unowned reconnect workers. Queued rustls ciphertext is retained across
backpressure without resubmitting accepted plaintext. Strict historical parsing
preserves optional raw endTime and distinct timeAvg metadata; new legacy missing
statistics use -1 sentinels, but malformed required prices still collapse to zero.
Upstream historical range strings are derived from request bounds, not verified
broker coverage.

At pause, the library and unit-test harness compile. Offline targeted tests:
controlled_ 21/21; strict_history 11/11. Existing upstream unused-import/variable
warnings remain. Full regression validation and lead review are pending. This
checkpoint has not been pushed or adopted as a production dependency.

Resume from G:\repos\worktree-rust-library1-ibx. Native mutations need the
sandbox escalation because G:\repos\ibx is outside the writable roots.
Use C:\Users\Markus\.cargo\bin\cargo.exe +stable and target\ibx-spike, with
CARGO_PROFILE_DEV_DEBUG=0, CARGO_PROFILE_TEST_DEBUG=0, CARGO_INCREMENTAL=0.
Do not run live/ignored broker tests. See the wrapper docs/ibx_pause_checkpoint.md
for remaining review, safe test filters and evidence/doc updates.