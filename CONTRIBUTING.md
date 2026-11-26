# Contributing to `geokit`

Thank you for your interest in improving `geokit`!

## Development Setup

1. Install [Rust](https://rustup.rs/) (stable toolchain).
2. Clone the repository and run tests:

```bash
git clone https://github.com/Penguncisurga/Geokit.git
cd Geokit
cargo test
```

## Code Quality Checks

Before opening a Pull Request, please ensure all checks pass:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
```

If you modify `scripts/arcpy_runner.py` or `scripts/arcmap_runner.py`, verify syntax compatibility:
- `scripts/arcpy_runner.py` must be valid **Python 3** (ArcGIS Pro).
- `scripts/arcmap_runner.py` must remain valid **Python 2.7** (ArcGIS Desktop / ArcMap 10.x).

## Commit Messages

We follow [Conventional Commits](https://www.conventionalcommits.org/) (`feat:`, `fix:`, `docs:`, `chore:`, `test:`).

## Reporting Bugs

Please use the GitHub Bug Report template and include the output of `geokit doctor` so issues can be reproduced quickly.
