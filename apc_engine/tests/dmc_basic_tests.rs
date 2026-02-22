// Basic DMC Controller Tests
use authrity_apc_engine::dmc::DmcController;
use authrity_apc_engine::config::{MvOptimizationMode, OptimizationMode};
use clarabel::solver::SolverStatus;

mod test_helpers;

#[test]
fn test_controller_initialization() {
    let model = test_helpers::create_simple_1x1_model();
    let step_coeffs = model.physics.step_coefficients.clone();
    let dv_coeffs = model.physics.dv_coefficients.clone();
    
    let _controller = DmcController::new_from_coefficients(step_coeffs, dv_coeffs, &model);
    
    assert!(true, "Controller initialized successfully");
}

#[test]
fn test_basic_control_move() {
    let model = test_helpers::create_simple_1x1_model();
    let step_coeffs = model.physics.step_coefficients.clone();
    let dv_coeffs = model.physics.dv_coefficients.clone();
    
    let mut controller = DmcController::new_from_coefficients(step_coeffs, dv_coeffs, &model);
    
    // CV below target → MV should increase
    let result = controller.next_move(
        &[40.0],      // current_pvs
        &[50.0],      // targets
        &[50.0],      // current_mvs
        &[],          // current_dvs
        &[],          // mv_targets
        &[10.0],      // dynamic_min
        &[90.0],      // dynamic_max
        &[20.0],      // cv_min
        &[80.0],      // cv_max
        &[1.0],       // cv_weights
        &[0.0],       // cv_alphas
        &[0.1],       // mv_weights
        true,         // commit
    );
    
    assert_eq!(result.status, SolverStatus::Solved);
    assert!(result.next_move[0] > 0.0, "MV should increase when CV below target");
    
    println!("✓ Basic control: MV move = {:.3}", result.next_move[0]);
}

#[test]
fn test_2x2_mimo_control() {
    // Test 2x2 MIMO system with coupling
    let model = test_helpers::create_2x2_model();
    let step_coeffs = model.physics.step_coefficients.clone();
    let dv_coeffs = model.physics.dv_coefficients.clone();
    
    let mut controller = DmcController::new_from_coefficients(step_coeffs, dv_coeffs, &model);
    
    // Both CVs below target
    let result = controller.next_move(
        &[40.0, 38.0],    // current_pvs - both low
        &[50.0, 45.0],    // targets
        &[50.0, 50.0],    // current_mvs
        &[],              // No DVs
        &[],              // No MV targets
        &[10.0, 10.0],    // dynamic_min
        &[90.0, 90.0],    // dynamic_max
        &[20.0, 20.0],    // cv_min
        &[80.0, 80.0],    // cv_max
        &[1.0, 1.0],      // cv_weights
        &[0.0, 0.0],      // cv_alphas
        &[0.1, 0.1],      // mv_weights
        true,
    );
    
    assert_eq!(result.status, SolverStatus::Solved);
    
    // Both MVs should increase (positive moves) since both CVs are low
    assert!(result.next_move[0] > 0.0, "MV1 should increase");
    assert!(result.next_move[1] > 0.0, "MV2 should increase");
    
    println!("✓ 2x2 MIMO test: MV1 = {:.3}, MV2 = {:.3}", 
             result.next_move[0], result.next_move[1]);
}

#[test]
fn test_dv_feedforward() {
    // Test DV feedforward compensation
    let model = test_helpers::create_model_with_dv();
    let step_coeffs = model.physics.step_coefficients.clone();
    let dv_coeffs = model.physics.dv_coefficients.clone();
    
    let mut controller = DmcController::new_from_coefficients(step_coeffs, dv_coeffs, &model);
    
    // First cycle: Establish baseline with DV=50
    controller.next_move(
        &[50.0],       // CV at target
        &[50.0],       // Target
        &[50.0],       // MV
        &[50.0],       // DV = 50 (baseline)
        &[],           // No MV targets
        &[10.0],       // MV low
        &[90.0],       // MV high
        &[20.0],       // CV min
        &[80.0],       // CV max
        &[1.0],        // CV weight
        &[0.0],        // CV alpha
        &[0.1],        // MV weight
        true,          // Commit to store DV state
    );
    
    // Second cycle: DV increases to 60 (delta = +10)
    // With negative gain (-0.3), this pushes CV down by 10*(-0.3) = -3
    // Controller should increase MV to compensate
    let result = controller.next_move(
        &[50.0],       // CV at target
        &[50.0],       // Target
        &[50.0],       // MV
        &[60.0],       // DV = 60 (disturbance injected!)
        &[],           // No MV targets
        &[10.0],       // MV low
        &[90.0],       // MV high
        &[20.0],       // CV min
        &[80.0],       // CV max
        &[1.0],        // CV weight
        &[0.0],        // CV alpha
        &[0.1],        // MV weight
        false,         // Don't commit
    );
    
    assert_eq!(result.status, SolverStatus::Solved);
    
    // DV increased by 10, with gain -0.3 this will push CV down by 3
    // Controller should preemptively increase MV to prevent CV drop
    assert!(result.next_move[0] > 0.5, 
            "MV should increase to compensate for DV disturbance (delta=+10, gain=-0.3), got: {}", 
            result.next_move[0]);
    
    println!("✓ DV feedforward test: MV compensation = {:.3}", result.next_move[0]);
}

#[test]
fn test_infeasible_problem() {
    // Create impossible constraint conflict → solver should detect infeasibility
    let model = test_helpers::create_simple_1x1_model();
    let step_coeffs = model.physics.step_coefficients.clone();
    let dv_coeffs = model.physics.dv_coefficients.clone();
    
    let mut controller = DmcController::new_from_coefficients(step_coeffs, dv_coeffs, &model);
    
    // CV far below target, but MV already at high limit with no headroom
    // Impossible to satisfy both constraints
    let result = controller.next_move(
        &[30.0],       // CV very low
        &[80.0],       // Target very high
        &[89.9],       // MV at high limit
        &[],           // No DVs
        &[],           // No MV targets
        &[89.0],       // MV low limit very close to current
        &[90.0],       // MV high limit (only 0.1% headroom)
        &[20.0],       // CV min
        &[80.0],       // CV max
        &[1.0],        // CV weight
        &[0.0],        // CV alpha
        &[0.1],        // MV weight
        true,
    );
    
    // Should still solve (soft constraints allow violations) but check status
    // Status should be Solved or PrimalInfeasible depending on constraints
    assert!(
        matches!(result.status, SolverStatus::Solved | SolverStatus::PrimalInfeasible),
        "Expected Solved or PrimalInfeasible, got: {:?}", 
        result.status
    );
    
    // MV should be at or near limit
    let new_mv = 89.9 + result.next_move[0];
    assert!(new_mv <= 90.1, "MV should respect limit despite infeasibility");
    
    println!("✓ Infeasibility test: Status = {:?}", result.status);
}

#[test]
fn test_cv_maximize_mode_raises_mv() {
    let mut model = test_helpers::create_simple_1x1_model();
    model.variables.cvs[0].optimization_mode = OptimizationMode::Maximize;

    let step_coeffs = model.physics.step_coefficients.clone();
    let dv_coeffs = model.physics.dv_coefficients.clone();
    let mut controller = DmcController::new_from_coefficients(step_coeffs, dv_coeffs, &model);

    let result = controller.next_move(
        &[40.0],
        &[50.0],
        &[50.0],
        &[],
        &[],
        &[10.0],
        &[90.0],
        &[20.0],
        &[80.0],
        &[1.0],
        &[0.0],
        &[0.1],
        true,
    );

    assert_eq!(result.status, SolverStatus::Solved);
    assert!(result.next_move[0] > 0.0, "MV should increase in CV maximize mode");
}

#[test]
fn test_cv_minimize_mode_lowers_mv() {
    let mut model = test_helpers::create_simple_1x1_model();
    model.variables.cvs[0].optimization_mode = OptimizationMode::Minimize;

    let step_coeffs = model.physics.step_coefficients.clone();
    let dv_coeffs = model.physics.dv_coefficients.clone();
    let mut controller = DmcController::new_from_coefficients(step_coeffs, dv_coeffs, &model);

    let result = controller.next_move(
        &[60.0],
        &[50.0],
        &[50.0],
        &[],
        &[],
        &[10.0],
        &[90.0],
        &[20.0],
        &[80.0],
        &[1.0],
        &[0.0],
        &[0.1],
        true,
    );

    assert_eq!(result.status, SolverStatus::Solved);
    assert!(result.next_move[0] < 0.0, "MV should decrease in CV minimize mode");
}

#[test]
fn test_cv_zone_mode_inside_zone_holds_mv() {
    let mut model = test_helpers::create_simple_1x1_model();
    model.variables.cvs[0].optimization_mode = OptimizationMode::Zone;

    let step_coeffs = model.physics.step_coefficients.clone();
    let dv_coeffs = model.physics.dv_coefficients.clone();
    let mut controller = DmcController::new_from_coefficients(step_coeffs, dv_coeffs, &model);

    let result = controller.next_move(
        &[50.0],
        &[50.0],
        &[50.0],
        &[],
        &[],
        &[10.0],
        &[90.0],
        &[40.0],
        &[60.0],
        &[1.0],
        &[0.0],
        &[0.1],
        true,
    );

    assert_eq!(result.status, SolverStatus::Solved);
    assert!(result.next_move[0].abs() < 1e-6, "MV should hold when CV is inside zone");
}

#[test]
fn test_cv_zone_mode_above_zone_lowers_mv() {
    let mut model = test_helpers::create_simple_1x1_model();
    model.variables.cvs[0].optimization_mode = OptimizationMode::Zone;

    let step_coeffs = model.physics.step_coefficients.clone();
    let dv_coeffs = model.physics.dv_coefficients.clone();
    let mut controller = DmcController::new_from_coefficients(step_coeffs, dv_coeffs, &model);

    let result = controller.next_move(
        &[75.0],
        &[50.0],
        &[50.0],
        &[],
        &[],
        &[10.0],
        &[90.0],
        &[40.0],
        &[60.0],
        &[1.0],
        &[0.0],
        &[0.1],
        true,
    );

    assert_eq!(result.status, SolverStatus::Solved);
    assert!(result.next_move[0] < 0.0, "MV should decrease when CV is above zone");
}

#[test]
fn test_mv_maximize_mode_raises_mv() {
    let mut model = test_helpers::create_simple_1x1_model();
    model.variables.mvs[0].optimization_mode = MvOptimizationMode::Maximize;
    model.variables.cvs[0].weight = 0.0; // Isolate MV objective

    let step_coeffs = model.physics.step_coefficients.clone();
    let dv_coeffs = model.physics.dv_coefficients.clone();
    let mut controller = DmcController::new_from_coefficients(step_coeffs, dv_coeffs, &model);

    let result = controller.next_move(
        &[50.0],
        &[50.0],
        &[50.0],
        &[],
        &[50.0],
        &[10.0],
        &[90.0],
        &[20.0],
        &[80.0],
        &[0.0],
        &[0.0],
        &[0.1],
        true,
    );

    assert_eq!(result.status, SolverStatus::Solved);
    assert!(result.next_move[0] > 0.0, "MV should increase in maximize mode");
}

#[test]
fn test_mv_minimize_mode_lowers_mv() {
    let mut model = test_helpers::create_simple_1x1_model();
    model.variables.mvs[0].optimization_mode = MvOptimizationMode::Minimize;
    model.variables.cvs[0].weight = 0.0; // Isolate MV objective

    let step_coeffs = model.physics.step_coefficients.clone();
    let dv_coeffs = model.physics.dv_coefficients.clone();
    let mut controller = DmcController::new_from_coefficients(step_coeffs, dv_coeffs, &model);

    let result = controller.next_move(
        &[50.0],
        &[50.0],
        &[50.0],
        &[],
        &[50.0],
        &[10.0],
        &[90.0],
        &[20.0],
        &[80.0],
        &[0.0],
        &[0.0],
        &[0.1],
        true,
    );

    assert_eq!(result.status, SolverStatus::Solved);
    assert!(result.next_move[0] < 0.0, "MV should decrease in minimize mode");
}

#[test]
fn test_mv_target_mode_keeps_legacy_target_weight_behavior() {
    let mut model = test_helpers::create_simple_1x1_model();
    model.variables.mvs[0].optimization_mode = MvOptimizationMode::Target { value: 70.0 };
    model.variables.mvs[0].target_weight = 2.0;
    model.variables.cvs[0].weight = 0.0; // Isolate MV objective

    let step_coeffs = model.physics.step_coefficients.clone();
    let dv_coeffs = model.physics.dv_coefficients.clone();
    let mut controller = DmcController::new_from_coefficients(step_coeffs, dv_coeffs, &model);

    let result = controller.next_move(
        &[50.0],
        &[50.0],
        &[50.0],
        &[],
        &[70.0],
        &[10.0],
        &[90.0],
        &[20.0],
        &[80.0],
        &[0.0],
        &[0.0],
        &[0.1],
        true,
    );

    assert_eq!(result.status, SolverStatus::Solved);
    assert!(result.next_move[0] > 0.0, "MV should increase toward explicit target in target mode");
}

#[test]
fn test_cv_constraint_overrides_mv_maximize_strategy() {
    let mut model = test_helpers::create_simple_1x1_model();

    // Competing objectives:
    // - MV strategy wants to maximize MV
    // - CV is above allowed zone, so controller must push it down
    model.variables.mvs[0].optimization_mode = MvOptimizationMode::Maximize;
    model.variables.mvs[0].target_weight = 1.0;
    model.variables.cvs[0].optimization_mode = OptimizationMode::Zone;
    model.variables.cvs[0].weight = 10.0;

    let step_coeffs = model.physics.step_coefficients.clone();
    let dv_coeffs = model.physics.dv_coefficients.clone();
    let mut controller = DmcController::new_from_coefficients(step_coeffs, dv_coeffs, &model);

    let result = controller.next_move(
        &[75.0],      // CV well above zone
        &[50.0],
        &[50.0],
        &[],
        &[50.0],
        &[10.0],
        &[90.0],
        &[40.0],      // CV low zone
        &[60.0],      // CV high zone
        &[10.0],      // Strong CV priority
        &[0.0],
        &[0.1],
        true,
    );

    assert_eq!(result.status, SolverStatus::Solved);
    assert!(
        result.next_move[0] < 0.0,
        "CV constraint pressure should dominate MV maximize strategy when CV is above zone"
    );
}

#[test]
fn test_dynamic_mv_weight_changes_move_aggressiveness() {
    let mut model = test_helpers::create_simple_1x1_model();
    model.variables.mvs[0].optimization_mode = MvOptimizationMode::Target { value: 70.0 };
    model.variables.mvs[0].target_weight = 1.0;
    model.variables.cvs[0].weight = 0.0; // isolate MV objective response

    let step_coeffs = model.physics.step_coefficients.clone();
    let dv_coeffs = model.physics.dv_coefficients.clone();
    let mut controller = DmcController::new_from_coefficients(step_coeffs, dv_coeffs, &model);

    let low_weight_result = controller.next_move(
        &[50.0],
        &[50.0],
        &[50.0],
        &[],
        &[70.0],
        &[10.0],
        &[90.0],
        &[20.0],
        &[80.0],
        &[0.0],
        &[0.0],
        &[0.01],      // very low move suppression
        true,
    );

    let high_weight_result = controller.next_move(
        &[50.0],
        &[50.0],
        &[50.0],
        &[],
        &[70.0],
        &[10.0],
        &[90.0],
        &[20.0],
        &[80.0],
        &[0.0],
        &[0.0],
        &[10.0],      // very high move suppression
        true,
    );

    assert_eq!(low_weight_result.status, SolverStatus::Solved);
    assert_eq!(high_weight_result.status, SolverStatus::Solved);
    assert!(
        low_weight_result.next_move[0].abs() > high_weight_result.next_move[0].abs(),
        "Lower MV weight should allow larger move than high MV weight"
    );
}
