// Test helper functions - matches actual config.rs structure
use authrity_apc_engine::config::*;

/// Create a minimal 1x1 model for testing
pub fn create_simple_1x1_model() -> UnifiedModel {
    UnifiedModel {
        metadata: Metadata {
            name: "Test1x1".to_string(),
            description: "Simple test".to_string(),
            version: "1.0".to_string(),
            model_type: "step_response".to_string(),
        },
        tuning: Tuning {
            prediction_horizon: 10,
            control_horizon: 3,
            sample_time: 1.0,
            solver_tolerance: 1e-6,
            max_iterations: 100,
            terminal_weight_factor: 1.0,
        },
        variables: Variables {
            cvs: vec![CvConfig {
                name: "CV1".to_string(),
                description: "Test CV".to_string(),
                units: "%".to_string(),
                weight: 1.0,
                alpha: 0.0,
                ece_factor: 1.0,
                optimization_mode: OptimizationMode::Target { value: 50.0 },
                slack_weight: 1000.0,
                is_integrating: false,
                limits: CvLimits {
                    low_low: 10.0,
                    low: 20.0,
                    target: 50.0,
                    high: 80.0,
                    high_high: 90.0,
                },
                node_ids: CvNodes {
                    pv: "CV1:PV".to_string(),
                    target: "CV1:Target".to_string(),
                    prediction: "CV1:Pred".to_string(),
                    limits: LimitNodes {
                        high: "CV1:HL".to_string(),
                        low: "CV1:LL".to_string(),
                        hh: "CV1:HH".to_string(),
                        ll: "CV1:LL".to_string(),
                    },
                },
            }],
            mvs: vec![MvConfig {
                name: "MV1".to_string(),
                description: "Test MV".to_string(),
                units: "%".to_string(),
                weight_r: 0.1,
                max_move: 5.0,
                optimization_mode: MvOptimizationMode::Target { value: 50.0 },
                target: None,
                target_weight: 0.0,
                limits: MvLimits {
                    low_low: 0.0,
                    low: 10.0,
                    high: 90.0,
                    high_high: 100.0,
                },
                node_ids: MvNodes {
                    pv: "MV1:PV".to_string(),
                    sp: "MV1:SP".to_string(),
                    op: "MV1:OP".to_string(),
                    mode: "MV1:Mode".to_string(),
                    mode_target: "MV1:ModeTarget".to_string(),
                    target: None,
                    future_plan: "MV1:Plan".to_string(),
                    limits: LimitNodes {
                        high: "MV1:HL".to_string(),
                        low: "MV1:LL".to_string(),
                        hh: "MV1:HH".to_string(),
                        ll: "MV1:LL".to_string(),
                    },
                },
            }],
            dvs: vec![],
        },
        physics: Physics {
            gain: vec![],
            tau: vec![],
            dead_time: vec![],
            gain_dv: vec![],
            tau_dv: vec![],
            dead_time_dv: vec![],
            // Simple step response: MV 1% move → CV changes ~0.5 at steady state
            // Match prediction_horizon exactly (10 steps)
            step_coefficients: vec![
                vec![vec![0.0, 0.0, 0.1, 0.25, 0.4, 0.5, 0.5, 0.5, 0.5, 0.5]],
            ],
            dv_coefficients: vec![],
        },
    }
}

/// Create a 2x2 model (2 CVs, 2 MVs) for MIMO testing
pub fn create_2x2_model() -> UnifiedModel {
    let mut model = create_simple_1x1_model();
    
    // Add second CV
    model.variables.cvs.push(CvConfig {
        name: "CV2".to_string(),
        description: "Test CV 2".to_string(),
        units: "%".to_string(),
        weight: 1.0,
        alpha: 0.0,
        ece_factor: 1.0,
        optimization_mode: OptimizationMode::Target { value: 45.0 },
        slack_weight: 1000.0,
        is_integrating: false,
        limits: CvLimits {
            low_low: 10.0,
            low: 20.0,
            target: 45.0,
            high: 80.0,
            high_high: 90.0,
        },
        node_ids: CvNodes {
            pv: "CV2:PV".to_string(),
            target: "CV2:Target".to_string(),
            prediction: "CV2:Pred".to_string(),
            limits: LimitNodes {
                high: "CV2:HL".to_string(),
                low: "CV2:LL".to_string(),
                hh: "CV2:HH".to_string(),
                ll: "CV2:LL".to_string(),
            },
        },
    });
    
    // Add second MV
    model.variables.mvs.push(MvConfig {
        name: "MV2".to_string(),
        description: "Test MV 2".to_string(),
        units: "%".to_string(),
        weight_r: 0.1,
        max_move: 5.0,
        optimization_mode: MvOptimizationMode::Target { value: 50.0 },
        target: None,
        target_weight: 0.0,
        limits: MvLimits {
            low_low: 0.0,
            low: 10.0,
            high: 90.0,
            high_high: 100.0,
        },
        node_ids: MvNodes {
            pv: "MV2:PV".to_string(),
            sp: "MV2:SP".to_string(),
            op: "MV2:OP".to_string(),
            mode: "MV2:Mode".to_string(),
            mode_target: "MV2:ModeTarget".to_string(),
            target: None,
            future_plan: "MV2:Plan".to_string(),
            limits: LimitNodes {
                high: "MV2:HL".to_string(),
                low: "MV2:LL".to_string(),
                hh: "MV2:HH".to_string(),
                ll: "MV2:LL".to_string(),
            },
        },
    });
    
    // Update step coefficients for 2x2 system
    // CV1 <- MV1: gain 0.5, CV1 <- MV2: gain 0.2 (coupling)
    // CV2 <- MV1: gain 0.3 (coupling), CV2 <- MV2: gain 0.6
    model.physics.step_coefficients = vec![
        vec![
            vec![0.0, 0.0, 0.1, 0.25, 0.4, 0.5, 0.5, 0.5, 0.5, 0.5], // CV1 <- MV1
            vec![0.0, 0.0, 0.05, 0.12, 0.18, 0.2, 0.2, 0.2, 0.2, 0.2], // CV1 <- MV2
        ],
        vec![
            vec![0.0, 0.0, 0.08, 0.2, 0.28, 0.3, 0.3, 0.3, 0.3, 0.3], // CV2 <- MV1
            vec![0.0, 0.0, 0.15, 0.35, 0.52, 0.6, 0.6, 0.6, 0.6, 0.6], // CV2 <- MV2
        ],
    ];
    
    model
}

/// Create a 1x1 model with 1 DV for disturbance testing
pub fn create_model_with_dv() -> UnifiedModel {
    let mut model = create_simple_1x1_model();
    
    // Add one DV
    model.variables.dvs.push(DvConfig {
        name: "DV1".to_string(),
        description: "Test disturbance".to_string(),
        units: "%".to_string(),
        limits: DvLimits {
            low: 0.0,
            high: 100.0,
        },
        node_ids: DvNodes {
            pv: "DV1:PV".to_string(),
            limits: DvLimitNodes {
                high: "DV1:HL".to_string(),
                low: "DV1:LL".to_string(),
            },
        },
    });
    
    // Add DV step coefficients: DV affects CV with gain -0.3
    model.physics.dv_coefficients = vec![
        vec![
            vec![0.0, 0.0, -0.08, -0.18, -0.26, -0.3, -0.3, -0.3, -0.3, -0.3], // CV1 <- DV1
        ],
    ];
    
    model
}
