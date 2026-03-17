## 1. Setup & Dependencies

- [x] 1.1 Add `flate2` and `tar` dependencies to `Cargo.toml` under the `setup` feature
- [x] 1.2 Remove `include_dir` dependency if no longer used by other components

## 2. Build Integration

- [x] 2.1 Create or update `build.rs` to compress the `assets/` directory into a `.tar.gz` archive
- [x] 2.2 Ensure `build.rs` uses `cargo:rerun-if-changed=assets/` to trigger rebuilds correctly
- [x] 2.3 Verify the compressed archive is generated in the `OUT_DIR`

## 3. Implementation

- [x] 3.1 Replace `include_dir!` macro in `src/setup.rs` with `include_bytes!` for the generated archive
- [x] 3.2 Implement decompression logic in `src/setup.rs` using `flate2` and `tar`
- [x] 3.3 Update asset extraction loop to iterate over tar archive entries instead of `include_dir` entries

## 4. Verification

- [x] 4.1 Run `cargo clippy` and fix any warnings
- [x] 4.2 Run `cargo test --features setup` to verify functionality
- [x] 4.3 Build release binary with `--features setup` and compare size with current version
- [x] 4.4 Manually verify the `setup` command correctly extracts all assets
