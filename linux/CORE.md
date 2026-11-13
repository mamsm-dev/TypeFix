# Linux core

This crate contains the platform-independent English/Persian word-ranking and layout-conversion
logic. It deliberately has no keyboard hooks, privileged device access, or network dependencies.

Build and run tests:

```sh
cargo test --manifest-path linux/Cargo.toml
```

The `typefix-check` binary is a developer diagnostic for testing dictionary decisions with an
explicit key mapping. It is not yet a desktop application or an input backend.
