## Unreleased

- BREAKING: changed the internals of how Feature properties are encoded to save a little space.
  - <https://github.com/michaelkirk/geomedea/pull/7>
- Add geomedea read support for WebAssembly.
  - <https://github.com/michaelkirk/geomedea/pull/6>
  - You can use this crate in your rust wasm project, or to use geomedea from javascript, see the geomedea-wasm crate.
  - Introduce a `writer` feature (enabled by default).
  - Switch to a new wasm compatible decompression library for reading.

## v0.2.0 - Jan 8, 2024

- Breaking: rename `FeatureIterator.next` -> `try_next` to avoid overlap with well known `Iterator` trait.
- Misc: fix some clippy lints
- Fix: off by one error in debug assert

## v0.1.1 - Dec 6, 2023

- Fix documentation link

## v0.1.0 - Dec 6, 2023

- Initial release
