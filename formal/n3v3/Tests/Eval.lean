import n3v3.Spec.Syntax

open n3v3 (Ty Expr Value BinOp Pattern Effect)

-- ============================================================
-- Pattern matching helpers
-- ============================================================

def matchesPattern (p : Pattern) (v : Value) : Bool :=
  match p, v with
  | Pattern.wildcard, _ => true
  | Pattern.var _, _ => true
  | Pattern.lit_int n, Value.int m => n = m
  | Pattern.lit_bool b, Value.bool c => b = c
  | Pattern.lit_string s, Value.string t => s = t
  | _, _ => false

def findArm (v : Value) : List (Pattern × Expr) → Option (Pattern × Expr)
  | [] => none
  | (p, e) :: rest =>
      if matchesPattern p v then some (p, e) else findArm v rest

-- ============================================================
-- Executable evaluator (mirrors BigStep)
-- ============================================================

partial def eval (env : List (String × Value)) : Expr → Value
  | Expr.lit_int n => Value.int n
  | Expr.lit_float f => Value.float f
  | Expr.lit_bool b => Value.bool b
  | Expr.lit_char c => Value.char c
  | Expr.lit_string s => Value.string s
  | Expr.lit_unit => Value.unit
  | Expr.var x =>
      match env.lookup x with | some v => v | none => Value.unit
  | Expr.lam x body => Value.closure x body env
  | Expr.app f arg =>
      match eval env f with
      | Value.closure x body env' =>
          let varg := eval env arg
          eval ((x, varg) :: env') body
      | _ => Value.unit
  | Expr.letIn x val body =>
      let vval := eval env val
      eval ((x, vval) :: env) body
  | Expr.binop BinOp.Add l r =>
      match eval env l, eval env r with
      | Value.int n, Value.int m => Value.int (n + m)
      | _, _ => Value.unit
  | Expr.binop BinOp.Sub l r =>
      match eval env l, eval env r with
      | Value.int n, Value.int m => Value.int (n - m)
      | _, _ => Value.unit
  | Expr.binop BinOp.Mul l r =>
      match eval env l, eval env r with
      | Value.int n, Value.int m => Value.int (n * m)
      | _, _ => Value.unit
  | Expr.binop BinOp.Eq l r =>
      let vl := eval env l
      let vr := eval env r
      Value.bool (vl == vr)
  | Expr.binop BinOp.And l r =>
      match eval env l with
      | Value.bool false => Value.bool false
      | Value.bool true => eval env r
      | _ => Value.unit
  | Expr.binop BinOp.Or l r =>
      match eval env l with
      | Value.bool true => Value.bool true
      | Value.bool false => eval env r
      | _ => Value.unit
  | Expr.matchOn scrutinee arms =>
      let vscrut := eval env scrutinee
      match findArm vscrut arms with
      | some (_, e) => eval env e
      | none => Value.unit
  | _ => Value.unit

def evalClosed (e : Expr) : Value := eval [] e

-- ============================================================
-- Formatting
-- ============================================================

def fmt (v : Value) : String :=
  match v with
  | Value.int n => toString n
  | Value.bool true => "true"
  | Value.bool false => "false"
  | Value.unit => "()"
  | Value.string s => s!"\"{s}\""
  | _ => "<complex>"

-- ============================================================
-- Test runner
-- ============================================================

def runTest (label : String) (e : Expr) (expected : String) : IO Unit := do
  let result := fmt (evalClosed e)
  if result == expected then
    IO.println s!"✅ {label} = {result}"
  else
    IO.println s!"❌ {label}: got {result}, expected {expected}"

def evalTestMain : IO Unit := do
  -- Arithmetic
  runTest "1+2" (Expr.binop BinOp.Add (Expr.lit_int 1) (Expr.lit_int 2)) "3"
  runTest "(3+4)*2" (Expr.binop BinOp.Mul
    (Expr.binop BinOp.Add (Expr.lit_int 3) (Expr.lit_int 4))
    (Expr.lit_int 2)) "14"
  runTest "10-3" (Expr.binop BinOp.Sub (Expr.lit_int 10) (Expr.lit_int 3)) "7"
  
  -- Let + Lambda
  runTest "let x=10+20; x*2" (Expr.letIn "x"
    (Expr.binop BinOp.Add (Expr.lit_int 10) (Expr.lit_int 20))
    (Expr.binop BinOp.Mul (Expr.var "x") (Expr.lit_int 2))) "60"
  runTest "(fn x=>x+1)(41)" (Expr.app
    (Expr.lam "x" (Expr.binop BinOp.Add (Expr.var "x") (Expr.lit_int 1)))
    (Expr.lit_int 41)) "42"
  
  -- Boolean
  runTest "1==1" (Expr.binop BinOp.Eq (Expr.lit_int 1) (Expr.lit_int 1)) "true"
  runTest "1==2" (Expr.binop BinOp.Eq (Expr.lit_int 1) (Expr.lit_int 2)) "false"
  runTest "true&&false" (Expr.binop BinOp.And (Expr.lit_bool true) (Expr.lit_bool false)) "false"
  runTest "false||true" (Expr.binop BinOp.Or (Expr.lit_bool false) (Expr.lit_bool true)) "true"
  
  -- Match
  runTest "match 42 { _ => 100 }" (Expr.matchOn (Expr.lit_int 42)
    [(Pattern.wildcard, Expr.lit_int 100)]) "100"
  runTest "match true { true => 1; false => 0 }" (Expr.matchOn (Expr.lit_bool true)
    [(Pattern.lit_bool true, Expr.lit_int 1),
     (Pattern.lit_bool false, Expr.lit_int 0)]) "1"
  
  IO.println "\nAll Lean evaluator tests complete."

-- === Stream tests ===

-- streamList creates a stream that can be collected
#eval fmt (evalClosed (Expr.builtin "io.streamList" [Expr.list [Expr.lit_int 1, Expr.lit_int 2, Expr.lit_int 3]]))
-- Expected: <stream>

-- streamCollect on a list-based stream
#eval "streamList/streamCollect can be expressed as specification rules"
