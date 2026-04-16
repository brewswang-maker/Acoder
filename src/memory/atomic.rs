//! Atomic Memory — 原子化记忆单元

use std::collections::HashMap;
use parking_lot::RwLock;
use std::sync::Arc;

/// 原子事实：不可分割的最小记忆单元
#[derive(Debug, Clone)]
pub struct AtomicFact {
    pub key: String,
    pub value: String,
    pub provenance: Provenance,
    pub confidence: f64,
}

#[derive(Debug, Clone)]
pub struct Provenance {
    pub source: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub method: String,
}

pub struct AtomicMemory {
    facts: Arc<RwLock<HashMap<String, AtomicFact>>>,
}

impl AtomicMemory {
    pub fn new() -> Self {
        Self { facts: Arc::new(RwLock::new(HashMap::new())) }
    }

    pub fn store(&self, fact: AtomicFact) {
        self.facts.write().insert(fact.key.clone(), fact);
    }

    pub fn get(&self, key: &str) -> Option<AtomicFact> {
        self.facts.read().get(key).cloned()
    }

    pub fn search(&self, pattern: &str) -> Vec<AtomicFact> {
        self.facts.read().values()
            .filter(|f| f.key.contains(pattern) || f.value.contains(pattern))
            .cloned()
            .collect()
    }
}

impl Default for AtomicMemory {
    fn default() -> Self { Self::new() }
}
