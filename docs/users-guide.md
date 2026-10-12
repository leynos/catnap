# User Guide

This guide explains how to use `catnap`, a GNU-like sleep command that reports
remaining time while it waits.

## Command Syntax

Run `catnap` with one or more duration operands:

```sh
catnap NUMBER[SUFFIX]...
```

Each operand is a non-negative decimal number with an optional suffix:

- `s` for seconds, which is also the default when no suffix is supplied.
- `m` for minutes.
- `h` for hours.
- `d` for days.

Multiple operands are summed, matching GNU `sleep` style:

```sh
catnap 1m 5s
```

Durations must remain separate operands: write `catnap 5h 20m`, not
`catnap 5h20m`. When otherwise valid operands are accidentally concatenated,
the diagnostic suggests the whitespace-separated form.

The command accepts `--help` and `--version`. Invalid operands, missing
operands, unsupported suffixes, and unknown options are reported to standard
error with a non-zero exit status.

## Progress Output

`catnap` uses a monotonic stopwatch, so changes to the system wall clock do not
alter the requested wait. Progress is written to standard error; standard
output stays empty.

The progress interval depends on the full requested duration:

- Durations greater than one minute report every thirty seconds.
- Durations of one minute or less report every five seconds.
- Durations of twenty seconds or less report every second.

Remaining time is formatted for the current environment locale where a
translation is available, with English used as the fallback locale.

For contributor build and validation actions, and local development-tool setup,
see the [developer guide](developers-guide.md).
