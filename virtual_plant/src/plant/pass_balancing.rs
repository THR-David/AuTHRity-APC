use std::collections::VecDeque;

#[derive(Clone, Debug)]
pub struct PassBalancingConfig {
    pub tau_seconds: f64,
    pub deadtime_seconds: f64,
    pub same_pass_gain: f64,
    pub cross_pass_gain: f64,
    pub valve_min: f64,
    pub valve_max: f64,
}

impl Default for PassBalancingConfig {
    fn default() -> Self {
        Self {
            tau_seconds: 80.0,
            deadtime_seconds: 20.0,
            same_pass_gain: -1.2,
            cross_pass_gain: 0.2,
            valve_min: 0.0,
            valve_max: 100.0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PassBalancingModel {
    config: PassBalancingConfig,
    dt_seconds: f64,
    dead_steps: usize,
    delayed_inputs: [VecDeque<f64>; 6],
    states: [[f64; 6]; 6],
    base_temperatures: [f64; 6],
    coke_dynamic_offsets: [f64; 6],
}

#[derive(Clone, Debug)]
pub struct PassBalancingInputs {
    pub flow_master_op: f64,
    pub bias_sp: [f64; 6],
    pub coke_enable: bool,
    pub coke_offset: [f64; 6],
    pub coke_ramp_per_min: [f64; 6],
}

#[derive(Clone, Debug)]
pub struct PassBalancingOutputs {
    pub temperatures: [f64; 6],
    pub temp_deviation: [f64; 6],
    pub valve_op: [f64; 6],
    pub pass_flow: [f64; 6],
    pub total_flow: f64,
    pub sum_bias: f64,
}

impl PassBalancingModel {
    pub fn new(dt_seconds: f64, config: PassBalancingConfig) -> Self {
        let dead_steps = ((config.deadtime_seconds / dt_seconds).round() as usize).max(1);
        let delayed_inputs = std::array::from_fn(|_| {
            let mut queue = VecDeque::new();
            for _ in 0..=dead_steps {
                queue.push_back(0.0);
            }
            queue
        });

        Self {
            config,
            dt_seconds,
            dead_steps,
            delayed_inputs,
            states: [[0.0; 6]; 6],
            base_temperatures: [103.0, 102.0, 101.0, 99.0, 98.0, 97.0],
            coke_dynamic_offsets: [0.0; 6],
        }
    }

    pub fn step(&mut self, input: &PassBalancingInputs) -> PassBalancingOutputs {
        let mut valve_op = [0.0; 6];
        let mut pass_flow = [0.0; 6];

        for i in 0..6 {
            valve_op[i] = (input.flow_master_op + input.bias_sp[i])
                .clamp(self.config.valve_min, self.config.valve_max);
            pass_flow[i] = valve_op[i];
        }

        let mut delayed_u = [0.0; 6];
        for i in 0..6 {
            let u_i = valve_op[i] - 50.0;
            self.delayed_inputs[i].push_back(u_i);
            delayed_u[i] = self.delayed_inputs[i].pop_front().unwrap_or(0.0);
        }

        let a = (self.dt_seconds / self.config.tau_seconds).clamp(0.0, 1.0);
        for cv_i in 0..6 {
            for mv_j in 0..6 {
                let gain = if cv_i == mv_j {
                    self.config.same_pass_gain
                } else {
                    self.config.cross_pass_gain
                };
                let target = gain * delayed_u[mv_j];
                self.states[cv_i][mv_j] += a * (target - self.states[cv_i][mv_j]);
            }
        }

        if input.coke_enable {
            for i in 0..6 {
                self.coke_dynamic_offsets[i] += input.coke_ramp_per_min[i] * (self.dt_seconds / 60.0);
            }
        } else {
            self.coke_dynamic_offsets = [0.0; 6];
        }

        let mut temperatures = [0.0; 6];
        for i in 0..6 {
            let dynamic_sum: f64 = self.states[i].iter().sum();
            let coke = if input.coke_enable {
                input.coke_offset[i] + self.coke_dynamic_offsets[i]
            } else {
                0.0
            };
            temperatures[i] = self.base_temperatures[i] + dynamic_sum + coke;
        }

        let avg_temp = temperatures.iter().sum::<f64>() / 6.0;
        let mut temp_deviation = [0.0; 6];
        for i in 0..6 {
            temp_deviation[i] = temperatures[i] - avg_temp;
        }

        let total_flow = pass_flow.iter().sum::<f64>();
        let sum_bias = input.bias_sp.iter().sum::<f64>();

        let _ = self.dead_steps;

        PassBalancingOutputs {
            temperatures,
            temp_deviation,
            valve_op,
            pass_flow,
            total_flow,
            sum_bias,
        }
    }
}
