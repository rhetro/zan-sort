# Changelog

## [0.2.0] - 2026-09-02
### Added
- Sequential macro routing mode (`--features sequential`)
- `MacroWorkspace` for zero-allocation sequential routing
- `zan_sort_into` ownership-based API
- Miri-verified memory safety for the new sequential `MacroWorkspace`
- Single-pass sequential routing optimized for WASM, Rayon worker pools, and single-core environments

### Changed
- Internal refactoring: Extracted parallel macro routing into `zan_sort_macro_parallel` for cleaner conditional compilation
- Unified Dynamic Precision Scaling (DPS) across parallel and sequential modes
- Improved SoA bucketing safety

### Fixed
- UB in `ChunkData` initialization
- UB in scatter pointer casting
- UB in `local_buf` initialization

## [0.1.0] - 2026-05-07
- Initial release
