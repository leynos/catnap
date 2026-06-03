# catnap

`catnap` is a GNU-like `sleep` command that reports remaining time while it
waits. It uses a monotonic clock, so wall-clock adjustments do not change the
requested wait.

```sh
catnap 90
catnap 1m 5s
```

Progress is written to standard error. Standard output remains empty for
scripts.

## Documentation

- [Documentation contents](docs/contents.md)
- [User guide](docs/users-guide.md)
- [Developer guide](docs/developers-guide.md)
