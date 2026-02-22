// Constraint Tests
use authrity_apc_engine::dmc::DmcController;
use clarabel::solver::SolverStatus;

mod test_helpers;

#[test]
fn test_mv_respects_limits() {
    let model = test_helpers::create_simple_1x1_model();
    let step_coeffs = model.physics.step_coefficients.clone();
    let dv_coeffs = model.physics.dv_coefficients.clone();
    
    let mut controller = DmcController::new_from_coefficients(step_coeffs, dv_coeffs, &model);
    
    // MV at 85%, limit is 90%, CV needs to go up
    let result = controller.next_move(
        &[40.0],  // CV low
        &[50.0],  // Target
        &[85.0],  // MV near high limit
        &[],      // No DVs
        &[],      // No MV targets
        &[10.0],  // MV low
        &[90.0],  // MV high (only 5% headroom)
        &[20.0],  // CV min
        &[80.0],  // CV max
        &[1.0],   // CV weight
        &[0.0],   // CV alpha
        &[0.1],   // MV weight
        true,
    );
    
    assert_eq!(result.status, SolverStatus::Solved);
    
    let new_mv = 85.0 + result.next_move[0];
    assert!(new_mv <= 90.1, "MV should respect high limit, got: {:.2}", new_mv);
    
    println!("✓ Constraint test: MV respects limits");
}
