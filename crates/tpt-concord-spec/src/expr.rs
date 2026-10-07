// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! A small, serializable predicate language for specifications.
//!
//! Specifications must be machine-readable objects (spec §5), so predicates
//! are a structured AST — not opaque strings or closures — with a total,
//! deterministic evaluator over a variable environment.
//!
//! Values are limited to what conformance checking needs: integers, booleans,
//! strings and sequences. The evaluator never panics: missing variables and
//! type mismatches are errors.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

/// A dynamically-typed value used in specification states and predicates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Value {
    Int(i128),
    Bool(bool),
    Str(String),
    Seq(Vec<Value>),
}

impl Value {
    /// Type name used in error messages.
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Int(_) => "int",
            Value::Bool(_) => "bool",
            Value::Str(_) => "str",
            Value::Seq(_) => "seq",
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(i) => write!(f, "{i}"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Str(s) => write!(f, "{s:?}"),
            Value::Seq(items) => {
                f.write_str("[")?;
                for (i, v) in items.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{v}")?;
                }
                f.write_str("]")
            }
        }
    }
}

/// A specification state: named values.
pub type State = BTreeMap<String, Value>;

/// Errors from predicate evaluation.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EvalError {
    #[error("unknown variable `{0}`")]
    UnknownVariable(String),
    #[error("type mismatch in `{op}`: expected {expected}, got {got}")]
    TypeMismatch {
        op: &'static str,
        expected: &'static str,
        got: String,
    },
    #[error("division by zero")]
    DivisionByZero,
    #[error("predicate did not evaluate to a bool (got {0})")]
    NotABool(String),
}

/// A predicate/expression AST over `State` environments.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Expr {
    /// Read a variable from the environment.
    Var(String),
    /// A literal value.
    Const(Value),
    /// Length of a sequence.
    Len(Box<Expr>),
    Not(Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    Eq(Box<Expr>, Box<Expr>),
    Ne(Box<Expr>, Box<Expr>),
    Lt(Box<Expr>, Box<Expr>),
    Le(Box<Expr>, Box<Expr>),
    Gt(Box<Expr>, Box<Expr>),
    Ge(Box<Expr>, Box<Expr>),
    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Div(Box<Expr>, Box<Expr>),
    Mod(Box<Expr>, Box<Expr>),
    /// Minimum of two integers.
    Min(Box<Expr>, Box<Expr>),
    /// Maximum of two integers.
    Max(Box<Expr>, Box<Expr>),
}

/// Convenience constructor functions.
impl Expr {
    pub fn var(name: impl Into<String>) -> Expr {
        Expr::Var(name.into())
    }
    pub fn int(i: i128) -> Expr {
        Expr::Const(Value::Int(i))
    }
    pub fn boolean(b: bool) -> Expr {
        Expr::Const(Value::Bool(b))
    }
    pub fn str(s: impl Into<String>) -> Expr {
        Expr::Const(Value::Str(s.into()))
    }
    pub fn len(e: Expr) -> Expr {
        Expr::Len(Box::new(e))
    }
    pub fn and(a: Expr, b: Expr) -> Expr {
        Expr::And(Box::new(a), Box::new(b))
    }
    pub fn or(a: Expr, b: Expr) -> Expr {
        Expr::Or(Box::new(a), Box::new(b))
    }
    pub fn eq(a: Expr, b: Expr) -> Expr {
        Expr::Eq(Box::new(a), Box::new(b))
    }
    pub fn ne(a: Expr, b: Expr) -> Expr {
        Expr::Ne(Box::new(a), Box::new(b))
    }
    pub fn lt(a: Expr, b: Expr) -> Expr {
        Expr::Lt(Box::new(a), Box::new(b))
    }
    pub fn le(a: Expr, b: Expr) -> Expr {
        Expr::Le(Box::new(a), Box::new(b))
    }
    pub fn gt(a: Expr, b: Expr) -> Expr {
        Expr::Gt(Box::new(a), Box::new(b))
    }
    pub fn ge(a: Expr, b: Expr) -> Expr {
        Expr::Ge(Box::new(a), Box::new(b))
    }
    pub fn r#mod(a: Expr, b: Expr) -> Expr {
        Expr::Mod(Box::new(a), Box::new(b))
    }
    pub fn min(a: Expr, b: Expr) -> Expr {
        Expr::Min(Box::new(a), Box::new(b))
    }
    pub fn max(a: Expr, b: Expr) -> Expr {
        Expr::Max(Box::new(a), Box::new(b))
    }

    fn ints(&self, a: &Expr, b: &Expr, env: &State) -> Result<(i128, i128), EvalError> {
        let av = a.eval(env)?;
        let bv = b.eval(env)?;
        match (av, bv) {
            (Value::Int(x), Value::Int(y)) => Ok((x, y)),
            (av, bv) => Err(EvalError::TypeMismatch {
                op: self.op_name(),
                expected: "int, int",
                got: format!("{}, {}", av.type_name(), bv.type_name()),
            }),
        }
    }

    fn op_name(&self) -> &'static str {
        match self {
            Expr::Var(_) | Expr::Const(_) | Expr::Len(_) => "expr",
            Expr::Not(_) => "not",
            Expr::And(_, _) => "and",
            Expr::Or(_, _) => "or",
            Expr::Eq(_, _) => "eq",
            Expr::Ne(_, _) => "ne",
            Expr::Lt(_, _) => "lt",
            Expr::Le(_, _) => "le",
            Expr::Gt(_, _) => "gt",
            Expr::Ge(_, _) => "ge",
            Expr::Add(_, _) => "add",
            Expr::Sub(_, _) => "sub",
            Expr::Mul(_, _) => "mul",
            Expr::Div(_, _) => "div",
            Expr::Mod(_, _) => "mod",
            Expr::Min(_, _) => "min",
            Expr::Max(_, _) => "max",
        }
    }

    /// Evaluate against a state. Deterministic and total over well-typed
    /// expressions; all failure modes are `EvalError`, never a panic.
    pub fn eval(&self, env: &State) -> Result<Value, EvalError> {
        match self {
            Expr::Var(name) => env
                .get(name)
                .cloned()
                .ok_or_else(|| EvalError::UnknownVariable(name.clone())),
            Expr::Const(v) => Ok(v.clone()),
            Expr::Len(e) => match e.eval(env)? {
                Value::Seq(items) => Ok(Value::Int(items.len() as i128)),
                other => Err(EvalError::TypeMismatch {
                    op: "len",
                    expected: "seq",
                    got: other.type_name().to_string(),
                }),
            },
            Expr::Not(e) => match e.eval(env)? {
                Value::Bool(b) => Ok(Value::Bool(!b)),
                other => Err(EvalError::TypeMismatch {
                    op: "not",
                    expected: "bool",
                    got: other.type_name().to_string(),
                }),
            },
            Expr::And(a, b) => match (a.eval(env)?, b.eval(env)?) {
                (Value::Bool(x), Value::Bool(y)) => Ok(Value::Bool(x && y)),
                (x, _) => Err(EvalError::TypeMismatch {
                    op: "and",
                    expected: "bool, bool",
                    got: x.type_name().to_string(),
                }),
            },
            Expr::Or(a, b) => match (a.eval(env)?, b.eval(env)?) {
                (Value::Bool(x), Value::Bool(y)) => Ok(Value::Bool(x || y)),
                (x, _) => Err(EvalError::TypeMismatch {
                    op: "or",
                    expected: "bool, bool",
                    got: x.type_name().to_string(),
                }),
            },
            Expr::Eq(a, b) => Ok(Value::Bool(a.eval(env)? == b.eval(env)?)),
            Expr::Ne(a, b) => Ok(Value::Bool(a.eval(env)? != b.eval(env)?)),
            Expr::Lt(a, b) => self.ints(a, b, env).map(|(x, y)| Value::Bool(x < y)),
            Expr::Le(a, b) => self.ints(a, b, env).map(|(x, y)| Value::Bool(x <= y)),
            Expr::Gt(a, b) => self.ints(a, b, env).map(|(x, y)| Value::Bool(x > y)),
            Expr::Ge(a, b) => self.ints(a, b, env).map(|(x, y)| Value::Bool(x >= y)),
            Expr::Add(a, b) => self
                .ints(a, b, env)
                .map(|(x, y)| Value::Int(x.wrapping_add(y))),
            Expr::Sub(a, b) => self
                .ints(a, b, env)
                .map(|(x, y)| Value::Int(x.wrapping_sub(y))),
            Expr::Mul(a, b) => self
                .ints(a, b, env)
                .map(|(x, y)| Value::Int(x.wrapping_mul(y))),
            Expr::Div(a, b) => match self.ints(a, b, env)? {
                (_, 0) => Err(EvalError::DivisionByZero),
                (x, y) => Ok(Value::Int(x.wrapping_div(y))),
            },
            Expr::Mod(a, b) => match self.ints(a, b, env)? {
                (_, 0) => Err(EvalError::DivisionByZero),
                (x, y) => Ok(Value::Int(x.wrapping_rem(y))),
            },
            Expr::Min(a, b) => self.ints(a, b, env).map(|(x, y)| Value::Int(x.min(y))),
            Expr::Max(a, b) => self.ints(a, b, env).map(|(x, y)| Value::Int(x.max(y))),
        }
    }

    /// Evaluate and require a boolean result — the common case for
    /// pre/postconditions and invariants.
    pub fn eval_bool(&self, env: &State) -> Result<bool, EvalError> {
        match self.eval(env)? {
            Value::Bool(b) => Ok(b),
            other => Err(EvalError::NotABool(other.to_string())),
        }
    }
}

/// Operator forms: `!a`, `a + b`, `a - b`, `a * b`, `a / b`, `a % b`.
impl std::ops::Not for Expr {
    type Output = Expr;
    fn not(self) -> Expr {
        Expr::Not(Box::new(self))
    }
}

impl std::ops::Add for Expr {
    type Output = Expr;
    fn add(self, rhs: Expr) -> Expr {
        Expr::Add(Box::new(self), Box::new(rhs))
    }
}

impl std::ops::Sub for Expr {
    type Output = Expr;
    fn sub(self, rhs: Expr) -> Expr {
        Expr::Sub(Box::new(self), Box::new(rhs))
    }
}

impl std::ops::Mul for Expr {
    type Output = Expr;
    fn mul(self, rhs: Expr) -> Expr {
        Expr::Mul(Box::new(self), Box::new(rhs))
    }
}

impl std::ops::Div for Expr {
    type Output = Expr;
    fn div(self, rhs: Expr) -> Expr {
        Expr::Div(Box::new(self), Box::new(rhs))
    }
}

impl std::ops::Rem for Expr {
    type Output = Expr;
    fn rem(self, rhs: Expr) -> Expr {
        Expr::Mod(Box::new(self), Box::new(rhs))
    }
}

impl fmt::Display for Expr {
    /// Readable s-expression rendering.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expr::Var(name) => f.write_str(name),
            Expr::Const(v) => write!(f, "{v}"),
            Expr::Len(e) => write!(f, "(len {e})"),
            Expr::Not(e) => write!(f, "(not {e})"),
            Expr::And(a, b) => write!(f, "(and {a} {b})"),
            Expr::Or(a, b) => write!(f, "(or {a} {b})"),
            Expr::Eq(a, b) => write!(f, "(= {a} {b})"),
            Expr::Ne(a, b) => write!(f, "(!= {a} {b})"),
            Expr::Lt(a, b) => write!(f, "(< {a} {b})"),
            Expr::Le(a, b) => write!(f, "(<= {a} {b})"),
            Expr::Gt(a, b) => write!(f, "(> {a} {b})"),
            Expr::Ge(a, b) => write!(f, "(>= {a} {b})"),
            Expr::Add(a, b) => write!(f, "(+ {a} {b})"),
            Expr::Sub(a, b) => write!(f, "(- {a} {b})"),
            Expr::Mul(a, b) => write!(f, "(* {a} {b})"),
            Expr::Div(a, b) => write!(f, "(/ {a} {b})"),
            Expr::Mod(a, b) => write!(f, "(% {a} {b})"),
            Expr::Min(a, b) => write!(f, "(min {a} {b})"),
            Expr::Max(a, b) => write!(f, "(max {a} {b})"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env() -> State {
        let mut s = State::new();
        s.insert("n".into(), Value::Int(5));
        s.insert("flag".into(), Value::Bool(true));
        s.insert("log".into(), Value::Seq(vec![Value::Str("a".into())]));
        s
    }

    #[test]
    fn arithmetic_and_comparison() {
        let e = Expr::ge(Expr::var("n"), Expr::int(3));
        assert_eq!(e.eval_bool(&env()), Ok(true));
        let e = Expr::eq(Expr::var("n") + Expr::int(1), Expr::int(6));
        assert_eq!(e.eval_bool(&env()), Ok(true));
    }

    #[test]
    fn seq_len() {
        let e = Expr::eq(Expr::len(Expr::var("log")), Expr::int(1));
        assert_eq!(e.eval_bool(&env()), Ok(true));
    }

    #[test]
    fn unknown_variable_is_error() {
        let e = Expr::var("nope");
        assert_eq!(
            e.eval(&env()),
            Err(EvalError::UnknownVariable("nope".into()))
        );
    }

    #[test]
    fn type_mismatch_is_error() {
        let e = Expr::var("flag") + Expr::int(1);
        assert!(matches!(
            e.eval(&env()),
            Err(EvalError::TypeMismatch { .. })
        ));
    }

    #[test]
    fn division_by_zero_is_error() {
        let e = Expr::var("n") / Expr::int(0);
        assert_eq!(e.eval(&env()), Err(EvalError::DivisionByZero));
    }

    #[test]
    fn min_max() {
        let e = Expr::eq(Expr::min(Expr::var("n"), Expr::int(10)), Expr::var("n"));
        assert_eq!(e.eval_bool(&env()), Ok(true));
    }

    #[test]
    fn display_is_readable() {
        let e = Expr::le(Expr::var("committed_len"), Expr::len(Expr::var("log")));
        assert_eq!(e.to_string(), "(<= committed_len (len log))");
    }

    #[test]
    fn expr_serializes() {
        let e = Expr::le(Expr::var("x"), Expr::int(10));
        let json = serde_json::to_string(&e).unwrap();
        let back: Expr = serde_json::from_str(&json).unwrap();
        assert_eq!(back, e);
    }
}
