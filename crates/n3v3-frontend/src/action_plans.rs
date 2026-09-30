//! Action-plan collection from canonical HIR.
//! 从规范 HIR 收集动作计划。

use n3v3_common::{ActionPlan, HostOp, HostOpKind, Span, intrinsic_metadata};
use n3v3_hir::{EnumDef, Expr, ExprKind, Item, ItemKind, Literal, Module, Stmt, StmtKind};
use std::collections::HashMap;

/// Collect typed host plans without executing any host operation.
/// 收集类型化宿主计划，但不执行任何宿主操作。
pub(crate) fn collect_action_plans(module: &Module) -> HashMap<Span, ActionPlan> {
    let mut plans = HashMap::new();
    for item in &module.items {
        visit_item(item, &mut plans);
    }
    plans
}

fn visit_item(item: &Item, plans: &mut HashMap<Span, ActionPlan>) {
    match &item.kind {
        ItemKind::Fn(function) => visit_expr(&function.body, plans),
        ItemKind::Expr(expression) => visit_expr(expression, plans),
        ItemKind::Struct(structure) => {
            for field in &structure.fields {
                if let Some(default) = &field.default {
                    visit_expr(default, plans);
                }
            }
        }
        ItemKind::Enum(enumeration) => visit_enum_defaults(enumeration, plans),
        ItemKind::Trait(trait_def) => {
            for method in &trait_def.items {
                if let Some(default) = &method.default {
                    visit_expr(default, plans);
                }
            }
        }
        ItemKind::Impl(implementation) => {
            for method in &implementation.items {
                visit_expr(&method.body, plans);
            }
        }
        ItemKind::TypeAlias(_) => {}
    }
}

fn visit_enum_defaults(enumeration: &EnumDef, plans: &mut HashMap<Span, ActionPlan>) {
    for variant in &enumeration.variants {
        if let Some(fields) = &variant.record_fields {
            for field in fields {
                if let Some(default) = &field.default {
                    visit_expr(default, plans);
                }
            }
        }
    }
}

fn visit_expr(expression: &Expr, plans: &mut HashMap<Span, ActionPlan>) {
    if let Some(plan) = plan_for_call(expression) {
        plans.insert(expression.span, plan);
    }

    match &expression.kind {
        ExprKind::Record(fields) => {
            for (_, value) in fields {
                visit_expr(value, plans);
            }
        }
        ExprKind::List(items) | ExprKind::Tuple(items) => {
            for item in items {
                visit_expr(item, plans);
            }
        }
        ExprKind::Lambda { body, .. }
        | ExprKind::Lazy(body)
        | ExprKind::Try(body)
        | ExprKind::TupleIndex(body, _) => visit_expr(body, plans),
        ExprKind::Call(function, args) => {
            visit_expr(function, plans);
            for arg in args {
                visit_expr(arg, plans);
            }
        }
        ExprKind::MethodCall {
            receiver,
            target,
            args,
            ..
        } => {
            visit_expr(receiver, plans);
            visit_expr(target, plans);
            for arg in args {
                visit_expr(arg, plans);
            }
        }
        ExprKind::Field(base, _) => visit_expr(base, plans),
        ExprKind::SafeField { base, .. } => visit_expr(base, plans),
        ExprKind::Index { base, index } => {
            visit_expr(base, plans);
            visit_expr(index, plans);
        }
        ExprKind::Binary(_, left, right) => {
            visit_expr(left, plans);
            visit_expr(right, plans);
        }
        ExprKind::Unary(_, operand) => visit_expr(operand, plans),
        ExprKind::If(condition, then_branch, else_branch) => {
            visit_expr(condition, plans);
            visit_expr(then_branch, plans);
            visit_expr(else_branch, plans);
        }
        ExprKind::Coalesce { value, default } => {
            visit_expr(value, plans);
            visit_expr(default, plans);
        }
        ExprKind::Match(scrutinee, arms) => {
            visit_expr(scrutinee, plans);
            for arm in arms {
                if let Some(guard) = &arm.guard {
                    visit_expr(guard, plans);
                }
                visit_expr(&arm.body, plans);
            }
        }
        ExprKind::Block(statements, result) => {
            for statement in statements {
                visit_stmt(statement, plans);
            }
            if let Some(result) = result {
                visit_expr(result, plans);
            }
        }
        ExprKind::Let { value, body, .. } => {
            visit_expr(value, plans);
            visit_expr(body, plans);
        }
        ExprKind::ListComp { body, generators } => {
            for generator in generators {
                visit_expr(&generator.iter, plans);
                if let Some(condition) = &generator.condition {
                    visit_expr(condition, plans);
                }
            }
            visit_expr(body, plans);
        }
        ExprKind::Interpolated(parts) => {
            for part in parts {
                if let n3v3_hir::StringPart::Expr(expression) = part {
                    visit_expr(expression, plans);
                }
            }
        }
        ExprKind::Literal(_)
        | ExprKind::Var(_)
        | ExprKind::Global(_)
        | ExprKind::Builtin(_)
        | ExprKind::Error(_) => {}
    }
}

fn visit_stmt(statement: &Stmt, plans: &mut HashMap<Span, ActionPlan>) {
    match &statement.kind {
        StmtKind::Let { value, .. } | StmtKind::Expr(value) => visit_expr(value, plans),
    }
}

fn plan_for_call(expression: &Expr) -> Option<ActionPlan> {
    let ExprKind::Call(function, args) = &expression.kind else {
        return None;
    };
    let ExprKind::Builtin(name) = &function.kind else {
        return None;
    };
    let metadata = intrinsic_metadata(name)?;
    if metadata.host_op != Some(HostOpKind::ReadFile) || args.len() != 1 {
        return None;
    }
    let [argument] = args.as_slice() else {
        return None;
    };
    let Literal::String(path) = argument_literal(argument)? else {
        return None;
    };

    Some(ActionPlan::new(
        expression.span,
        HostOp::ReadFile { path: path.clone() },
    ))
}

fn argument_literal(expression: &Expr) -> Option<&Literal> {
    match &expression.kind {
        ExprKind::Literal(literal) => Some(literal),
        _ => None,
    }
}
