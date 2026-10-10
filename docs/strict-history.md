# Opt-in strict historical bar parsing

`ibx::control::historical::parse_bar_response_strict(input, max_rows, max_bytes)`
validates one finite historical response. Existing parsing and request dispatch
keep their current behavior; callers choose this function explicitly.

The parser rejects malformed or unsupported syntax, unknown or duplicate
information, missing required prices, nonfinite prices and invalid statistics.
Byte and row limits are checked before owned response allocation. Structural
validation uses a fixed-depth borrowed stack; no dependency is added.

The result preserves the query identifier, timezone text, row order and original
daily or intraday timestamps. Prices are finite `f64` values, including negative
prices. Optional bar end timestamps and time averages remain distinct metadata.
Volume and count are optional nonnegative integers: missing, empty and unset
values become `None`, while real zero remains present. A weighted average of -1
remains a raw price because interpretation depends on the requested price family.

`is_complete` records the response's explicit final-response indication. It
does not establish requested coverage or lossless delivery. Session markers are
validated and discarded; they are not turned into coverage bounds. No response
range, timezone conversion or timestamp normalization is invented.

Callers must bound cumulative responses and retain separately delivered notices
and errors. A parser error returns no partial result. Do not publish request
completion after a parser failure. This function does not establish transport,
account or broker acceptance.

Validation uses captured responses already present in the upstream test suite,
malformed input, byte/row boundaries and property tests. No broker connection is
needed to run these tests.
