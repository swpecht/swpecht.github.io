use std::{collections::HashMap, ops::Index};

use crate::game::Action;

pub(super) struct NormalizerVector {
    normalized: bool,
    total: f64,
    values: Vec<f64>,
}

impl NormalizerVector {
    pub fn new() -> Self {
        Self {
            normalized: false,
            total: 0.0,
            values: Vec::with_capacity(32), // number of actions for bluff
        }
    }

    pub fn push(&mut self, v: f64) {
        assert!(v >= 0.0);
        self.total += v;
        self.values.push(v)
    }

    pub fn normalize(&mut self) {
        assert!(self.total > 0.0);
        assert!(!self.normalized);

        for i in 0..self.values.len() {
            self.values[i] = self.values[i] / self.total;
        }

        self.normalized = true;
    }
}

impl Index<usize> for NormalizerVector {
    type Output = f64;

    fn index(&self, index: usize) -> &Self::Output {
        return &self.values[index];
    }
}

pub(super) struct NormalizerMap {
    normalized: bool,
    total: f64,
    values: HashMap<Action, f64>,
}

impl NormalizerMap {
    pub fn new() -> Self {
        Self {
            normalized: false,
            total: 0.0,
            values: HashMap::new(),
        }
    }

    pub fn normalize(&mut self) {
        assert!(self.total > 0.0);
        assert!(!self.normalized);

        for (k, v) in self.values.clone() {
            self.values.insert(k, v / self.total);
        }

        self.normalized = true;
    }

    pub fn add(&mut self, k: Action, v: f64) {
        assert!(v >= 0.0);
        self.total += v;
        self.values.insert(k, v);
    }
}

impl Index<Action> for NormalizerMap {
    type Output = f64;

    fn index(&self, index: Action) -> &Self::Output {
        return &self.values[&index];
    }
}
