// src/dmc.rs
use crate::config::UnifiedModel;
use nalgebra::{DMatrix, DVector};
use clarabel::solver::{
    DefaultSolver, DefaultSettings, SolverStatus, 
    IPSolver, SupportedConeT
}; 
use clarabel::algebra::CscMatrix; 

/// The Rich Output from the DMC Controller
#[derive(Debug, Clone)]
pub struct DmcResult {
    pub next_move: Vec<f64>,
    pub future_plan: Vec<f64>,
    pub predicted_pvs: Vec<f64>,
    pub status: SolverStatus,
    pub cv_bias: Vec<f64>,           // Bias correction per CV
    pub objective_value: f64,         // QP objective function value
}

pub struct DmcController {
    // Math
    matrix_a: DMatrix<f64>,
    
    // Dimensions
    p_horizon: usize,
    m_horizon: usize,
    num_cv: usize,
    num_mv: usize,
    num_dv: usize,
    num_slack: usize, // Feature 1: Slack variables (num_cv * p_horizon)

    // Optimization Matrices (now include slack variables)
    p_matrix_dense_base: DMatrix<f64>,
    a_constraints_csc: CscMatrix<f64>,
    
    // Stored Limits for dynamic updates
    mv_rate_limits: Vec<f64>, 
    #[allow(dead_code)] // Stored for future dynamic limit updates
    mv_abs_min: Vec<f64>,
    #[allow(dead_code)]
    mv_abs_max: Vec<f64>,
    
    // Weights
    q_diagonal: DVector<f64>,
    mv_base_move_weights: Vec<f64>,
    mv_target_weights: Vec<f64>,  // MV target tracking weights
    mv_optimization_modes: Vec<crate::config::MvOptimizationMode>,

    // ✅ ADDED: The memory of what we think the future looks like
    prev_prediction: Option<Vec<f64>>, 

    // Alpha for CV trajectories
    #[allow(dead_code)] // Stored for future use in next_move dynamic parameters
    cv_alphas: Vec<f64>,
    
    // Equal Concern Error (ECE) normalization factors
    cv_ece_factors: Vec<f64>,
    
    // Feature 1: Soft Constraints
    #[allow(dead_code)] // Stored for future constraint weight adjustments
    cv_slack_weights: Vec<f64>, // Penalty for each CV constraint violation
    cv_optimization_modes: Vec<crate::config::OptimizationMode>,
    
    // Feature 2: Integrating Variables
    cv_is_integrating: Vec<bool>,
    cv_steady_state_gains: Vec<Vec<f64>>, // [cv][mv] steady-state gains for disturbance calc
    
    // Feature 3: Terminal Weighting
    #[allow(dead_code)] // Stored for future terminal weighting feature
    terminal_weight_factor: f64,
    
    // DV (Disturbance Variable) Support
    dv_coefficients: Vec<Vec<Vec<f64>>>, // [cv][dv][time]
    prev_dv_values: Option<Vec<f64>>, // Last measured DV values
}

impl DmcController {
    /// Create a new DMC Controller from pre-computed step response coefficients
    /// step_coefficients[cv_idx][mv_idx][time_step]
    /// dv_coefficients[cv_idx][dv_idx][time_step]
    pub fn new_from_coefficients(
        step_coefficients: Vec<Vec<Vec<f64>>>,
        dv_coefficients: Vec<Vec<Vec<f64>>>,
        model: &UnifiedModel,
    ) -> Self {
        println!("🧠 Initializing Constrained DMC with Shift-And-Correct...");

        let num_cv = model.variables.cvs.len();
        let num_mv = model.variables.mvs.len();
        let num_dv = model.variables.dvs.len();
        let p_horizon = model.tuning.prediction_horizon as usize;
        let m_horizon = model.tuning.control_horizon as usize;
        let n_vars = num_mv * m_horizon;
        let cv_alphas: Vec<f64> = model.variables.cvs.iter().map(|cv| cv.alpha).collect();
        let cv_ece_factors: Vec<f64> = model.variables.cvs.iter().map(|cv| cv.ece_factor).collect();
        
        // Feature 1: Soft Constraints
        let cv_slack_weights: Vec<f64> = model.variables.cvs.iter()
            .map(|cv| cv.slack_weight)
            .collect();
        let cv_optimization_modes = model.variables.cvs.iter()
            .map(|cv| cv.optimization_mode.clone())
            .collect();
        
        // Feature 2: Integrating Variables - Extract steady-state gains
        let cv_is_integrating: Vec<bool> = model.variables.cvs.iter()
            .map(|cv| cv.is_integrating)
            .collect();
        
        // Calculate steady-state gains (last coefficient value)
        let mut cv_steady_state_gains = vec![vec![0.0; num_mv]; num_cv];
        for cv_idx in 0..num_cv {
            for mv_idx in 0..num_mv {
                let curve = &step_coefficients[cv_idx][mv_idx];
                if !curve.is_empty() {
                    cv_steady_state_gains[cv_idx][mv_idx] = *curve.last().unwrap();
                }
            }
        }
        
        let num_slack = num_cv * p_horizon; // One slack per CV per time step
        let _terminal_weight_factor = model.tuning.terminal_weight_factor;

        println!("   -> Dimensions: {} CVs, {} MVs, {} DVs, {} Vars, {} Slack", num_cv, num_mv, num_dv, n_vars, num_slack);
        println!("   -> Features: Soft Constraints={}, Integrators={}", 
            cv_slack_weights.iter().any(|&w| w > 0.0),
            cv_is_integrating.iter().any(|&x| x));

        // --- 1. Build Dynamic Matrix A from Step Coefficients ---
        let rows = num_cv * p_horizon;
        let cols = num_mv * m_horizon;
        let mut matrix_a = DMatrix::zeros(rows, cols);

        for cv_idx in 0..num_cv {
            for mv_idx in 0..num_mv {
                let curve = &step_coefficients[cv_idx][mv_idx];

                // 🔍 DEBUG: Print first few coefficients
                if cv_idx == 0 && mv_idx == 0 {
                    println!("   🔍 DEBUG: CV[0] MV[0] step coefficients (first 5): {:?}", &curve[0..5.min(curve.len())]);
                }

                for col_step in 0..m_horizon {
                    for row_step in col_step..p_horizon {
                        let effect_time_idx = row_step - col_step;
                        let val = curve[effect_time_idx];
                        let r = (row_step * num_cv) + cv_idx;
                        let c = (col_step * num_mv) + mv_idx;
                        matrix_a[(r, c)] = val;
                    }
                }
            }
        }

        // 🔍 DEBUG: Check A matrix first column (effect of first MV delta on all CV predictions)
        println!("   🔍 DEBUG: A matrix column 0 (first 5 rows): [{:.6}, {:.6}, {:.6}, {:.6}, {:.6}]",
            matrix_a[(0, 0)], matrix_a[(1, 0)], matrix_a[(2, 0)], matrix_a[(3, 0)], matrix_a[(4, 0)]);

        // --- 2. Build Hessian P (with Terminal Weighting) ---
        // Q Matrix (CV Weights) - Apply terminal weighting for stability (Feature 3)
        let terminal_weight_factor = model.tuning.terminal_weight_factor;
        let mut q_vec = Vec::with_capacity(rows);
        for t in 0..p_horizon {
            for cv in &model.variables.cvs {
                let weight = if t == p_horizon - 1 {
                    // Feature 3: Heavily weight last step for stability
                    cv.weight * terminal_weight_factor
                } else {
                    cv.weight
                };
                q_vec.push(weight); 
            }
        }
        let q_diagonal = DVector::from_vec(q_vec);
        let q_matrix = DMatrix::from_diagonal(&q_diagonal);
        
        println!("   ⚖️  Terminal Weighting: Last step multiplier = {:.1}x", terminal_weight_factor);

        // R Matrix (MV Weights)
        let mut mv_base_move_weights = Vec::with_capacity(num_mv);
        for mv in &model.variables.mvs {
            mv_base_move_weights.push(mv.weight_r);
        }

        let mut r_vec = Vec::with_capacity(cols);
        for _t in 0..m_horizon {
            for mv in &model.variables.mvs { r_vec.push(mv.weight_r); }
        }
        let r_matrix = DMatrix::from_diagonal(&DVector::from_vec(r_vec));

        // MV Target Weights (for economic optimization)
        let mv_target_weights: Vec<f64> = model.variables.mvs.iter()
            .map(|mv| mv.target_weight)
            .collect();
        let mv_optimization_modes = model.variables.mvs.iter()
            .map(|mv| mv.optimization_mode.clone())
            .collect();

        let a_t = matrix_a.transpose();
        let term1 = &a_t * &q_matrix * &matrix_a;
        
        // Feature 1: Augment decision vector with slack variables
        let n_delta_u = n_vars; // Original MV decision variables
        let n_slack = num_cv * p_horizon; // One slack per CV per time step
        let n_total_vars = n_delta_u + n_slack;
        
        // Build augmented P matrix: [A^T·Q·A + R,  0  ]
        //                            [    0,      ρ·I]
        // where ρ is the slack penalty weight
        let mut p_dense = DMatrix::zeros(n_total_vars, n_total_vars);
        
        // Top-left block: Original Hessian for ΔU
        let term1_plus_r = 2.0 * (term1 + r_matrix);
        for r in 0..n_delta_u {
            for c in 0..n_delta_u {
                p_dense[(r, c)] = term1_plus_r[(r, c)];
            }
        }
        
        // Bottom-right block: Slack penalties (diagonal)
        for cv_i in 0..num_cv {
            let slack_weight = cv_slack_weights[cv_i];
            for t in 0..p_horizon {
                let idx = n_delta_u + (t * num_cv) + cv_i;
                p_dense[(idx, idx)] = 2.0 * slack_weight; // Quadratic penalty: ρ·ε²
            }
        }
        
        println!("   📊 Decision Vector: {} ΔU vars + {} slack vars = {} total", n_delta_u, n_slack, n_total_vars);

        // --- 3. Build Constraints (Augmented with Soft Constraints) ---
        // Original constraints: 4 * n_delta_u (MV rate up/down, abs min/max)
        // Soft constraints: 2 * n_slack (CV ≤ high+ε, CV ≥ low-ε) + n_slack (ε ≥ 0)
        let mv_constraints = 4 * n_delta_u;
        let soft_cv_constraints = 2 * n_slack; // Upper and lower bounds
        let slack_positive_constraints = n_slack; // ε ≥ 0
        let total_constraints = mv_constraints + soft_cv_constraints + slack_positive_constraints; 
        let mut a_dense = DMatrix::zeros(total_constraints, n_total_vars);

        // Block 1 & 2: MV Rate Constraints (ΔU ≤ max_move, -ΔU ≤ max_move)
        for i in 0..n_delta_u {
            a_dense[(i, i)] = 1.0;            
            a_dense[(n_delta_u + i, i)] = -1.0;  
        }

        // Block 3 & 4: MV Absolute Constraints (cumsum(ΔU) ≤ high, -cumsum(ΔU) ≤ low)
        for mv_i in 0..num_mv {
            for step_k in 0..m_horizon {
                for step_j in 0..=step_k {
                    let col_idx = (step_j * num_mv) + mv_i;
                    let row_idx_upper = (2 * n_delta_u) + (step_k * num_mv) + mv_i;
                    let row_idx_lower = (3 * n_delta_u) + (step_k * num_mv) + mv_i;
                    a_dense[(row_idx_upper, col_idx)] = 1.0;
                    a_dense[(row_idx_lower, col_idx)] = -1.0;
                }
            }
        }
        
        // Block 5 & 6: Soft CV Constraints
        // CV_pred ≤ high + ε  =>  A·ΔU - ε ≤ high
        // CV_pred ≥ low - ε   =>  -A·ΔU - ε ≤ -low
        let soft_constraint_start = mv_constraints;
        for t in 0..p_horizon {
            for cv_i in 0..num_cv {
                let row_high = soft_constraint_start + (t * num_cv) + cv_i;
                let row_low = soft_constraint_start + n_slack + (t * num_cv) + cv_i;
                let slack_idx = n_delta_u + (t * num_cv) + cv_i;
                
                // Upper bound: A·ΔU - ε ≤ high
                for mv_i in 0..num_mv {
                    for step_k in 0..=t.min(m_horizon - 1) {
                        let col_idx = (step_k * num_mv) + mv_i;
                        let a_val = matrix_a[(t * num_cv + cv_i, col_idx)];
                        a_dense[(row_high, col_idx)] = a_val;
                    }
                }
                a_dense[(row_high, slack_idx)] = -1.0; // Subtract slack
                
                // Lower bound: -A·ΔU - ε ≤ -low
                for mv_i in 0..num_mv {
                    for step_k in 0..=t.min(m_horizon - 1) {
                        let col_idx = (step_k * num_mv) + mv_i;
                        let a_val = matrix_a[(t * num_cv + cv_i, col_idx)];
                        a_dense[(row_low, col_idx)] = -a_val;
                    }
                }
                a_dense[(row_low, slack_idx)] = -1.0; // Subtract slack
            }
        }
        
        // Block 7: Slack Non-negativity (ε ≥ 0  =>  -ε ≤ 0)
        let slack_nn_start = mv_constraints + soft_cv_constraints;
        for i in 0..n_slack {
            a_dense[(slack_nn_start + i, n_delta_u + i)] = -1.0;
        }

        let a_constraints_csc = to_csc(&a_dense);

        let mut mv_rate_limits = Vec::new();
        let mut mv_abs_min = Vec::new();
        let mut mv_abs_max = Vec::new();
        for mv in &model.variables.mvs {
            mv_rate_limits.push(mv.max_move);
            mv_abs_min.push(mv.limits.low);
            mv_abs_max.push(mv.limits.high);
        }

        Self {
            matrix_a,
            p_horizon,
            m_horizon,
            num_cv,
            num_mv,
            num_dv,
            num_slack,
            p_matrix_dense_base: p_dense,
            a_constraints_csc,
            mv_rate_limits,
            mv_abs_min,
            mv_abs_max,
            q_diagonal,
            mv_base_move_weights,
            mv_target_weights,
            mv_optimization_modes,
            prev_prediction: None,
            cv_alphas,
            cv_ece_factors,
            cv_slack_weights,
            cv_optimization_modes,
            cv_is_integrating,
            cv_steady_state_gains,
            terminal_weight_factor,
            dv_coefficients,
            prev_dv_values: None,
        }
    }

    pub fn next_move(
        &mut self, 
        current_pvs: &[f64], 
        targets: &[f64], 
        current_mvs: &[f64],
        current_dvs: &[f64],
        mv_targets: &[f64],      // MV economic targets
        dynamic_min: &[f64],     // MV lower limits
        dynamic_max: &[f64],     // MV upper limits
        cv_min: &[f64],          // CV lower limits (for soft constraints and rail-riding)
        cv_max: &[f64],          // CV upper limits (for soft constraints and rail-riding)
        cv_weights: &[f64],      // CV weights (dynamic from OPC UA)
        cv_alphas: &[f64],       // CV alphas (dynamic from OPC UA)
        mv_weights: &[f64],      // MV move suppression weights (dynamic from OPC UA)
        commit: bool 
    ) -> DmcResult {
        let n_delta_u = self.num_mv * self.m_horizon;
        let n_slack = self.num_slack;
        let n_vars = n_delta_u + n_slack;
        let total_preds = self.num_cv * self.p_horizon;

        // --- RAIL-RIDING STRATEGY: Override Targets Based on Optimization Mode ---
        // Maximize: Target = High Limit (ride the high rail)
        // Minimize: Target = Low Limit (ride the low rail)
        // Zone: If outside zone, target = violated limit; if inside, target = PV (zero error)
        // Target: Use provided target
        let mut active_targets = targets.to_vec();
        
        for cv_i in 0..self.num_cv {
            match &self.cv_optimization_modes[cv_i] {
                crate::config::OptimizationMode::Maximize => {
                    // Ride the high rail
                    active_targets[cv_i] = cv_max[cv_i];
                },
                crate::config::OptimizationMode::Minimize => {
                    // Ride the low rail
                    active_targets[cv_i] = cv_min[cv_i];
                },
                crate::config::OptimizationMode::Zone => {
                    let pv = current_pvs[cv_i];
                    if pv > cv_max[cv_i] {
                        // Above zone: push back down to high limit
                        active_targets[cv_i] = cv_max[cv_i];
                    } else if pv < cv_min[cv_i] {
                        // Below zone: push back up to low limit
                        active_targets[cv_i] = cv_min[cv_i];
                    } else {
                        // Inside zone: stay where you are (error = 0)
                        active_targets[cv_i] = pv;
                    }
                },
                crate::config::OptimizationMode::Target { .. } => {
                    // Keep original target
                    // (already in active_targets from .to_vec())
                }
            }
        }

        // --- 1. SHIFT-AND-CORRECT: Free Response with Memory ---
        let mut free_response = DVector::zeros(total_preds);
        let mut cv_bias = vec![0.0; self.num_cv];  // ✅ Store bias per CV
        
        if let Some(prev) = &self.prev_prediction {
            // STEP 1: Shift previous TOTAL prediction forward (t+1 becomes t)
            for t in 0..(self.p_horizon - 1) {
                for cv_i in 0..self.num_cv {
                    let idx = t * self.num_cv + cv_i;
                    let prev_idx = (t + 1) * self.num_cv + cv_i;
                    free_response[idx] = prev[prev_idx];
                }
            }
            // Last prediction: hold final value
            for cv_i in 0..self.num_cv {
                let last_idx = (self.p_horizon - 1) * self.num_cv + cv_i;
                let second_last_idx = (self.p_horizon - 2) * self.num_cv + cv_i;
                free_response[last_idx] = prev[second_last_idx];
            }
            
            // STEP 2: Bias Correction - Measured vs Predicted at t=0
            // CRITICAL: Use the SHIFTED prediction (free_response[cv_i]), not the old unshifted data
            // After shifting, free_response[cv_i] contains what we predicted last cycle for NOW
            for cv_i in 0..self.num_cv {
                let predicted_now = free_response[cv_i]; // ✅ FIXED: Use shifted prediction
                let actual_now = current_pvs[cv_i];
                let bias = actual_now - predicted_now;
                cv_bias[cv_i] = bias;  // ✅ Store for output
                
                // Apply bias to ENTIRE shifted horizon
                for t in 0..self.p_horizon {
                    let idx = t * self.num_cv + cv_i;
                    free_response[idx] += bias;
                }
            }
            
        } else {
            // COLD START: No previous prediction
            for t in 0..self.p_horizon {
                for cv_i in 0..self.num_cv {
                    let idx = (t * self.num_cv) + cv_i;
                    free_response[idx] = current_pvs[cv_i];
                }
            }
        }
        
        // --- ADD DV FEEDFORWARD EFFECTS ---
        if self.num_dv > 0 && current_dvs.len() == self.num_dv {
            println!("🔍 DV FEEDFORWARD DEBUG:");
            println!("   Current DVs: {:?}", current_dvs);
            
            // If DVs changed from last scan, add their predicted effects
            if let Some(prev_dvs) = &self.prev_dv_values {
                println!("   Previous DVs: {:?}", prev_dvs);
                
                for dv_i in 0..self.num_dv {
                    let dv_delta = current_dvs[dv_i] - prev_dvs[dv_i];
                    println!("   DV[{}] Delta: {:.4}", dv_i, dv_delta);
                    
                    if dv_delta.abs() > 0.001 {
                        println!("   Free response BEFORE DV correction (first 6 CVs):");
                        for cv_i in 0..self.num_cv {
                            print!("      CV[{}]: ", cv_i);
                            for t in 0..3 {
                                let idx = (t * self.num_cv) + cv_i;
                                print!("{:.6} ", free_response[idx]);
                            }
                            println!();
                        }
                    }
                    
                    // Add DV step response effect to free response
                    for cv_i in 0..self.num_cv {
                        if !self.dv_coefficients.is_empty() 
                            && cv_i < self.dv_coefficients.len() 
                            && dv_i < self.dv_coefficients[cv_i].len() {
                            let dv_curve = &self.dv_coefficients[cv_i][dv_i];
                            
                            if dv_delta.abs() > 0.001 {
                                println!("   DV[{}] step coefficients for CV[{}] (first 5): {:?}", 
                                    dv_i, cv_i, &dv_curve[0..5.min(dv_curve.len())]);
                            }
                            
                            for t in 0..self.p_horizon {
                                if t < dv_curve.len() {
                                    let idx = (t * self.num_cv) + cv_i;
                                    free_response[idx] += dv_delta * dv_curve[t];
                                }
                            }
                        }
                    }
                    
                    if dv_delta.abs() > 0.001 {
                        println!("   Free response AFTER DV correction (first 6 CVs):");
                        for cv_i in 0..self.num_cv {
                            print!("      CV[{}]: ", cv_i);
                            for t in 0..3 {
                                let idx = (t * self.num_cv) + cv_i;
                                print!("{:.6} ", free_response[idx]);
                            }
                            println!();
                        }
                    }
                }
            } else {
                println!("   No previous DV values - first cycle");
            }
            
            // Store current DV values for next iteration
            self.prev_dv_values = Some(current_dvs.to_vec());
        }

        // --- Feature 2: INPUT DISTURBANCE FOR INTEGRATING VARIABLES ---
        // For integrating CVs (e.g., levels), output bias causes drift.
        // Instead, estimate unmeasured input disturbance from model error.
        // Disturbance = (Actual - Predicted) / SteadyStateGain
        // This disturbance is then added via step response to the prediction.
        if let Some(prev) = &self.prev_prediction {
            for cv_i in 0..self.num_cv {
                if self.cv_is_integrating[cv_i] {
                    let predicted_now = prev[cv_i]; // What we predicted last cycle for NOW
                    let actual_now = current_pvs[cv_i];
                    let model_error = actual_now - predicted_now;
                    
                    // Calculate input disturbance for the dominant MV
                    // In MIMO, we assume the first MV is dominant (or could select max gain)
                    // TODO: For multi-MV integrators, distribute across all MVs proportionally
                    let mut dominant_mv = 0;
                    let mut max_gain = 0.0;
                    for mv_i in 0..self.num_mv {
                        let gain = self.cv_steady_state_gains[cv_i][mv_i].abs();
                        if gain > max_gain {
                            max_gain = gain;
                            dominant_mv = mv_i;
                        }
                    }
                    
                    if max_gain > 1e-6 {
                        let disturbance = model_error / self.cv_steady_state_gains[cv_i][dominant_mv];
                        
                        println!("INPUT DISTURBANCE: CV[{}] error={:.4}, K_ss={:.4}, D={:.4}",
                            cv_i, model_error, self.cv_steady_state_gains[cv_i][dominant_mv], disturbance);
                        
                        // Add step response of disturbance to free response
                        // This is equivalent to a sustained MV move of size 'disturbance'
                        for t in 0..self.p_horizon {
                            let idx = (t * self.num_cv) + cv_i;
                            // Use the step coefficient at time t for this MV->CV
                            if t < self.matrix_a.nrows() {
                                let row = t * self.num_cv + cv_i;
                                let col = dominant_mv; // First move of dominant MV
                                let step_coeff = self.matrix_a[(row, col)];
                                free_response[idx] += disturbance * step_coeff;
                            }
                        }
                    }
                }
            }
        }

        // --- 2. Calculate Error with Dynamic Weights ---
        // Cost = (Target - (FreeResponse + A*dU))^2
        // Rewrite: Cost = (Error - A*dU)^2 where Error = Target - FreeResponse
        let mut error_vec = DVector::zeros(self.num_cv * self.p_horizon);
        
        // Build dynamic Q diagonal from OPC UA weights
        let mut q_dynamic = DVector::zeros(self.num_cv * self.p_horizon);
        for t in 0..self.p_horizon {
            for cv_i in 0..self.num_cv {
                let idx = (t * self.num_cv) + cv_i;
                q_dynamic[idx] = cv_weights[cv_i];
            }
        }
        
        for i in 0..self.p_horizon {
            for cv_i in 0..self.num_cv {
                // Calculate the "First Order Path" to the target using dynamic alpha
                let alpha = cv_alphas[cv_i];
                // CRITICAL: Reference trajectory starts from ACTUAL measurement, not free response
                // This ensures error reflects real CV position, not model prediction
                let actual_current = current_pvs[cv_i];
                let current_deviation = active_targets[cv_i] - actual_current;
                
                // Ref[i] approaches Target as i increases
                // powi(i + 1) because at i=0 (first prediction), we want some movement
                let decay_factor = alpha.powi((i + 1) as i32);
                let reference_trajectory = active_targets[cv_i] - (current_deviation * decay_factor);

                let idx = (i * self.num_cv) + cv_i;
                // Error = Desired_Path - Predicted_Free_Path
                // Apply ECE normalization to prevent large-valued CVs from dominating
                let raw_error = reference_trajectory - free_response[idx];
                error_vec[idx] = raw_error / self.cv_ece_factors[cv_i];
            }
        }

        // --- 3. Build QP Vectors with Dynamic Weights ---
        let weighted_error = error_vec.component_mul(&q_dynamic);
        let a_t = self.matrix_a.transpose();
        
        // 🔍 DEBUG: Show A^T structure and multiplication
        println!("🔍 A^T MATRIX DEBUG:");
        println!("   A^T shape: {} rows × {} cols", a_t.nrows(), a_t.ncols());
        println!("   Weighted_error length: {}", weighted_error.len());
        print!("   A^T row 0 (first 10 elements): [");
        for i in 0..10.min(a_t.ncols()) {
            print!("{:.6}", a_t[(0, i)]);
            if i < 9.min(a_t.ncols() - 1) { print!(", "); }
        }
        println!("]");
        print!("   A^T row 1 (first 10 elements): [");
        for i in 0..10.min(a_t.ncols()) {
            print!("{:.6}", a_t[(1, i)]);
            if i < 9.min(a_t.ncols() - 1) { print!(", "); }
        }
        println!("]");
        
        // Gradient from CV target tracking: -2·A^T·Q·error
        // Rail-Riding: Maximize/Minimize modes achieved by setting target = limit
        // No need for separate linear cost term
        let q_delta_u = -2.0 * (&a_t * &weighted_error);
        
        // Augment q vector with slack variable terms (Feature 1: Soft Constraints)
        let mut q_vec = DVector::zeros(n_vars);
        
        // Copy ΔU gradient (tracking only - rail-riding handled via active_targets)
        for i in 0..n_delta_u {
            q_vec[i] = q_delta_u[i];
        }
        
        // Slack variables: no linear cost (only quadratic penalty in P matrix)
        for i in n_delta_u..n_vars {
            q_vec[i] = 0.0;
        }
        
        
        // MV economic terms and dynamic MV move suppression
        // We build a per-scan Hessian so OPC-updated MV weights are fully active.
        let mut p_dynamic_dense = self.p_matrix_dense_base.clone();

        // Replace baseline move suppression (R) with live OPC weights.
        for mv_i in 0..self.num_mv {
            let base_weight = self.mv_base_move_weights[mv_i];
            let live_weight = mv_weights.get(mv_i).copied().unwrap_or(base_weight);
            let delta_diag = 2.0 * (live_weight - base_weight);

            if delta_diag.abs() > 1e-12 {
                for step_k in 0..self.m_horizon {
                    let idx = (step_k * self.num_mv) + mv_i;
                    p_dynamic_dense[(idx, idx)] += delta_diag;
                }
            }
        }

        // Add consistent quadratic + linear economic objective in absolute MV space:
        // J_econ = Σ_t w * (u_target - (u_current + Σ_{j<=t}Δu_j))²
        for mv_i in 0..self.num_mv {
            let mode = &self.mv_optimization_modes[mv_i];
            let configured_weight = self.mv_target_weights[mv_i];

            let (mv_target, econ_weight) = match mode {
                crate::config::MvOptimizationMode::Maximize => {
                    let mode_weight = if configured_weight > 0.0 { configured_weight } else { 1.0 };
                    (dynamic_max[mv_i], mode_weight)
                }
                crate::config::MvOptimizationMode::Minimize => {
                    let mode_weight = if configured_weight > 0.0 { configured_weight } else { 1.0 };
                    (dynamic_min[mv_i], mode_weight)
                }
                crate::config::MvOptimizationMode::Target { .. } => {
                    let runtime_target = mv_targets.get(mv_i).copied().unwrap_or(current_mvs[mv_i]);
                    (runtime_target, configured_weight)
                }
            };

            if econ_weight <= 0.0 {
                continue;
            }

            let target_error = mv_target - current_mvs[mv_i];

            // Linear term: -2*w*e*L^T*1
            for step_j in 0..self.m_horizon {
                let idx_j = (step_j * self.num_mv) + mv_i;
                let influence_count = (self.m_horizon - step_j) as f64;
                q_vec[idx_j] += -2.0 * econ_weight * target_error * influence_count;
            }

            // Quadratic term: 2*w*(L^T*L)
            for step_j in 0..self.m_horizon {
                let idx_j = (step_j * self.num_mv) + mv_i;
                for step_k in 0..self.m_horizon {
                    let idx_k = (step_k * self.num_mv) + mv_i;
                    let shared_count = (self.m_horizon - usize::max(step_j, step_k)) as f64;
                    p_dynamic_dense[(idx_j, idx_k)] += 2.0 * econ_weight * shared_count;
                }
            }
        }
        
        let q_slice: Vec<f64> = q_vec.as_slice().to_vec();
        let p_dynamic_csc = to_csc(&p_dynamic_dense);

        // --- 4. Build Constraint Bounds ---
        let mv_constraints = 4 * n_delta_u;
        let soft_cv_constraints = 2 * n_slack;
        let slack_nn_constraints = n_slack;
        let total_constraint_rows = mv_constraints + soft_cv_constraints + slack_nn_constraints;
        let mut b_constraints = Vec::with_capacity(total_constraint_rows);

        // Block 1 & 2: MV Rate Limits
        for _ in 0..2 { 
            for _step in 0..self.m_horizon {
                for mv_i in 0..self.num_mv {
                    b_constraints.push(self.mv_rate_limits[mv_i]);
                }
            }
        }

        // Block 3 & 4: MV Absolute Limits (Dynamic)
        for _step in 0..self.m_horizon {
            for mv_i in 0..self.num_mv {
                let dist_to_high = dynamic_max[mv_i] - current_mvs[mv_i];
                b_constraints.push(dist_to_high);
            }
        }
        for _step in 0..self.m_horizon {
            for mv_i in 0..self.num_mv {
                let dist_to_low = current_mvs[mv_i] - dynamic_min[mv_i];
                b_constraints.push(dist_to_low);
            }
        }
        
        // Block 5: Soft CV Upper Bounds (CV_pred ≤ high + ε  =>  A·ΔU - ε ≤ high)
        for t in 0..self.p_horizon {
            for cv_i in 0..self.num_cv {
                let free_resp_idx = (t * self.num_cv) + cv_i;
                let high_limit = cv_max[cv_i];
                
                // Constraint: A·ΔU - ε ≤ high  =>  A·ΔU - ε ≤ high - free_response
                b_constraints.push(high_limit - free_response[free_resp_idx]);
            }
        }
        
        // Block 6: Soft CV Lower Bounds (CV_pred ≥ low - ε  =>  -A·ΔU - ε ≤ -low)
        for t in 0..self.p_horizon {
            for cv_i in 0..self.num_cv {
                let free_resp_idx = (t * self.num_cv) + cv_i;
                let low_limit = cv_min[cv_i];
                
                // Constraint: -A·ΔU - ε ≤ -low  =>  -A·ΔU - ε ≤ free_response - low
                b_constraints.push(free_response[free_resp_idx] - low_limit);
            }
        }
        
        // Block 7: Slack Non-negativity (ε ≥ 0  =>  -ε ≤ 0)
        for _ in 0..n_slack {
            b_constraints.push(0.0);
        }

        // --- 5. Solve (dynamic Hessian each scan for live MV weights + economic terms) ---
        let cones = [SupportedConeT::NonnegativeConeT(b_constraints.len())];
        let mut settings = DefaultSettings::default();
        settings.presolve_enable = false;

        let solver_result = DefaultSolver::new(
            &p_dynamic_csc,
            &q_slice,
            &self.a_constraints_csc,
            &b_constraints,
            &cones,
            settings,
        );

        let mut solver = match solver_result {
            Ok(solver) => solver,
            Err(e) => {
                println!("   ❌ Solver initialization failed: {:?}", e);
                return DmcResult {
                    next_move: vec![0.0; self.num_mv],
                    future_plan: vec![0.0; n_delta_u],
                    predicted_pvs: vec![0.0; total_preds],
                    status: SolverStatus::Unsolved,
                    cv_bias: vec![0.0; self.num_cv],
                    objective_value: 0.0,
                };
            }
        };

        solver.solve();

        // --- 6. Extract & Update State ---
        match solver.solution.status {
            SolverStatus::Solved | SolverStatus::AlmostSolved => {
                let solution_full = solver.solution.x.clone();
                
                // Extract ΔU (MV moves) from solution
                let delta_u: Vec<f64> = solution_full[0..n_delta_u].to_vec();
                let slack_vars: Vec<f64> = solution_full[n_delta_u..n_vars].to_vec();
                
                // Check for constraint violations (non-zero slack)
                let max_slack = slack_vars.iter().cloned().fold(0.0, f64::max);
                if max_slack > 0.01 {
                    println!("⚠️  Soft Constraint Violation: max slack = {:.4}", max_slack);
                }
                
                // Get next moves (first step of each MV)
                let mut next_move = Vec::new();
                for i in 0..self.num_mv {
                    next_move.push(delta_u[i]);
                }

                // Calculate Full Prediction (Free Response + Forced Response)
                // Only use ΔU part for prediction (slack vars don't affect dynamics)
                let plan_vec = DVector::from_vec(delta_u.clone());
                let forced_response = &self.matrix_a * plan_vec;
                
                // Total Prediction = Free (includes Bias) + Forced
                let total_prediction = &free_response + &forced_response;
                
                if commit {
                    self.prev_prediction = Some(total_prediction.as_slice().to_vec());
                } else {
                    // In Mode 1 (Monitor), we discard the state. 
                    // Next run will be a fresh "Cold Start" from the real PVs.
                    self.prev_prediction = None; 
                }

                DmcResult { 
                    next_move, 
                    future_plan: delta_u, // Only return ΔU moves, not slack variables
                    predicted_pvs: total_prediction.as_slice().to_vec(),
                    status: solver.solution.status.clone(),
                    cv_bias,
                    objective_value: solver.solution.obj_val,
                }
            },
            _ => {
                let status = solver.solution.status.clone();
                println!("🔥 QP Solver Failed: {:?}. MVs will be HELD at last values.", status);
                // On failure, we wipe the prediction state to force a "Cold Start" next time
                self.prev_prediction = None;
                
                DmcResult {
                    next_move: vec![0.0; self.num_mv],  // Won't be written - just for safety
                    future_plan: vec![0.0; n_delta_u], // Only ΔU, not slack
                    predicted_pvs: vec![0.0; total_preds],
                    status,
                    cv_bias: vec![0.0; self.num_cv],
                    objective_value: 0.0,
                }
            }
        }
    }
}

// --- Helpers ---
fn to_csc(dense: &DMatrix<f64>) -> CscMatrix<f64> {
    let (nrows, ncols) = dense.shape();
    let mut col_ptr = vec![0; ncols + 1];
    let mut row_val = Vec::new();
    let mut values = Vec::new();
    let mut count = 0;
    for c in 0..ncols {
        for r in 0..nrows {
            let val = dense[(r, c)];
            if val.abs() > 1e-12 { 
                values.push(val);
                row_val.push(r);
                count += 1;
            }
        }
        col_ptr[c + 1] = count;
    }
    CscMatrix::new(nrows, ncols, col_ptr, row_val, values)
}