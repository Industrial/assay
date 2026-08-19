use serde_json::Value;

use crate::outcome::AssayOutcome;

/// Evaluate a simple Nix expression to a JSON value.
///
/// Currently supports:
/// - Integer literals (e.g., "42")
/// - String literals (e.g., "\"foo\"")
/// - Addition of two integer literals (e.g., "1 + 1")
/// - Addition of two string literals (e.g., "\"foo\" + \"bar\"")
///
/// Returns Ok(value) if the expression is in the supported subset, Err(()) otherwise.
#[allow(clippy::result_unit_err)]
pub fn eval_simple_expr(expr: &str) -> Result<Value, ()> {
  let expr = expr.trim();

  // Integer literal
  if let Ok(int) = expr.parse::<i64>() {
    Ok(Value::Number(int.into()))
  } else {
    // Addition: split by " + " (note: spaces around +)
    if let Some((left, right)) = expr.split_once(" + ") {
      // Evaluate left and right as simple literals (integer or string)
      let left_val = eval_simple_expr(left)?;
      let right_val = eval_simple_expr(right)?;

      // Integer addition
      if let (Some(left_int), Some(right_int)) = (left_val.as_i64(), right_val.as_i64()) {
        Ok(Value::Number((left_int + right_int).into()))
      } else {
        // String concatenation
        if let (Some(left_str), Some(right_str)) = (left_val.as_str(), right_val.as_str()) {
          Ok(Value::String(format!("{}{}", left_str, right_str)))
        } else {
          // If we get here, the types don't match for addition
          Err(())
        }
      }
    } else {
      // String literal
      if expr.starts_with('\"') && expr.ends_with('\"') {
        // Remove the quotes and escape sequences? We'll just take the content as is for now.
        // Note: This does not handle escaped quotes inside the string.
        let content = &expr[1..expr.len() - 1];
        Ok(Value::String(content.to_string()))
      } else {
        // If none of the above, the expression is not in our supported subset
        Err(())
      }
    }
  }
}

/// Simple Nix evaluation backend that evaluates a subset of Nix expressions without spawning a process.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SimpleNixEval;

impl crate::eval::NixEval for SimpleNixEval {
  fn eval_json(&self, expr: &str) -> crate::eval::EvalResult {
    match eval_simple_expr(expr) {
      Ok(value) => crate::eval::EvalResult::Ok(value),
      Err(()) => crate::eval::EvalResult::Err(AssayOutcome::EvalError {
        kind: "unsupported_simple_expr".to_string(),
        message: format!("expression not supported by simple eval: {expr}"),
        span: None,
      }),
    }
  }
}
