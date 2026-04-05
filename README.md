# meedya-core

Shared core library for the [Meedya](https://github.com/MWBMPartners) suite of applications.

Written in Rust. Distributable to all Meedya apps via:
- **Rust/Tauri apps** (MeedyaDL, MeedyaConverter) — native Cargo dependency
- **Swift/SwiftUI apps** (MeedyaManager, MeedyaDB) — via `bindings/swift` Swift Package (C FFI / XCFramework)
- **Web targets** — via `bindings/wasm` (future)

## Crates

| Crate | Purpose |
|---|---|
| `meedya-metadata` | Tag schemas, read/write, TOML registry |
| `meedya-codecs` | Codec detection, format handling |
| `meedya-fingerprint` | AcoustID, ReplayGain |
| `meedya-db` | Shared database schema and models |

## Platform Support

macOS · Windows · Linux · Raspberry Pi (armv7/arm64) · iOS · iPadOS · visionOS

## App Store Compatibility

`meedya-core` compiles to native static libraries with no interpreted code,
making it fully compatible with Mac App Store and iOS App Store distribution.

## License

MIT
