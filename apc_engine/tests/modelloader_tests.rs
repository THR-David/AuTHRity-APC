// Integration tests for modelloader.rs
use authrity_apc_engine::modelloader::{load_parametric_model, load_stepresponse_model};
use authrity_apc_engine::config::*;

// Import test helper models
mod test_helpers;

/// Create parametric model from existing test helper and add FOPDT parameters
fn create_parametric_model_with_fopdt() -> UnifiedModel {
    let mut model = test_helpers::create_2x2_model();
    model.metadata.model_type = "parametric".to_string();
    
    // Add FOPDT parameters for 2x2 model
    // With sample_time=1.0, dead_time=5.0 means response starts at step 6
    model.physics.gain = vec![
        vec![0.5, 0.2],  // CV1 ← MV1, MV2
        vec![0.3, 0.6],  // CV2 ← MV1, MV2
    ];
    model.physics.tau = vec![
        vec![6.0, 8.0],  // Time constants in seconds
        vec![7.0, 9.0],
    ];
    model.physics.dead_time = vec![
        vec![3.0, 4.0],  // Dead times in seconds (3-4 samples with sample_time=1.0)
        vec![3.5, 4.5],
    ];
    model.physics.gain_dv = vec![];
    model.physics.tau_dv = vec![];
    model.physics.dead_time_dv = vec![];
    model.physics.step_coefficients = vec![];
    model.physics.dv_coefficients = vec![];
    
    model
}

#[test]
fn test_load_parametric_model_success() {
    let model = create_parametric_model_with_fopdt();
    let result = load_parametric_model(&model);
    
    assert!(result.is_ok(), "Should successfully load parametric model: {:?}", result.err());
    
    let coeffs = result.unwrap();
    assert_eq!(coeffs.len(), 2, "Should have 2 CVs");
    assert_eq!(coeffs[0].len(), 2, "CV1 should have 2 MVs");
    assert_eq!(coeffs[1].len(), 2, "CV2 should have 2 MVs");
    assert_eq!(coeffs[0][0].len(), 10, "Should have 10 horizon steps");
    
    // Verify FOPDT curve properties
    // CV1 ← MV1: gain=0.5, tau=6.0, theta=3.0, sample_time=1.0
    let cv1_mv1 = &coeffs[0][0];
    assert_eq!(cv1_mv1[0], 0.0, "Should be 0 during dead time (t=0)");
    assert_eq!(cv1_mv1[1], 0.0, "Should be 0 during dead time (t=1)");
    assert_eq!(cv1_mv1[2], 0.0, "Should be 0 during dead time (t=2)");
    // At t=3s, we're at the dead time threshold, should start responding
    // At t=4s (index 4), past dead time (theta=3s), should be responding
    assert!(cv1_mv1[4] > 0.0, "Should start responding after dead time at t=4s, got {}", cv1_mv1[4]);
    assert!(cv1_mv1[9] > cv1_mv1[4], "FOPDT curve should be increasing");
    assert!(cv1_mv1[9] < 0.5, "Should not reach full gain yet (tau=6s), got {}", cv1_mv1[9]);
    
    println!("✓ Parametric model loaded: 2 CVs × 2 MVs × 10 steps");
}

#[test]
fn test_load_stepresponse_model_success() {
    let model = test_helpers::create_simple_1x1_model();
    let result = load_stepresponse_model(&model);
    
    assert!(result.is_ok(), "Should successfully load step response model: {:?}", result.err());
    
    let (mv_coeffs, dv_coeffs) = result.unwrap();
    assert_eq!(mv_coeffs.len(), 1, "Should have 1 CV");
    assert_eq!(mv_coeffs[0].len(), 1, "CV1 should have 1 MV");
    assert_eq!(dv_coeffs.len(), 0, "Should have no DVs");
    
    // Verify coefficients are preserved
    assert_eq!(mv_coeffs[0][0][0], 0.0);
    
    println!("✓ Step response model loaded: 1 CV × 1 MV × {} steps", mv_coeffs[0][0].len());
}

#[test]
fn test_parametric_model_wrong_type() {
    let mut model = create_parametric_model_with_fopdt();
    model.metadata.model_type = "step_response".to_string();
    
    let result = load_parametric_model(&model);
    assert!(result.is_err(), "Should reject wrong model type");
    assert!(result.unwrap_err().to_string().contains("parametric"));
}

#[test]
fn test_stepresponse_model_wrong_type() {
    let mut model = test_helpers::create_simple_1x1_model();
    model.metadata.model_type = "parametric".to_string();
    
    let result = load_stepresponse_model(&model);
    assert!(result.is_err(), "Should reject wrong model type");
    assert!(result.unwrap_err().to_string().contains("step_response"));
}

#[test]
fn test_parametric_model_dimension_mismatch() {
    let mut model = create_parametric_model_with_fopdt();
    // gain matrix has 2 rows, but tau matrix has 1 row
    model.physics.tau = vec![vec![60.0, 80.0]]; // Only 1 CV instead of 2
    
    let result = load_parametric_model(&model);
    assert!(result.is_err(), "Should reject mismatched dimensions");
    assert!(result.unwrap_err().to_string().contains("mismatch"));
}

#[test]
fn test_stepresponse_wrong_cv_count() {
    let mut model = test_helpers::create_simple_1x1_model();
    // Add a second CV to variables (model has 2 CVs but step_coefficients has only 1 CV)
    let new_cv = model.variables.cvs[0].clone();
    model.variables.cvs.push(new_cv);
    
    let result = load_stepresponse_model(&model);
    assert!(result.is_err(), "Should reject CV count mismatch");
    assert!(result.unwrap_err().to_string().contains("CV count mismatch"));
}

#[test]
fn test_stepresponse_wrong_mv_count() {
    let mut model = test_helpers::create_simple_1x1_model();
    // Add a second MV to variables (model has 2 MVs but step_coefficients has only 1 MV)
    let new_mv = model.variables.mvs[0].clone();
    model.variables.mvs.push(new_mv);
    
    let result = load_stepresponse_model(&model);
    assert!(result.is_err(), "Should reject MV count mismatch");
    assert!(result.unwrap_err().to_string().contains("MV count mismatch"));
}

#[test]
fn test_stepresponse_wrong_horizon_length() {
    let mut model = test_helpers::create_simple_1x1_model();
    // Coefficients have 10 steps, but model expects 15
    model.tuning.prediction_horizon = 15;
    
    let result = load_stepresponse_model(&model);
    assert!(result.is_err(), "Should reject horizon length mismatch");
    assert!(result.unwrap_err().to_string().contains("length mismatch"));
}

#[test]
fn test_stepresponse_with_dv_coefficients() {
    let model = test_helpers::create_model_with_dv();
    
    let result = load_stepresponse_model(&model);
    assert!(result.is_ok(), "Should successfully load model with DVs: {:?}", result.err());
    
    let (_mv_coeffs, dv_coeffs_out) = result.unwrap();
    assert_eq!(dv_coeffs_out.len(), 1, "Should have 1 CV");
    assert_eq!(dv_coeffs_out[0].len(), 1, "CV1 should have 1 DV");
    
    println!("✓ Step response model with DVs loaded: 1 CV × 1 MV × 1 DV × {} steps", dv_coeffs_out[0][0].len());
}

#[test]
fn test_parametric_negative_gain() {
    let mut model = create_parametric_model_with_fopdt();
    model.physics.gain[0][0] = -2.0; // Negative gain (valid for reverse-acting)
    
    let result = load_parametric_model(&model);
    assert!(result.is_ok(), "Negative gains are valid (reverse-acting processes)");
    
    let coeffs = result.unwrap();
    // With negative gain, after dead time (3s) response should be negative
    // At t=4s (index 4), past dead time, should be negative
    assert!(coeffs[0][0][4] < 0.0, "FOPDT with negative gain should produce negative response after dead time, got {}", coeffs[0][0][4]);
}

#[test]
fn test_parametric_zero_tau() {
    let mut model = create_parametric_model_with_fopdt();
    model.physics.tau[0][0] = 1e-10; // Near-zero time constant (pure gain, no lag)
    
    let result = load_parametric_model(&model);
    assert!(result.is_ok(), "Zero tau should be handled (pure gain)");
    
    let coeffs = result.unwrap();
    // With tau≈0, response should jump immediately to gain value after dead time
    // Dead time is 3s, sample time is 1s, so at t=4s (index 4) past dead time
    let gain = model.physics.gain[0][0];
    assert!((coeffs[0][0][4] - gain).abs() < 0.01, "Zero tau should give immediate response ≈ gain, expected {}, got {}", gain, coeffs[0][0][4]);
}

#[test]
fn test_parametric_large_dead_time() {
    let mut model = create_parametric_model_with_fopdt();
    model.physics.dead_time[0][0] = 300.0; // Dead time > entire horizon (10 steps × 1s = 10s)
    
    let result = load_parametric_model(&model);
    assert!(result.is_ok(), "Large dead time should be valid");
    
    let coeffs = result.unwrap();
    // All coefficients should be zero (response hasn't started yet)
    for i in 0..10 {
        assert_eq!(coeffs[0][0][i], 0.0, "Response should be zero during dead time at step {}", i);
    }
}
