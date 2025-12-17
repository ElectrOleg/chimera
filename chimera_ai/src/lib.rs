use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct PathStats {
    pub latency: Duration,
    pub packet_loss: f32, // 0.0 to 1.0
    pub bandwidth: u64,   // bits per second
    pub last_updated: Instant,
}

impl PathStats {
    pub fn new() -> Self {
        Self {
            latency: Duration::from_millis(100), // Default assumption
            packet_loss: 0.0,
            bandwidth: 1_000_000,
            last_updated: Instant::now(),
        }
    }
    
    // Simple heuristic score: lower is better
    pub fn score(&self) -> u64 {
        let latency_ms = self.latency.as_millis() as u64;
        let loss_penalty = (self.packet_loss * 1000.0) as u64;
        latency_ms + loss_penalty
    }
}

pub struct Router {
    // Map of Transport Name -> Stats
    paths: Arc<Mutex<HashMap<String, PathStats>>>,
}

impl Router {
    pub fn new() -> Self {
        Self {
            paths: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn register_path(&self, name: &str) {
        let mut paths = self.paths.lock().unwrap();
        paths.insert(name.to_string(), PathStats::new());
    }

    pub fn update_latency(&self, name: &str, latency: Duration) {
        let mut paths = self.paths.lock().unwrap();
        if let Some(stats) = paths.get_mut(name) {
            // Exponential moving average for smoothing
            stats.latency = stats.latency.mul_f32(0.8) + latency.mul_f32(0.2);
            stats.packet_loss = stats.packet_loss * 0.9; // Decay loss over time if successful
            stats.last_updated = Instant::now();
        }
    }

    pub fn report_failure(&self, name: &str) {
        let mut paths = self.paths.lock().unwrap();
        if let Some(stats) = paths.get_mut(name) {
             // Drastic penalty for failure
             stats.packet_loss = 1.0; // 100% loss
             stats.last_updated = Instant::now();
        }
    }

    pub fn get_best_path(&self) -> Option<String> {
        let paths = self.paths.lock().unwrap();
        paths.iter()
            .min_by_key(|(_, stats)| stats.score())
            .map(|(name, _)| name.clone())
    }
    
    pub fn get_stats(&self, name: &str) -> Option<PathStats> {
        let paths = self.paths.lock().unwrap();
        paths.get(name).cloned()
    }
}
