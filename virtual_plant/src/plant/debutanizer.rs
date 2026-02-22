// src/plant/debutanizer.rs

// Chemical Properties for LPG (C4/C5 mix)
pub const MW: f64 = 60.0;       // kg/kmol
pub const DENSITY: f64 = 580.0; // kg/m3

#[derive(Clone, Debug)]
pub struct ColumnConfig {
    pub num_stages: usize,
    pub feed_stage: usize,
    pub relative_volatility: f64, 
    pub hold_up_molar: f64,       
    pub dt_seconds: f64,         
}

#[derive(Clone, Debug)]
pub struct DebutanizerModel {
    config: ColumnConfig,
    pub x: Vec<f64>, // State vector (compositions)
}

// Inputs are pure Molar Flows (Physics doesn't care about m3/h)
pub struct ModelInputs {
    pub reflux_l: f64, 
    pub boilup_v: f64, 
    pub feed_f: f64,     
    pub feed_z: f64,   
}

pub struct ModelOutputs {
    pub top_c4_frac: f64,    
    pub bottom_temp_deg_c: f64, 
}

impl DebutanizerModel {
    pub fn new(config: ColumnConfig) -> Self {
        let n = config.num_stages;
        let mut x = Vec::with_capacity(n);
        // Initialize with a simple gradient (0.05 bottom -> 0.95 top)
        for i in 0..n {
            x.push(0.05 + (0.90 * (i as f64) / ((n - 1) as f64)));
        }
        Self { config, x }
    }

    fn vle(&self, x: f64) -> f64 {
        let alpha = self.config.relative_volatility;
        (alpha * x) / (1.0 + (alpha - 1.0) * x)
    }

    fn derivatives(&self, current_x: &[f64], u: &ModelInputs) -> Vec<f64> {
        let n = self.config.num_stages;
        let mut dxdt = vec![0.0; n];
        
        let l_rect = u.reflux_l;
        let v_rect = u.boilup_v;
        let v_strip = u.boilup_v;
        let l_strip = u.reflux_l + u.feed_f; // q=1 assumption

        let distillate_d = v_rect - l_rect; 
        let bottoms_b = l_strip - v_strip;

        // Safety clamp
        if distillate_d.abs() < 0.001 || bottoms_b.abs() < 0.001 {
            return dxdt; 
        }

        for i in 0..n {
            let m = self.config.hold_up_molar;
            let xi = current_x[i];
            let yi = self.vle(xi);
            
            // Neighbors
            let li_plus_1 = if i == n - 1 { 0.0 } else { current_x[i+1] }; 
            let yi_minus_1 = if i == 0 { 0.0 } else { self.vle(current_x[i-1]) }; 

            let (l_in, v_in) = if i < self.config.feed_stage {
                (l_strip, v_strip)
            } else {
                (l_rect, v_rect)
            };

            let mut change = 0.0;

            // 1. Liquid from above
            if i < n - 1 {
                let l_flow = if i + 1 == self.config.feed_stage { l_rect } else { l_in }; 
                change += l_flow * li_plus_1;
            }
            // 2. Vapor from below
            if i > 0 {
                 let v_flow = if i - 1 == self.config.feed_stage { v_strip } else { v_in };
                 change += v_flow * yi_minus_1;
            }
            // 3. Outflows
            if i == 0 { change -= bottoms_b * xi; change -= v_in * yi; } 
            else if i == n - 1 { change -= distillate_d * xi; change -= l_in * xi; } 
            else { change -= l_in * xi; change -= v_in * yi; }

            // 4. Feed
            if i == self.config.feed_stage { change += u.feed_f * u.feed_z; }

            dxdt[i] = change / m;
        }
        dxdt
    }

    pub fn step(&mut self, u: &ModelInputs) -> ModelOutputs {
        let dt = self.config.dt_seconds;
        let n = self.config.num_stages;

        // RK4 Integration
        let k1 = self.derivatives(&self.x, u);
        let x_k2: Vec<f64> = self.x.iter().zip(&k1).map(|(x, k)| x + 0.5 * dt * k).collect();
        let k2 = self.derivatives(&x_k2, u);
        let x_k3: Vec<f64> = self.x.iter().zip(&k2).map(|(x, k)| x + 0.5 * dt * k).collect();
        let k3 = self.derivatives(&x_k3, u);
        let x_k4: Vec<f64> = self.x.iter().zip(&k3).map(|(x, k)| x + dt * k).collect();
        let k4 = self.derivatives(&x_k4, u);

        for i in 0..n {
            self.x[i] += (dt / 6.0) * (k1[i] + 2.0*k2[i] + 2.0*k3[i] + k4[i]);
            self.x[i] = self.x[i].max(0.0).min(1.0);
        }

        // Output Correlations
        let top_c4 = self.x[n-1];
        let bottom_c4 = self.x[0];
        let temp_est = 50.0 + (1.0 - bottom_c4) * 30.0; 

        ModelOutputs {
            top_c4_frac: top_c4,
            bottom_temp_deg_c: temp_est,
        }
    }
}