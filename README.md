# Zublime

[![Release](https://img.shields.io/github/v/release/lydakis/zublime?include_prereleases)](https://github.com/lydakis/zublime/releases)
[![CI](https://github.com/lydakis/zublime/actions/workflows/run_tests.yml/badge.svg)](https://github.com/lydakis/zublime/actions/workflows/run_tests.yml)

Welcome to Zublime, a high-performance, multiplayer code editor from the creators of [Atom](https://github.com/atom/atom) and [Tree-sitter](https://github.com/tree-sitter/tree-sitter).

---

### Installation

On macOS, Linux, and Windows you can [download Zublime directly](https://github.com/lydakis/zublime/releases) or install Zublime via your local package manager when available.

Other platforms are not yet available:

- Web ([tracking issue](https://github.com/zed-industries/zed/issues/5396))

### Developing Zublime

- [Building Zublime for macOS](./docs/src/development/macos.md)
- [Building Zublime for Linux](./docs/src/development/linux.md)
- [Building Zublime for Windows](./docs/src/development/windows.md)

### Contributing

See [CONTRIBUTING.md](./CONTRIBUTING.md) for ways you can contribute to Zublime.

### Licensing

License information for third party dependencies must be correctly provided for CI to pass.

We use [`cargo-about`](https://github.com/EmbarkStudios/cargo-about) to automatically comply with open source licenses. If CI is failing, check the following:

- Is it showing a `no license specified` error for a crate you've created? If so, add `publish = false` under `[package]` in your crate's Cargo.toml.
- Is the error `failed to satisfy license requirements` for a dependency? If so, first determine what license the project has and whether this system is sufficient to comply with this license's requirements. If you're unsure, ask a lawyer. Once you've verified that this system is acceptable add the license's SPDX identifier to the `accepted` array in `script/licenses/zed-licenses.toml`.
- Is `cargo-about` unable to find the license for a dependency? If so, add a clarification field at the end of `script/licenses/zed-licenses.toml`, as specified in the [cargo-about book](https://embarkstudios.github.io/cargo-about/cli/generate/config.html#crate-configuration).
