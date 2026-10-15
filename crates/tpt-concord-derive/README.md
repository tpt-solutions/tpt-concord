# tpt-concord-derive

`#[derive(SpecState)]` for [TPT Concord](https://github.com/tpt-solutions/tpt-concord): map a Rust struct onto
specification state, so an implementation's state and its specification cannot silently disagree on names or types.

Enable it through `tpt-concord-spec`'s `derive` feature, which re-exports the macro:

```toml
tpt-concord-spec = { version = "0.1", features = ["derive"] }
```

```rust
use tpt_concord_spec::SpecState;

#[derive(SpecState)]
#[spec(id = "spec.wal", version = "1.0.0", description = "WAL state")]
struct Wal {
    committed: u64,
    #[spec(rename = "in_flight")]
    pending: Vec<u64>,
    #[spec(skip)]
    scratch: Vec<u8>,
}

let skeleton = Wal::specification(); // identity + declared state variables
let state = wal.to_spec_state();     // compare against a spec step
```

Supported field types: `bool`, `String`, integers up to 64 bits, `usize`/`isize`, and `Vec<T>` of those.
Any other type is a compile error, because the derive asks each field for its `SpecValue` impl.

Structs with generics, enums and tuple structs are rejected at compile time.

## License

Dual-licensed under **MIT OR Apache-2.0**. Copyright TPT Solutions.
