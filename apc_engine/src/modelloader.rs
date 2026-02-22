// src/modelloader.rs
use crate::config::UnifiedModel;
use anyhow::{Result, Context, anyhow};
use std::fs::File;
use std::io::{BufReader, BufRead};

/// Load a parametric model (FOPDT: gain, tau, dead_time)
/// Generates step response curves from FOPDT parameters
pub fn load_parametric_model(model: &UnifiedModel) -> Result<Vec<Vec<Vec<f64>>>> {
    if model.metadata.model_type != "parametric" {
        return Err(anyhow!("Model type must be 'parametric', got: {}", model.metadata.model_type));
    }

    let num_cv = model.variables.cvs.len();
    let num_mv = model.variables.mvs.len();
    let p_horizon = model.tuning.prediction_horizon;
    let sample_time = model.tuning.sample_time;

    // Validate FOPDT matrices
    if model.physics.gain.len() != num_cv || 
       model.physics.tau.len() != num_cv || 
       model.physics.dead_time.len() != num_cv {
        return Err(anyhow!("Physics matrix row count mismatch"));
    }

    let mut step_coefficients = Vec::with_capacity(num_cv);

    for cv_idx in 0..num_cv {
        let mut cv_row = Vec::with_capacity(num_mv);
        
        for mv_idx in 0..num_mv {
            let k = model.physics.gain[cv_idx][mv_idx];
            let tau = model.physics.tau[cv_idx][mv_idx];
            let theta = model.physics.dead_time[cv_idx][mv_idx];
            
            let curve = generate_fopdt_curve(k, tau, theta, sample_time, p_horizon);
            cv_row.push(curve);
        }
        
        step_coefficients.push(cv_row);
    }

    Ok(step_coefficients)
}

/// Load a step response model (direct coefficients)
/// Just validates and returns the coefficients as-is
/// Returns (mv_coefficients, dv_coefficients)
pub fn load_stepresponse_model(model: &UnifiedModel) -> Result<(Vec<Vec<Vec<f64>>>, Vec<Vec<Vec<f64>>>)> {
    if model.metadata.model_type != "step_response" {
        return Err(anyhow!("Model type must be 'step_response', got: {}", model.metadata.model_type));
    }

    let num_cv = model.variables.cvs.len();
    let num_mv = model.variables.mvs.len();
    let p_horizon = model.tuning.prediction_horizon;

    // Validate dimensions
    if model.physics.step_coefficients.len() != num_cv {
        return Err(anyhow!(
            "Step coefficients CV count mismatch: expected {}, got {}", 
            num_cv, 
            model.physics.step_coefficients.len()
        ));
    }

    for (cv_idx, cv_row) in model.physics.step_coefficients.iter().enumerate() {
        if cv_row.len() != num_mv {
            return Err(anyhow!(
                "Step coefficients MV count mismatch at CV {}: expected {}, got {}", 
                cv_idx, num_mv, cv_row.len()
            ));
        }

        for (mv_idx, coeffs) in cv_row.iter().enumerate() {
            if coeffs.len() != p_horizon {
                return Err(anyhow!(
                    "Step coefficient length mismatch at CV {} MV {}: expected {}, got {}", 
                    cv_idx, mv_idx, p_horizon, coeffs.len()
                ));
            }
        }
    }

    // Validate DV coefficients if present
    let num_dv = model.variables.dvs.len();
    if !model.physics.dv_coefficients.is_empty() {
        if model.physics.dv_coefficients.len() != num_cv {
            return Err(anyhow!(
                "DV coefficients CV count mismatch: expected {}, got {}", 
                num_cv, 
                model.physics.dv_coefficients.len()
            ));
        }

        for (cv_idx, cv_row) in model.physics.dv_coefficients.iter().enumerate() {
            if cv_row.len() != num_dv {
                return Err(anyhow!(
                    "DV coefficients DV count mismatch at CV {}: expected {}, got {}", 
                    cv_idx, num_dv, cv_row.len()
                ));
            }

            for (dv_idx, coeffs) in cv_row.iter().enumerate() {
                if coeffs.len() != p_horizon {
                    return Err(anyhow!(
                        "DV coefficient length mismatch at CV {} DV {}: expected {}, got {}", 
                        cv_idx, dv_idx, p_horizon, coeffs.len()
                    ));
                }
            }
        }

        println!("✅ Validated DV coefficients: {} CVs × {} DVs × {} steps", num_cv, num_dv, p_horizon);
    }

    Ok((model.physics.step_coefficients.clone(), model.physics.dv_coefficients.clone()))
}

/// Parse legacy DMC step response text file (ModelA_orig.txt format)
/// Returns step_coefficients[cv_idx][mv_idx][time_step]
#[allow(dead_code)] // Migration utility kept for legacy model import
pub fn parse_legacy_stepresponse(
    file_path: &str,
    expected_cvs: usize,
    expected_mvs: usize,
    expected_horizon: usize,
) -> Result<Vec<Vec<Vec<f64>>>> {
    let file = File::open(file_path)
        .with_context(|| format!("Failed to open step response file: {}", file_path))?;
    let reader = BufReader::new(file);
    let all_lines: Vec<String> = reader.lines().collect::<Result<_, _>>()
        .context("Failed to read lines")?;
    
    let mut line_iter = all_lines.iter();

    // Skip header line (Last Run: ...)
    line_iter.next();

    // Read header numbers: 0, num_mvs, prediction_horizon, tss_minutes
    let header_line = line_iter.next()
        .ok_or_else(|| anyhow!("Missing header line"))?;
    
    let header_parts: Vec<&str> = header_line.split_whitespace().collect();
    if header_parts.len() < 4 {
        return Err(anyhow!("Invalid header format. Expected: 0 num_mvs horizon tss_minutes"));
    }

    let num_mvs_file: usize = header_parts[1].parse()
        .context("Failed to parse num_mvs from header")?;
    let horizon_file: usize = header_parts[2].parse()
        .context("Failed to parse prediction_horizon from header")?;

    println!("📄 Legacy file header: {} MVs, {} step horizon", num_mvs_file, horizon_file);

    // Validate dimensions
    if num_mvs_file != expected_mvs {
        return Err(anyhow!(
            "MV count mismatch: file has {}, expected {}", 
            num_mvs_file, expected_mvs
        ));
    }
    if horizon_file != expected_horizon {
        return Err(anyhow!(
            "Horizon mismatch: file has {}, expected {}", 
            horizon_file, expected_horizon
        ));
    }

    let mut step_coefficients = Vec::with_capacity(expected_cvs);
    let mut current_cv_name: Option<String> = None;
    let mut current_cv_data: Vec<Vec<f64>> = Vec::new();
    let mut line_idx = 0;
    let lines_vec: Vec<&str> = all_lines.iter().skip(2).map(|s| s.as_str()).collect();

    while line_idx < lines_vec.len() {
        let trimmed = lines_vec[line_idx].trim();
        line_idx += 1;

        // Skip empty lines
        if trimmed.is_empty() {
            continue;
        }

        // Check if this is a CV header (e.g., "TI1    C               0      0.000000")
        if !trimmed.starts_with(' ') && !trimmed.chars().next().unwrap_or(' ').is_numeric() {
            // Save previous CV if exists
            if let Some(_cv_name) = current_cv_name {
                if current_cv_data.len() == expected_mvs {
                    step_coefficients.push(current_cv_data.clone());
                    current_cv_data.clear();
                } else {
                    return Err(anyhow!(
                        "CV {} has {} MVs, expected {}", 
                        _cv_name, current_cv_data.len(), expected_mvs
                    ));
                }
            }

            // Start new CV
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            current_cv_name = Some(parts[0].to_string());
            continue;
        }

        // Check if this is an MV header (e.g., "FC1  %                                -1.400000000000000e+000")
        if !trimmed.starts_with(' ') {
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() >= 3 {
                // This is an MV header line, steady-state gain is the last number
                // We'll collect the coefficients from the following lines
                let mut coefficients = Vec::new();
                
                // Read the next lines containing the step response coefficients
                // Format: 5 rows × 9 columns = 45 values
                let rows_needed = (expected_horizon + 8) / 9; // Ceiling division
                
                for _ in 0..rows_needed {
                    if line_idx < lines_vec.len() {
                        let coeff_line = lines_vec[line_idx];
                        line_idx += 1;
                        
                        let coeff_trimmed = coeff_line.trim();
                        let values: Result<Vec<f64>, _> = coeff_trimmed
                            .split_whitespace()
                            .map(|s| s.parse::<f64>())
                            .collect();
                        
                        match values {
                            Ok(vals) => coefficients.extend(vals),
                            Err(_) => continue, // Skip malformed lines
                        }
                    }
                }

                // Truncate or pad to exact horizon
                coefficients.truncate(expected_horizon);
                if coefficients.len() < expected_horizon {
                    // Pad with last value (steady-state)
                    let last_val = *coefficients.last().unwrap_or(&0.0);
                    coefficients.resize(expected_horizon, last_val);
                }

                current_cv_data.push(coefficients);
            }
        }
    }

    // Save last CV
    if let Some(_cv_name) = current_cv_name {
        if current_cv_data.len() == expected_mvs {
            step_coefficients.push(current_cv_data);
        }
    }

    // Final validation
    if step_coefficients.len() != expected_cvs {
        return Err(anyhow!(
            "CV count mismatch: parsed {}, expected {}", 
            step_coefficients.len(), expected_cvs
        ));
    }

    println!("✅ Parsed {} CVs × {} MVs × {} steps", expected_cvs, expected_mvs, expected_horizon);
    Ok(step_coefficients)
}

/// Generate FOPDT step response curve
fn generate_fopdt_curve(k: f64, tau: f64, theta: f64, sample_time: f64, horizon: usize) -> Vec<f64> {
    let mut curve = Vec::with_capacity(horizon);
    for step in 0..horizon {
        let t = (step as f64) * sample_time;  // Changed: removed + 1.0
        if t < theta {
            curve.push(0.0);
        } else {
            let exponent = -(t - theta) / tau;
            let response = if tau.abs() < 1e-6 {
                k
            } else {
                k * (1.0 - exponent.exp())
            };
            curve.push(response);
        }
    }
    curve
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fopdt_curve_generation() {
        let curve = generate_fopdt_curve(2.0, 60.0, 10.0, 20.0, 10);
        assert_eq!(curve.len(), 10);
        assert_eq!(curve[0], 0.0); // Before dead time
        assert!(curve[9] > curve[1]); // Should be increasing
    }
}
