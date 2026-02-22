# APC Engine Test Suite

This folder contains integration tests for the APC engine.

## Running Tests

```powershell
cd apc_engine
cargo test              # Run all tests
cargo test dmc          # Run only DMC tests
cargo test --test dmc_basic_tests  # Run specific test file
```

## Test Organization

- `test_helpers.rs` - Shared utilities for creating test models
- `dmc_basic_tests.rs` - Core DMC algorithm tests (1x1, 2x2 control)
- `constraint_tests.rs` - MV/CV constraint handling
- `integration_tests.rs` - End-to-end controller behaviors
- `modelloader_tests.rs` - Model loading and validation

## Writing New Tests

All tests import the public API:

```rust
use apc_engine::dmc::DmcController;
use apc_engine::config::UnifiedModel;
mod test_helpers;  // Import shared helpers

#[test]
fn test_my_scenario() {
    let model = test_helpers::create_1x1_model();
    let controller = DmcController::new_from_coefficients(...);
    // Test code here
}
```

## Notes

- Tests target the public API in `apc_engine` and use helper models from `test_helpers.rs`.
