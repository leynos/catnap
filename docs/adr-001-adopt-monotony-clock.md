# Architectural decision record (ADR) 001: Adopt Monotony for monotonic time

## Status

Accepted. Catnap adopts Monotony for monotonic time observation while retaining
logical-time scaling and blocking sleep in Catnap.

## Date

2026-09-04.

## Context and problem statement

Catnap's runner needs monotonic elapsed-time observation, logical-second
scaling for its accelerated command mode, and blocking sleep. Its former clock
abstraction combined all three responsibilities, which made the time
observation boundary broader than necessary and coupled deterministic tests to
Catnap's clock implementation.

The dependency must remain small, usable by synchronous runner code, and
testable without waiting on wall-clock time. Monotony provides a dependency-free
`MonotonicClock` abstraction and deterministic manual-clock utilities, but it
does not own Catnap's application-specific scaling or sleeper policy.

## Decision drivers

- Keep monotonic observation narrow and reusable.
- Preserve Catnap's `--logical-second-ms` behaviour and scaling semantics.
- Keep runner tests deterministic and independent of real time.
- Avoid widening the public API with duplicate clock abstractions.

## Options considered

### Retain Catnap's combined clock

Catnap could retain one abstraction for observation, scaling, and blocking
sleep. This would avoid a dependency and a split at the runner boundary, but
would keep application policy coupled to the clock and require Catnap to
maintain its own manual-clock test implementation.

### Adopt Monotony as a drop-in replacement

Catnap could replace its former clock with Monotony directly. This would narrow
the dependency but would lose the Catnap-owned logical-time scaling and
sleeping policy that support accelerated command execution and runner tests.

### Adopt Monotony for observation and retain Catnap's sleeper

Catnap can use Monotony's `MonotonicClock` and `StdMonotonicClock` for time
observation, while `LogicalSleeper` and `ThreadLogicalSleeper` retain scaling
and blocking sleep. This keeps each responsibility at its natural ownership
boundary and preserves existing behaviour.

## Decision outcome

Catnap adopts Monotony's `MonotonicClock` trait and `StdMonotonicClock`
implementation. Catnap's `LogicalSleeper` trait remains the runner's adapter
for logical elapsed-time conversion and sleeping, and `ThreadLogicalSleeper`
remains the production implementation.

`run_sleep` receives a clock, a mutable sleeper, a progress writer, and its
configuration. The clock reports monotonic elapsed time; the sleeper converts
that elapsed time to logical time, scales requested sleeps, and blocks the
thread. Catnap re-exports the Monotony clock types for library consumers.

## Consequences

The runner has an explicit two-dependency time boundary, and Catnap no longer
maintains duplicate monotonic timestamp or real-clock types. Consumers that
used the removed Catnap clock exports must migrate to Monotony's clock types
and pass a `LogicalSleeper` to `run_sleep`.

Logical-time scaling remains Catnap-owned, so accelerated command execution and
its configuration error remain stable. The split also permits alternative
sleeper implementations without changing Monotony's observation contract.

Runner tests use Monotony's `SharedManualMonotonicClock` together with a local
advancing sleeper. The sleeper advances the shared manual clock instead of
blocking, allowing progress output and completion to be tested
deterministically without relying on wall-clock timing.

## Architectural rationale

This boundary follows dependency inversion: the runner depends on the smallest
clock contract needed for observation, while application policy remains in the
Catnap adapter that owns logical-time semantics. It also keeps the reusable
Monotony dependency independent of Catnap's command-line concerns.

## Alternatives and future changes

Monotony is the source of truth for monotonic observation. Catnap should not
reintroduce a second clock trait or move logical-time scaling into that
dependency unless a future decision record supersedes this one.
