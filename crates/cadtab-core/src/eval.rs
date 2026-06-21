//! Evaluation values and the lexical environment. Later steps turn the AST
//! into these values and, ultimately, a musical-model phrase; this step defines
//! the value taxonomy and the scope chain they live in.

use std::collections::HashMap;

use crate::model::{Duration, Note, Phrase, Position};

/// A runtime value. The taxonomy mirrors the static one: integers, durations,
/// positions, single notes, and phrases. A chord literal evaluates to a phrase.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(u32),
    Duration(Duration),
    Position(Position),
    Note(Note),
    Phrase(Phrase),
}

impl Value {
    /// A noun phrase naming the value's kind, for diagnostics.
    pub fn kind_label(&self) -> &'static str {
        match self {
            Value::Int(_) => "an integer",
            Value::Duration(_) => "a duration",
            Value::Position(_) => "a position",
            Value::Note(_) => "a note",
            Value::Phrase(_) => "a phrase",
        }
    }
}

/// A lexical scope chain of value bindings. Lookups walk innermost → outermost,
/// so a binding in a nested scope shadows one outside it. The outermost (global)
/// scope is always present and never popped.
#[derive(Debug, Clone)]
pub struct Env {
    scopes: Vec<HashMap<String, Value>>,
}

impl Default for Env {
    fn default() -> Self {
        Self::new()
    }
}

impl Env {
    pub fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],
        }
    }

    /// Enter a fresh nested scope.
    pub fn push(&mut self) {
        self.scopes.push(HashMap::new());
    }

    /// Leave the innermost scope. The global scope is kept.
    pub fn pop(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    /// Bind `name` in the innermost scope, shadowing any outer binding.
    pub fn define(&mut self, name: impl Into<String>, value: Value) {
        self.scopes
            .last_mut()
            .expect("env always has a global scope")
            .insert(name.into(), value);
    }

    /// Look up `name`, innermost scope first.
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.scopes.iter().rev().find_map(|scope| scope.get(name))
    }

    /// Number of active scopes (always ≥ 1).
    pub fn depth(&self) -> usize {
        self.scopes.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Position;

    fn pos(string: u8, fret: u8) -> Value {
        Value::Position(Position::new(string, fret))
    }

    #[test]
    fn kind_labels_name_each_variant() {
        assert_eq!(Value::Int(3).kind_label(), "an integer");
        assert_eq!(pos(3, 0).kind_label(), "a position");
        assert_eq!(
            Value::Phrase(crate::model::Phrase::new()).kind_label(),
            "a phrase"
        );
    }

    #[test]
    fn define_and_get_round_trip() {
        let mut env = Env::new();
        env.define("x", Value::Int(5));
        assert_eq!(env.get("x"), Some(&Value::Int(5)));
        assert_eq!(env.get("missing"), None);
    }

    #[test]
    fn inner_scope_shadows_outer() {
        let mut env = Env::new();
        env.define("g", pos(3, 0));
        env.push();
        env.define("g", pos(1, 7)); // shadows
        assert_eq!(env.get("g"), Some(&pos(1, 7)));
        env.pop();
        // The outer binding is visible again.
        assert_eq!(env.get("g"), Some(&pos(3, 0)));
    }

    #[test]
    fn nested_scope_sees_outer_bindings() {
        let mut env = Env::new();
        env.define("outer", Value::Int(1));
        env.push();
        env.define("inner", Value::Int(2));
        assert_eq!(env.get("outer"), Some(&Value::Int(1)));
        assert_eq!(env.get("inner"), Some(&Value::Int(2)));
        env.pop();
        // The nested binding is gone once its scope ends.
        assert_eq!(env.get("inner"), None);
    }

    #[test]
    fn depth_tracks_scope_nesting_and_global_is_protected() {
        let mut env = Env::new();
        assert_eq!(env.depth(), 1);
        env.push();
        assert_eq!(env.depth(), 2);
        env.pop();
        assert_eq!(env.depth(), 1);
        // Popping past the global scope is a no-op.
        env.pop();
        assert_eq!(env.depth(), 1);
    }

    #[test]
    fn redefining_in_the_same_scope_overwrites() {
        let mut env = Env::new();
        env.define("x", Value::Int(1));
        env.define("x", Value::Int(2));
        assert_eq!(env.get("x"), Some(&Value::Int(2)));
    }
}
