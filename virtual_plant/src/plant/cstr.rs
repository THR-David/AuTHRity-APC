// src/plant/cstr.rs

#[derive(Clone, Debug)]
pub struct CSTRConfig {
    pub vol: f64,       // m3
    pub density: f64,   // kg/m3
    pub cp: f64,        // J/kg.K
    pub heat_rxn: f64,  // J/mol (Negative for exothermic)
    pub e_over_r: f64,  // K
    pub k0: f64,        // 1/min
    pub ua: f64,        // J/min.K
    pub dt_min: f64,    // Time step in minutes
}

#[derive(Clone, Debug)]
pub struct CSTRModel {
    pub config: CSTRConfig,
    pub ca: f64, 
    pub t: f64,  
}

pub struct CSTRInputs {
    pub flow_f: f64,     // INPUT: m3/h (Intuitive Unit)
    pub cool_temp: f64,  // Kelvin
    pub ca_feed: f64,    // kmol/m3
    pub t_feed: f64,     // Kelvin
}

pub struct CSTROutputs {
    pub ca: f64,
    pub t: f64,
}

impl CSTRModel {
    pub fn new() -> Self {
        Self {
            ca: 8.56,    
            t: 311.26,   
            config: CSTRConfig {
                vol: 1.0,           
                density: 1000.0,    
                cp: 4000.0,          
                heat_rxn: -50000.0,  
                e_over_r: 8750.0,    
                k0: 7.2e10,          
                ua: 50000.0,         
                dt_min: 0.016, // ~1 second
            }
        }
    }

    pub fn step(&mut self, u: &CSTRInputs) -> CSTROutputs {
        let dt = self.config.dt_min;
        
        // --- UNIT CONVERSION ---
        // Input is m3/h. Physics needs m3/min.
        let flow_rate_min = u.flow_f / 60.0; 

        // 1. Reaction Rate
        let k = self.config.k0 * (-self.config.e_over_r / self.t).exp();
        let reaction_rate = k * self.ca;

        // 2. Mass Balance (Using converted flow)
        let dca_dt = (flow_rate_min / self.config.vol) * (u.ca_feed - self.ca) - reaction_rate;

        // 3. Energy Balance (Using converted flow)
        let term_flow = (flow_rate_min / self.config.vol) * (u.t_feed - self.t);
        let term_rxn  = (-self.config.heat_rxn / (self.config.density * self.config.cp)) * reaction_rate; 
        let term_cool = (self.config.ua / (self.config.vol * self.config.density * self.config.cp)) * (self.t - u.cool_temp);
        
        let dt_dt = term_flow + term_rxn - term_cool;

        // 4. Update State
        self.ca += dca_dt * dt;
        self.t += dt_dt * dt;

        CSTROutputs {
            ca: self.ca,
            t: self.t,
        }
    }
}