//! Builtin type resolution for the type checker.
//! 类型检查器的内置类型解析。

use super::TypeChecker;
use crate::builtin_types::{
    builtin_bytes, builtin_command, builtin_event, builtin_list, builtin_live, builtin_map,
    builtin_option, builtin_path, builtin_pipeline, builtin_process_result, builtin_redirect,
    builtin_result, builtin_set, builtin_stream, builtin_task,
};
use crate::unify::instantiate;
use n3v3_common::Span;
use n3v3_hir::{Ty, TyKind};

fn builtin_ty(kind: TyKind, span: Span) -> Ty {
    Ty { kind, span }
}

fn builtin_param(idx: u32, name: &str, span: Span) -> Ty {
    builtin_ty(TyKind::Param(idx, name.to_string()), span)
}

fn builtin_fn(params: Vec<Ty>, ret: Ty, span: Span) -> Ty {
    builtin_ty(TyKind::Fn(params, Box::new(ret)), span)
}

fn builtin_forall(params: Vec<&str>, body: Ty, span: Span) -> Ty {
    builtin_ty(
        TyKind::Forall(
            params.into_iter().map(|param| param.to_string()).collect(),
            Box::new(body),
        ),
        span,
    )
}

fn builtin_record(fields: Vec<(&str, Ty)>, span: Span) -> Ty {
    builtin_ty(
        TyKind::Record(
            fields
                .into_iter()
                .map(|(name, ty)| (name.to_string(), ty))
                .collect(),
        ),
        span,
    )
}

fn builtin_safe_record_base(fields: Vec<(&str, Ty)>, span: Span) -> Ty {
    builtin_ty(
        TyKind::SafeRecordBase(
            fields
                .into_iter()
                .map(|(name, ty)| (name.to_string(), ty))
                .collect(),
        ),
        span,
    )
}

fn builtin_string(span: Span) -> Ty {
    builtin_ty(TyKind::String, span)
}

fn builtin_string_option(span: Span) -> Ty {
    builtin_option(builtin_string(span), span)
}

fn builtin_path_option(span: Span) -> Ty {
    builtin_option(builtin_path(span), span)
}

fn builtin_string_list(span: Span) -> Ty {
    builtin_list(builtin_ty(TyKind::String, span), span)
}

fn builtin_path_list(span: Span) -> Ty {
    builtin_list(builtin_path(span), span)
}

fn builtin_exec_with_options(span: Span) -> Ty {
    builtin_safe_record_base(
        vec![
            ("program", builtin_ty(TyKind::String, span)),
            ("args", builtin_string_list(span)),
            ("cwd", builtin_ty(TyKind::String, span)),
            ("stdin", builtin_ty(TyKind::String, span)),
            ("env", builtin_ty(TyKind::DynamicRecord(Vec::new()), span)),
        ],
        span,
    )
}

fn builtin_fetch_result(span: Span) -> Ty {
    builtin_record(
        vec![
            ("path", builtin_ty(TyKind::String, span)),
            ("hash", builtin_ty(TyKind::String, span)),
            ("cached", builtin_ty(TyKind::Bool, span)),
        ],
        span,
    )
}
fn builtin_job(span: Span) -> Ty {
    builtin_record(
        vec![
            ("id", builtin_ty(TyKind::Int, span)),
            ("state", builtin_ty(TyKind::String, span)),
        ],
        span,
    )
}

fn builtin_job_result(span: Span) -> Ty {
    builtin_record(
        vec![
            ("id", builtin_ty(TyKind::Int, span)),
            ("result", builtin_process_result(span)),
        ],
        span,
    )
}

impl TypeChecker {
    pub(super) fn builtin_type(&mut self, name: &str, span: Span) -> Option<Ty> {
        let polymorphic = match name {
            "force" => Ty {
                kind: TyKind::Forall(
                    vec!["a".to_string()],
                    Box::new(Ty {
                        kind: TyKind::Fn(
                            vec![Ty {
                                kind: TyKind::Param(0, "a".to_string()),
                                span,
                            }],
                            Box::new(Ty {
                                kind: TyKind::Param(0, "a".to_string()),
                                span,
                            }),
                        ),
                        span,
                    }),
                ),
                span,
            },
            "isLazy" | "isEvaluated" => Ty {
                kind: TyKind::Forall(
                    vec!["a".to_string()],
                    Box::new(Ty {
                        kind: TyKind::Fn(
                            vec![Ty {
                                kind: TyKind::Param(0, "a".to_string()),
                                span,
                            }],
                            Box::new(Ty {
                                kind: TyKind::Bool,
                                span,
                            }),
                        ),
                        span,
                    }),
                ),
                span,
            },
            "toString" => {
                let a = builtin_param(0, "a", span);
                builtin_forall(
                    Vec::from(["a"]),
                    builtin_fn(vec![a], builtin_ty(TyKind::String, span), span),
                    span,
                )
            }
            "id" => {
                let a = builtin_param(0, "a", span);
                builtin_forall(Vec::from(["a"]), builtin_fn(vec![a.clone()], a, span), span)
            }
            "const" => {
                let a = builtin_param(0, "a", span);
                let b = builtin_param(1, "b", span);
                builtin_forall(
                    Vec::from(["a", "b"]),
                    builtin_fn(vec![a.clone(), b], a, span),
                    span,
                )
            }
            "print" | "println" | "io.print" | "io.println" => {
                let a = builtin_param(0, "a", span);
                builtin_forall(
                    Vec::from(["a"]),
                    builtin_fn(vec![a], builtin_ty(TyKind::Unit, span), span),
                    span,
                )
            }
            "len" => {
                let a = builtin_param(0, "a", span);
                builtin_forall(
                    Vec::from(["a"]),
                    builtin_fn(vec![a], builtin_ty(TyKind::Int, span), span),
                    span,
                )
            }
            "typeOf" => {
                let a = builtin_param(0, "a", span);
                builtin_forall(
                    Vec::from(["a"]),
                    builtin_fn(vec![a], builtin_ty(TyKind::String, span), span),
                    span,
                )
            }
            "toInt" => {
                let a = builtin_param(0, "a", span);
                builtin_forall(
                    Vec::from(["a"]),
                    builtin_fn(vec![a], builtin_ty(TyKind::Int, span), span),
                    span,
                )
            }
            "toFloat" => {
                let a = builtin_param(0, "a", span);
                builtin_forall(
                    Vec::from(["a"]),
                    builtin_fn(vec![a], builtin_ty(TyKind::Float, span), span),
                    span,
                )
            }
            "Some" => {
                let a = builtin_param(0, "a", span);
                builtin_forall(
                    Vec::from(["a"]),
                    builtin_fn(vec![a.clone()], builtin_option(a, span), span),
                    span,
                )
            }
            "None" => builtin_forall(
                Vec::from(["a"]),
                builtin_option(builtin_param(0, "a", span), span),
                span,
            ),
            "Ok" => {
                let a = builtin_param(0, "a", span);
                let _e = builtin_param(1, "e", span);
                builtin_forall(
                    Vec::from(["a", "e"]),
                    builtin_fn(
                        vec![a.clone()],
                        builtin_result(a, builtin_param(1, "e", span), span),
                        span,
                    ),
                    span,
                )
            }
            "Err" => {
                let _a = builtin_param(0, "a", span);
                let e = builtin_param(1, "e", span);
                builtin_forall(
                    Vec::from(["a", "e"]),
                    builtin_fn(
                        vec![e.clone()],
                        builtin_result(builtin_param(0, "a", span), e, span),
                        span,
                    ),
                    span,
                )
            }
            "assert" => builtin_fn(
                vec![builtin_ty(TyKind::Bool, span)],
                builtin_ty(TyKind::Unit, span),
                span,
            ),
            "assertEq" => {
                let a = builtin_param(0, "a", span);
                builtin_forall(
                    Vec::from(["a"]),
                    builtin_fn(vec![a.clone(), a], builtin_ty(TyKind::Unit, span), span),
                    span,
                )
            }
            "trace" => {
                let a = builtin_param(0, "a", span);
                let b = builtin_param(1, "b", span);
                builtin_forall(
                    Vec::from(["a", "b"]),
                    builtin_fn(vec![a, b.clone()], b, span),
                    span,
                )
            }
            "list.empty" => {
                let a = builtin_param(0, "a", span);
                builtin_forall(Vec::from(["a"]), builtin_list(a, span), span)
            }
            "list.singleton" => {
                let a = builtin_param(0, "a", span);
                builtin_forall(
                    Vec::from(["a"]),
                    builtin_fn(vec![a.clone()], builtin_list(a, span), span),
                    span,
                )
            }
            "list.len" => {
                let a = builtin_param(0, "a", span);
                builtin_forall(
                    Vec::from(["a"]),
                    builtin_fn(
                        vec![builtin_list(a, span)],
                        builtin_ty(TyKind::Int, span),
                        span,
                    ),
                    span,
                )
            }
            "list.isEmpty" => {
                let a = builtin_param(0, "a", span);
                builtin_forall(
                    Vec::from(["a"]),
                    builtin_fn(
                        vec![builtin_list(a, span)],
                        builtin_ty(TyKind::Bool, span),
                        span,
                    ),
                    span,
                )
            }
            "list.head" | "list.last" => {
                let a = builtin_param(0, "a", span);
                builtin_forall(
                    Vec::from(["a"]),
                    builtin_fn(
                        vec![builtin_list(a.clone(), span)],
                        builtin_option(a, span),
                        span,
                    ),
                    span,
                )
            }
            "list.tail" => {
                let a = builtin_param(0, "a", span);
                builtin_forall(
                    Vec::from(["a"]),
                    builtin_fn(
                        vec![builtin_list(a.clone(), span)],
                        builtin_list(a, span),
                        span,
                    ),
                    span,
                )
            }
            "list.init" | "list.reverse" => {
                let a = builtin_param(0, "a", span);
                builtin_forall(
                    Vec::from(["a"]),
                    builtin_fn(
                        vec![builtin_list(a.clone(), span)],
                        builtin_list(a, span),
                        span,
                    ),
                    span,
                )
            }
            "list.get" => {
                let a = builtin_param(0, "a", span);
                builtin_forall(
                    Vec::from(["a"]),
                    builtin_fn(
                        vec![builtin_ty(TyKind::Int, span), builtin_list(a.clone(), span)],
                        builtin_option(a, span),
                        span,
                    ),
                    span,
                )
            }
            "list.cons" => {
                let a = builtin_param(0, "a", span);
                let list_a = builtin_list(a.clone(), span);
                builtin_forall(
                    Vec::from(["a"]),
                    builtin_fn(vec![a, list_a.clone()], list_a, span),
                    span,
                )
            }
            "list.take" => {
                let a = builtin_param(0, "a", span);
                let list_a = builtin_list(a.clone(), span);
                builtin_forall(
                    Vec::from(["a"]),
                    builtin_fn(
                        vec![builtin_ty(TyKind::Int, span), list_a.clone()],
                        list_a,
                        span,
                    ),
                    span,
                )
            }
            "list.drop" => {
                let a = builtin_param(0, "a", span);
                let list_a = builtin_list(a.clone(), span);
                builtin_forall(
                    Vec::from(["a"]),
                    builtin_fn(
                        vec![builtin_ty(TyKind::Int, span), list_a.clone()],
                        list_a,
                        span,
                    ),
                    span,
                )
            }
            "list.contains" => {
                let a = builtin_param(0, "a", span);
                builtin_forall(
                    Vec::from(["a"]),
                    builtin_fn(
                        vec![a.clone(), builtin_list(a, span)],
                        builtin_ty(TyKind::Bool, span),
                        span,
                    ),
                    span,
                )
            }
            "list.indexOf" => {
                let a = builtin_param(0, "a", span);
                let list_a = builtin_list(a.clone(), span);
                builtin_forall(
                    Vec::from(["a"]),
                    builtin_fn(
                        vec![a, list_a],
                        builtin_option(builtin_ty(TyKind::Int, span), span),
                        span,
                    ),
                    span,
                )
            }
            "list.append" => {
                let a = builtin_param(0, "a", span);
                let list_a = builtin_list(a.clone(), span);
                builtin_forall(
                    Vec::from(["a"]),
                    builtin_fn(vec![list_a.clone(), list_a.clone()], list_a, span),
                    span,
                )
            }
            "list.map" => {
                let a = builtin_param(0, "a", span);
                let b = builtin_param(1, "b", span);
                builtin_forall(
                    Vec::from(["a", "b"]),
                    builtin_fn(
                        vec![
                            builtin_fn(vec![a.clone()], b.clone(), span),
                            builtin_list(a, span),
                        ],
                        builtin_list(b, span),
                        span,
                    ),
                    span,
                )
            }
            "list.filter" => {
                let a = builtin_param(0, "a", span);
                builtin_forall(
                    Vec::from(["a"]),
                    builtin_fn(
                        vec![
                            builtin_fn(vec![a.clone()], builtin_ty(TyKind::Bool, span), span),
                            builtin_list(a.clone(), span),
                        ],
                        builtin_list(a, span),
                        span,
                    ),
                    span,
                )
            }
            "list.fold" => {
                let a = builtin_param(0, "a", span);
                let b = builtin_param(1, "b", span);
                builtin_forall(
                    Vec::from(["a", "b"]),
                    builtin_fn(
                        vec![
                            b.clone(),
                            builtin_fn(vec![b.clone(), a.clone()], b.clone(), span),
                            builtin_list(a, span),
                        ],
                        b,
                        span,
                    ),
                    span,
                )
            }
            "list.foldRight" => {
                let a = builtin_param(0, "a", span);
                let b = builtin_param(1, "b", span);
                builtin_forall(
                    Vec::from(["a", "b"]),
                    builtin_fn(
                        vec![
                            b.clone(),
                            builtin_fn(vec![a.clone(), b.clone()], b.clone(), span),
                            builtin_list(a, span),
                        ],
                        b,
                        span,
                    ),
                    span,
                )
            }
            "list.sum" => builtin_fn(
                vec![builtin_list(builtin_ty(TyKind::Int, span), span)],
                builtin_ty(TyKind::Int, span),
                span,
            ),
            "list.product" => builtin_fn(
                vec![builtin_list(builtin_ty(TyKind::Int, span), span)],
                builtin_ty(TyKind::Int, span),
                span,
            ),
            "list.sort" => {
                let a = builtin_param(0, "a", span);
                let list_a = builtin_list(a.clone(), span);
                builtin_forall(
                    Vec::from(["a"]),
                    builtin_fn(vec![list_a.clone()], list_a, span),
                    span,
                )
            }
            "list.max" | "list.min" => builtin_fn(
                vec![builtin_list(builtin_ty(TyKind::Int, span), span)],
                builtin_option(builtin_ty(TyKind::Int, span), span),
                span,
            ),
            "list.range" => builtin_fn(
                vec![builtin_ty(TyKind::Int, span), builtin_ty(TyKind::Int, span)],
                builtin_list(builtin_ty(TyKind::Int, span), span),
                span,
            ),
            "list.replicate" => {
                let a = builtin_param(0, "a", span);
                builtin_forall(
                    Vec::from(["a"]),
                    builtin_fn(
                        vec![builtin_ty(TyKind::Int, span), a.clone()],
                        builtin_list(a, span),
                        span,
                    ),
                    span,
                )
            }
            "list.zip" => {
                let a = builtin_param(0, "a", span);
                let b = builtin_param(1, "b", span);
                let pair = builtin_ty(TyKind::Tuple(vec![a.clone(), b.clone()]), span);
                builtin_forall(
                    Vec::from(["a", "b"]),
                    builtin_fn(
                        vec![builtin_list(a, span), builtin_list(b, span)],
                        builtin_list(pair, span),
                        span,
                    ),
                    span,
                )
            }
            "list.unzip" => {
                let a = builtin_param(0, "a", span);
                let b = builtin_param(1, "b", span);
                let pair = builtin_ty(TyKind::Tuple(vec![a.clone(), b.clone()]), span);
                let result = builtin_ty(
                    TyKind::Tuple(vec![builtin_list(a, span), builtin_list(b, span)]),
                    span,
                );
                builtin_forall(
                    Vec::from(["a", "b"]),
                    builtin_fn(vec![builtin_list(pair, span)], result, span),
                    span,
                )
            }
            // bytes module
            "bytes.len" => builtin_fn(
                vec![builtin_bytes(span)],
                builtin_ty(TyKind::Int, span),
                span,
            ),
            "bytes.isEmpty" => builtin_fn(
                vec![builtin_bytes(span)],
                builtin_ty(TyKind::Bool, span),
                span,
            ),
            "bytes.concat" => builtin_fn(
                vec![builtin_bytes(span), builtin_bytes(span)],
                builtin_bytes(span),
                span,
            ),
            "bytes.fromString" => builtin_fn(
                vec![builtin_ty(TyKind::String, span)],
                builtin_bytes(span),
                span,
            ),
            "bytes.toString" => builtin_fn(
                vec![builtin_bytes(span)],
                builtin_ty(TyKind::String, span),
                span,
            ),
            "bytes.toList" => builtin_fn(
                vec![builtin_bytes(span)],
                builtin_list(builtin_ty(TyKind::Int, span), span),
                span,
            ),
            "bytes.fromList" => builtin_fn(
                vec![builtin_list(builtin_ty(TyKind::Int, span), span)],
                builtin_bytes(span),
                span,
            ),
            "string.len" => builtin_fn(
                vec![builtin_ty(TyKind::String, span)],
                builtin_ty(TyKind::Int, span),
                span,
            ),
            "string.split" => builtin_fn(
                vec![
                    builtin_ty(TyKind::String, span),
                    builtin_ty(TyKind::String, span),
                ],
                builtin_list(builtin_ty(TyKind::String, span), span),
                span,
            ),
            "string.join" => builtin_fn(
                vec![
                    builtin_list(builtin_ty(TyKind::String, span), span),
                    builtin_ty(TyKind::String, span),
                ],
                builtin_ty(TyKind::String, span),
                span,
            ),
            "string.trim" | "string.upper" | "string.lower" => builtin_fn(
                vec![builtin_ty(TyKind::String, span)],
                builtin_ty(TyKind::String, span),
                span,
            ),
            "string.contains" | "string.startsWith" | "string.endsWith" => builtin_fn(
                vec![
                    builtin_ty(TyKind::String, span),
                    builtin_ty(TyKind::String, span),
                ],
                builtin_ty(TyKind::Bool, span),
                span,
            ),
            "string.replace" => builtin_fn(
                vec![
                    builtin_ty(TyKind::String, span),
                    builtin_ty(TyKind::String, span),
                    builtin_ty(TyKind::String, span),
                ],
                builtin_ty(TyKind::String, span),
                span,
            ),
            "string.substring" => builtin_fn(
                vec![
                    builtin_ty(TyKind::String, span),
                    builtin_ty(TyKind::Int, span),
                    builtin_ty(TyKind::Int, span),
                ],
                builtin_ty(TyKind::String, span),
                span,
            ),
            "string.isEmpty" => builtin_fn(
                vec![builtin_ty(TyKind::String, span)],
                builtin_ty(TyKind::Bool, span),
                span,
            ),
            "string.repeat" => builtin_fn(
                vec![
                    builtin_ty(TyKind::String, span),
                    builtin_ty(TyKind::Int, span),
                ],
                builtin_ty(TyKind::String, span),
                span,
            ),
            "string.lines" => builtin_fn(
                vec![builtin_ty(TyKind::String, span)],
                builtin_list(builtin_ty(TyKind::String, span), span),
                span,
            ),
            "string.chars" => builtin_fn(
                vec![builtin_ty(TyKind::String, span)],
                builtin_list(builtin_ty(TyKind::Char, span), span),
                span,
            ),
            "option.some" => {
                let a = builtin_param(0, "a", span);
                builtin_forall(
                    Vec::from(["a"]),
                    builtin_fn(vec![a.clone()], builtin_option(a, span), span),
                    span,
                )
            }
            "option.none" => {
                let a = builtin_param(0, "a", span);
                builtin_forall(Vec::from(["a"]), builtin_option(a, span), span)
            }
            "option.is_some" | "option.is_none" => {
                let a = builtin_param(0, "a", span);
                builtin_forall(
                    Vec::from(["a"]),
                    builtin_fn(
                        vec![builtin_option(a, span)],
                        builtin_ty(TyKind::Bool, span),
                        span,
                    ),
                    span,
                )
            }
            "option.unwrap" => {
                let a = builtin_param(0, "a", span);
                builtin_forall(
                    Vec::from(["a"]),
                    builtin_fn(vec![builtin_option(a.clone(), span)], a, span),
                    span,
                )
            }
            "option.unwrap_or" => {
                let a = builtin_param(0, "a", span);
                builtin_forall(
                    Vec::from(["a"]),
                    builtin_fn(vec![builtin_option(a.clone(), span), a.clone()], a, span),
                    span,
                )
            }
            "result.ok" => {
                let t = builtin_param(0, "t", span);
                let e = builtin_param(1, "e", span);
                builtin_forall(
                    Vec::from(["t", "e"]),
                    builtin_fn(vec![t.clone()], builtin_result(t, e, span), span),
                    span,
                )
            }
            "result.err" => {
                let t = builtin_param(0, "t", span);
                let e = builtin_param(1, "e", span);
                builtin_forall(
                    Vec::from(["t", "e"]),
                    builtin_fn(vec![e.clone()], builtin_result(t, e, span), span),
                    span,
                )
            }
            "result.is_ok" | "result.is_err" => {
                let t = builtin_param(0, "t", span);
                let e = builtin_param(1, "e", span);
                builtin_forall(
                    Vec::from(["t", "e"]),
                    builtin_fn(
                        vec![builtin_result(t, e, span)],
                        builtin_ty(TyKind::Bool, span),
                        span,
                    ),
                    span,
                )
            }
            "result.unwrap" => {
                let t = builtin_param(0, "t", span);
                let e = builtin_param(1, "e", span);
                builtin_forall(
                    Vec::from(["t", "e"]),
                    builtin_fn(vec![builtin_result(t.clone(), e, span)], t, span),
                    span,
                )
            }
            "result.unwrap_err" => {
                let t = builtin_param(0, "t", span);
                let e = builtin_param(1, "e", span);
                builtin_forall(
                    Vec::from(["t", "e"]),
                    builtin_fn(vec![builtin_result(t, e.clone(), span)], e, span),
                    span,
                )
            }
            "math.toInt" => {
                let a = builtin_param(0, "a", span);
                builtin_forall(
                    Vec::from(["a"]),
                    builtin_fn(vec![a], builtin_ty(TyKind::Int, span), span),
                    span,
                )
            }
            "math.toFloat" => {
                let a = builtin_param(0, "a", span);
                builtin_forall(
                    Vec::from(["a"]),
                    builtin_fn(vec![a], builtin_ty(TyKind::Float, span), span),
                    span,
                )
            }
            "math.isNan" | "math.isInf" => builtin_fn(
                vec![builtin_ty(TyKind::Float, span)],
                builtin_ty(TyKind::Bool, span),
                span,
            ),
            "math.floor" | "math.ceil" | "math.round" => builtin_fn(
                vec![builtin_ty(TyKind::Float, span)],
                builtin_ty(TyKind::Int, span),
                span,
            ),
            "math.sqrt" | "math.log" | "math.log10" | "math.exp" => builtin_fn(
                vec![builtin_ty(TyKind::Float, span)],
                builtin_ty(TyKind::Float, span),
                span,
            ),
            "math.sin" | "math.cos" | "math.tan" => builtin_fn(
                vec![builtin_ty(TyKind::Float, span)],
                builtin_ty(TyKind::Float, span),
                span,
            ),
            "math.pi" | "math.e" | "math.inf" | "math.nan" => builtin_ty(TyKind::Float, span),
            "path.fromString" => builtin_fn(
                vec![builtin_ty(TyKind::String, span)],
                builtin_path(span),
                span,
            ),
            "path.joinPath" => builtin_fn(
                vec![builtin_path(span), builtin_ty(TyKind::String, span)],
                builtin_path(span),
                span,
            ),
            "path.parentPath" => builtin_fn(
                vec![builtin_path(span)],
                builtin_option(builtin_path(span), span),
                span,
            ),
            "path.filenamePath" => builtin_fn(
                vec![builtin_path(span)],
                builtin_option(builtin_ty(TyKind::String, span), span),
                span,
            ),
            "path.extensionPath" => builtin_fn(
                vec![builtin_path(span)],
                builtin_option(builtin_ty(TyKind::String, span), span),
                span,
            ),
            "path.isAbsolutePath" => builtin_fn(
                vec![builtin_path(span)],
                builtin_ty(TyKind::Bool, span),
                span,
            ),
            "path.join" => builtin_fn(
                vec![
                    builtin_ty(TyKind::String, span),
                    builtin_ty(TyKind::String, span),
                ],
                builtin_ty(TyKind::String, span),
                span,
            ),
            "path.parent" | "path.filename" | "path.extension" => builtin_fn(
                vec![builtin_ty(TyKind::String, span)],
                builtin_option(builtin_ty(TyKind::String, span), span),
                span,
            ),
            "path.is_absolute" => builtin_fn(
                vec![builtin_ty(TyKind::String, span)],
                builtin_ty(TyKind::Bool, span),
                span,
            ),
            "io.readFile" | "io.hashFile" | "io.hashString" => builtin_fn(
                vec![builtin_ty(TyKind::String, span)],
                builtin_ty(TyKind::String, span),
                span,
            ),
            "io.hashFilePath" => builtin_fn(
                vec![builtin_path(span)],
                builtin_ty(TyKind::String, span),
                span,
            ),
            "io.readFilePath" => builtin_fn(
                vec![builtin_path(span)],
                builtin_ty(TyKind::String, span),
                span,
            ),
            "io.readDir" => builtin_fn(
                vec![builtin_ty(TyKind::String, span)],
                builtin_string_list(span),
                span,
            ),
            "io.readDirPath" => {
                builtin_fn(vec![builtin_path(span)], builtin_string_list(span), span)
            }
            "io.readDirEntryPaths" => {
                builtin_fn(vec![builtin_path(span)], builtin_path_list(span), span)
            }
            "io.writeFile" | "io.appendFile" => builtin_fn(
                vec![
                    builtin_ty(TyKind::String, span),
                    builtin_ty(TyKind::String, span),
                ],
                builtin_ty(TyKind::Unit, span),
                span,
            ),
            "io.writeFilePath" => builtin_fn(
                vec![builtin_path(span), builtin_ty(TyKind::String, span)],
                builtin_ty(TyKind::Unit, span),
                span,
            ),
            "io.appendFilePath" => builtin_fn(
                vec![builtin_path(span), builtin_ty(TyKind::String, span)],
                builtin_ty(TyKind::Unit, span),
                span,
            ),
            "io.writeFileBytesPath" => builtin_fn(
                vec![builtin_path(span), builtin_bytes(span)],
                builtin_ty(TyKind::Unit, span),
                span,
            ),
            "io.appendFileBytesPath" => builtin_fn(
                vec![builtin_path(span), builtin_bytes(span)],
                builtin_ty(TyKind::Unit, span),
                span,
            ),
            "io.createDirAll" | "io.removeDirAll" | "io.pathExists" | "io.isDir" | "io.isFile" => {
                let ret_ty = match name {
                    "io.createDirAll" | "io.removeDirAll" => builtin_ty(TyKind::Unit, span),
                    _ => builtin_ty(TyKind::Bool, span),
                };
                builtin_fn(vec![builtin_ty(TyKind::String, span)], ret_ty, span)
            }
            "io.createDirAllPath" => builtin_fn(
                vec![builtin_path(span)],
                builtin_ty(TyKind::Unit, span),
                span,
            ),
            "io.removeDirAllPath" => builtin_fn(
                vec![builtin_path(span)],
                builtin_ty(TyKind::Unit, span),
                span,
            ),
            "io.pathExistsPath" => builtin_fn(
                vec![builtin_path(span)],
                builtin_ty(TyKind::Bool, span),
                span,
            ),
            "io.readFileBytesPath" => {
                builtin_fn(vec![builtin_path(span)], builtin_bytes(span), span)
            }
            "io.isDirPath" => builtin_fn(
                vec![builtin_path(span)],
                builtin_ty(TyKind::Bool, span),
                span,
            ),
            "io.isFilePath" => builtin_fn(
                vec![builtin_path(span)],
                builtin_ty(TyKind::Bool, span),
                span,
            ),
            "io.setEnv" => builtin_fn(
                vec![
                    builtin_ty(TyKind::String, span),
                    builtin_ty(TyKind::String, span),
                ],
                builtin_ty(TyKind::Unit, span),
                span,
            ),
            "io.unsetEnv" => builtin_fn(
                vec![builtin_ty(TyKind::String, span)],
                builtin_ty(TyKind::Unit, span),
                span,
            ),
            "io.getEnv" => builtin_fn(
                vec![builtin_ty(TyKind::String, span)],
                builtin_string_option(span),
                span,
            ),
            "io.env" => builtin_fn(Vec::new(), builtin_ty(TyKind::Record(vec![]), span), span),
            "io.sleep" => builtin_fn(
                vec![builtin_ty(TyKind::Int, span)],
                builtin_ty(TyKind::Unit, span),
                span,
            ),
            "io.which" => builtin_fn(
                vec![builtin_ty(TyKind::String, span)],
                builtin_string_option(span),
                span,
            ),
            "io.currentDir" | "io.currentSystem" => {
                builtin_fn(Vec::new(), builtin_ty(TyKind::String, span), span)
            }
            "io.currentDirPath" => builtin_fn(Vec::new(), builtin_path(span), span),
            "io.homeDirPath" => builtin_fn(Vec::new(), builtin_path_option(span), span),
            "io.command" => builtin_fn(
                vec![builtin_ty(TyKind::String, span), builtin_string_list(span)],
                builtin_command(span),
                span,
            ),
            "io.commandWith" => builtin_fn(
                vec![builtin_exec_with_options(span)],
                builtin_command(span),
                span,
            ),
            "io.commandWithRedirects" => builtin_fn(
                vec![
                    builtin_command(span),
                    builtin_list(builtin_redirect(span), span),
                ],
                builtin_command(span),
                span,
            ),
            "io.execCommand" => builtin_fn(
                vec![builtin_command(span)],
                builtin_process_result(span),
                span,
            ),
            "io.execCommandLines" => builtin_fn(
                vec![builtin_command(span)],
                builtin_list(builtin_ty(TyKind::String, span), span),
                span,
            ),
            "io.execCommandStreaming" => builtin_fn(
                vec![
                    builtin_command(span),
                    builtin_fn(
                        vec![builtin_ty(TyKind::String, span)],
                        builtin_ty(TyKind::Unit, span),
                        span,
                    ),
                ],
                builtin_process_result(span),
                span,
            ),
            "io.execPipelineStreaming" => builtin_fn(
                vec![
                    builtin_pipeline(span),
                    builtin_fn(
                        vec![builtin_ty(TyKind::String, span)],
                        builtin_ty(TyKind::Unit, span),
                        span,
                    ),
                ],
                builtin_process_result(span),
                span,
            ),
            "io.execCommandStreamingWithTimeout" => builtin_fn(
                vec![
                    builtin_command(span),
                    builtin_fn(
                        vec![builtin_ty(TyKind::String, span)],
                        builtin_ty(TyKind::Unit, span),
                        span,
                    ),
                    builtin_ty(TyKind::Int, span),
                ],
                builtin_option(builtin_process_result(span), span),
                span,
            ),
            "io.execPipelineStreamingWithTimeout" => builtin_fn(
                vec![
                    builtin_pipeline(span),
                    builtin_fn(
                        vec![builtin_ty(TyKind::String, span)],
                        builtin_ty(TyKind::Unit, span),
                        span,
                    ),
                    builtin_ty(TyKind::Int, span),
                ],
                builtin_option(builtin_process_result(span), span),
                span,
            ),
            "io.readFileLines" => builtin_fn(
                vec![
                    builtin_ty(TyKind::String, span),
                    builtin_fn(
                        vec![builtin_ty(TyKind::String, span)],
                        builtin_ty(TyKind::Unit, span),
                        span,
                    ),
                ],
                builtin_ty(TyKind::Unit, span),
                span,
            ),
            // io.readFileLinesPath is implemented by the HIR evaluator, not by the
            // std registry: dispatch is crates/n3v3-eval/src/eval.rs:1724-1729 ->
            // builtin_read_file_lines_path, which requires a Path value. The
            // n3v3-std entry is only a name/arity registration stub whose
            // "evaluator-owned" error is a fallback that dispatch never reaches.
            // Open gap: only the String variant has an E2E test.
            "io.readFileLinesPath" => builtin_fn(
                vec![
                    builtin_path(span),
                    builtin_fn(
                        vec![builtin_ty(TyKind::String, span)],
                        builtin_ty(TyKind::Unit, span),
                        span,
                    ),
                ],
                builtin_ty(TyKind::Unit, span),
                span,
            ),
            "io.atomicWrite" => builtin_fn(
                vec![
                    builtin_ty(TyKind::String, span),
                    builtin_ty(TyKind::String, span),
                ],
                builtin_ty(TyKind::Unit, span),
                span,
            ),
            "io.atomicWritePath" => builtin_fn(
                vec![builtin_path(span), builtin_ty(TyKind::String, span)],
                builtin_ty(TyKind::Unit, span),
                span,
            ),
            "io.atomicWriteAll" => builtin_fn(
                vec![builtin_list(
                    builtin_ty(
                        TyKind::Record(vec![
                            ("path".to_string(), builtin_ty(TyKind::String, span)),
                            ("content".to_string(), builtin_ty(TyKind::String, span)),
                        ]),
                        span,
                    ),
                    span,
                )],
                builtin_ty(TyKind::Unit, span),
                span,
            ),
            "io.copy" | "io.move" => builtin_fn(
                vec![
                    builtin_ty(TyKind::String, span),
                    builtin_ty(TyKind::String, span),
                ],
                builtin_ty(TyKind::Unit, span),
                span,
            ),
            "io.copyPath" | "io.movePath" => builtin_fn(
                vec![builtin_path(span), builtin_path(span)],
                builtin_ty(TyKind::Unit, span),
                span,
            ),
            "io.pipeline" => builtin_fn(
                vec![builtin_list(builtin_command(span), span)],
                builtin_pipeline(span),
                span,
            ),
            "io.pipelineWithRedirects" => builtin_fn(
                vec![
                    builtin_pipeline(span),
                    builtin_list(builtin_redirect(span), span),
                ],
                builtin_pipeline(span),
                span,
            ),
            "io.execPipeline" => builtin_fn(
                vec![builtin_pipeline(span)],
                builtin_process_result(span),
                span,
            ),
            "io.redirectStdoutPath" => {
                builtin_fn(vec![builtin_path(span)], builtin_redirect(span), span)
            }
            "io.redirectStderrPath" => {
                builtin_fn(vec![builtin_path(span)], builtin_redirect(span), span)
            }
            "io.redirectStdinPath" => {
                builtin_fn(vec![builtin_path(span)], builtin_redirect(span), span)
            }
            "io.taskCommand" => builtin_fn(
                vec![builtin_command(span)],
                builtin_task(builtin_process_result(span), span),
                span,
            ),
            "io.taskPipeline" => builtin_fn(
                vec![builtin_pipeline(span)],
                builtin_task(builtin_process_result(span), span),
                span,
            ),
            "io.eventMap" => {
                let a = builtin_param(0, "a", span);
                let b = builtin_param(1, "b", span);
                builtin_forall(
                    Vec::from(["a", "b"]),
                    builtin_fn(
                        vec![
                            builtin_event(a.clone(), span),
                            builtin_fn(vec![a], b.clone(), span),
                        ],
                        builtin_event(b, span),
                        span,
                    ),
                    span,
                )
            }
            "io.read" => builtin_fn(
                vec![builtin_ty(TyKind::String, span)],
                builtin_ty(TyKind::String, span),
                span,
            ),
            "io.write" => builtin_fn(
                vec![
                    builtin_ty(TyKind::String, span),
                    builtin_ty(TyKind::String, span),
                ],
                builtin_ty(TyKind::Unit, span),
                span,
            ),
            "io.run" => builtin_fn(
                vec![builtin_command(span)],
                builtin_process_result(span),
                span,
            ),
            "io.shell" => builtin_fn(
                vec![builtin_ty(TyKind::String, span)],
                builtin_process_result(span),
                span,
            ),
            "io.lines" => builtin_fn(
                vec![builtin_ty(TyKind::String, span)],
                builtin_string_list(span),
                span,
            ),
            "io.readPassword" => builtin_fn(
                vec![builtin_ty(TyKind::String, span)],
                builtin_ty(TyKind::String, span),
                span,
            ),
            "io.input" => builtin_fn(
                vec![builtin_ty(TyKind::String, span)],
                builtin_ty(TyKind::String, span),
                span,
            ),
            "io.glob" => builtin_fn(
                vec![builtin_ty(TyKind::String, span)],
                builtin_list(builtin_path(span), span),
                span,
            ),
            "io.onSignal" => builtin_fn(
                vec![
                    builtin_ty(TyKind::String, span),
                    builtin_fn(Vec::new(), builtin_ty(TyKind::Unit, span), span),
                ],
                builtin_ty(TyKind::Unit, span),
                span,
            ),
            "io.defer" => builtin_fn(
                vec![builtin_fn(Vec::new(), builtin_ty(TyKind::Unit, span), span)],
                builtin_ty(TyKind::Unit, span),
                span,
            ),
            "io.retry" => builtin_fn(
                vec![
                    builtin_fn(Vec::new(), builtin_ty(TyKind::Bool, span), span),
                    builtin_ty(TyKind::Int, span),
                    builtin_ty(TyKind::Int, span),
                ],
                builtin_ty(TyKind::Bool, span),
                span,
            ),
            "io.ensure" => builtin_fn(
                vec![
                    builtin_fn(Vec::new(), builtin_ty(TyKind::Bool, span), span),
                    builtin_ty(TyKind::Int, span),
                    builtin_ty(TyKind::Int, span),
                ],
                builtin_ty(TyKind::Bool, span),
                span,
            ),
            "io.reactive" => {
                let a = builtin_param(0, "a", span);
                builtin_forall(
                    Vec::from(["a"]),
                    builtin_fn(
                        vec![builtin_event(a.clone(), span)],
                        builtin_live(a, span),
                        span,
                    ),
                    span,
                )
            }
            "io.liveNext" => {
                let a = builtin_param(0, "a", span);
                builtin_forall(
                    Vec::from(["a"]),
                    builtin_fn(vec![builtin_live(a.clone(), span)], a, span),
                    span,
                )
            }
            "io.liveCurrent" => {
                let a = builtin_param(0, "a", span);
                builtin_forall(
                    Vec::from(["a"]),
                    builtin_fn(
                        vec![builtin_live(a.clone(), span)],
                        builtin_option(a, span),
                        span,
                    ),
                    span,
                )
            }
            "io.liveCancel" => {
                let a = builtin_param(0, "a", span);
                builtin_forall(
                    Vec::from(["a"]),
                    builtin_fn(
                        vec![builtin_live(a, span)],
                        builtin_ty(TyKind::Unit, span),
                        span,
                    ),
                    span,
                )
            }
            "io.eventFilter" => {
                let a = builtin_param(0, "a", span);
                builtin_forall(
                    Vec::from(["a"]),
                    builtin_fn(
                        vec![
                            builtin_event(a.clone(), span),
                            builtin_fn(vec![a.clone()], builtin_ty(TyKind::Bool, span), span),
                        ],
                        builtin_event(a, span),
                        span,
                    ),
                    span,
                )
            }
            "io.watchFile" => builtin_fn(
                vec![builtin_ty(TyKind::String, span)],
                builtin_event(builtin_ty(TyKind::String, span), span),
                span,
            ),
            "io.every" => builtin_fn(
                vec![builtin_ty(TyKind::Int, span)],
                builtin_event(builtin_ty(TyKind::Int, span), span),
                span,
            ),
            "io.eventNext" => {
                let a = builtin_param(0, "a", span);
                builtin_forall(
                    Vec::from(["a"]),
                    builtin_fn(vec![builtin_event(a.clone(), span)], a, span),
                    span,
                )
            }
            "io.walk" => builtin_fn(
                vec![builtin_path(span)],
                builtin_list(builtin_path(span), span),
                span,
            ),
            "io.chown" => builtin_fn(
                vec![
                    builtin_path(span),
                    builtin_ty(TyKind::Int, span),
                    builtin_ty(TyKind::Int, span),
                ],
                builtin_ty(TyKind::Unit, span),
                span,
            ),
            "io.chmod" => builtin_fn(
                vec![builtin_path(span), builtin_ty(TyKind::Int, span)],
                builtin_ty(TyKind::Unit, span),
                span,
            ),
            "io.readlink" => builtin_fn(vec![builtin_path(span)], builtin_path(span), span),
            "io.symlink" => builtin_fn(
                vec![builtin_path(span), builtin_path(span)],
                builtin_ty(TyKind::Unit, span),
                span,
            ),
            "io.tempDir" => {
                let a = builtin_param(0, "a", span);
                builtin_forall(
                    Vec::from(["a"]),
                    builtin_fn(
                        vec![builtin_fn(vec![builtin_path(span)], a.clone(), span)],
                        a,
                        span,
                    ),
                    span,
                )
            }
            "io.args" => builtin_fn(
                Vec::new(),
                builtin_ty(
                    TyKind::Tuple(vec![
                        builtin_list(builtin_ty(TyKind::String, span), span),
                        builtin_record(Vec::new(), span),
                    ]),
                    span,
                ),
                span,
            ),
            "io.isTTY" => builtin_fn(
                vec![builtin_ty(TyKind::Int, span)],
                builtin_ty(TyKind::Bool, span),
                span,
            ),
            "io.terminalSize" => builtin_fn(
                Vec::new(),
                builtin_option(
                    builtin_record(
                        vec![
                            ("rows", builtin_ty(TyKind::Int, span)),
                            ("cols", builtin_ty(TyKind::Int, span)),
                        ],
                        span,
                    ),
                    span,
                ),
                span,
            ),
            "io.setRawMode" => builtin_fn(
                vec![
                    builtin_ty(TyKind::Int, span),
                    builtin_ty(TyKind::Bool, span),
                ],
                builtin_ty(TyKind::Unit, span),
                span,
            ),
            "io.resetTerminal" => builtin_fn(
                vec![builtin_ty(TyKind::Int, span)],
                builtin_ty(TyKind::Unit, span),
                span,
            ),
            "io.readKey" => builtin_fn(
                vec![builtin_ty(TyKind::Int, span)],
                builtin_ty(TyKind::Int, span),
                span,
            ),
            "io.jobs" => builtin_fn(Vec::new(), builtin_list(builtin_job(span), span), span),
            "io.waitAnyJob" => builtin_fn(Vec::new(), builtin_job_result(span), span),
            "io.spawnWithTimeout" => builtin_fn(
                vec![
                    builtin_task(builtin_process_result(span), span),
                    builtin_ty(TyKind::Int, span),
                ],
                builtin_ty(TyKind::Int, span),
                span,
            ),
            "io.spawn" => builtin_fn(
                vec![builtin_task(builtin_process_result(span), span)],
                builtin_ty(TyKind::Int, span),
                span,
            ),
            "io.poll" => builtin_fn(
                vec![builtin_ty(TyKind::Int, span)],
                builtin_option(builtin_process_result(span), span),
                span,
            ),
            "io.cancel" => builtin_fn(
                vec![builtin_ty(TyKind::Int, span)],
                builtin_ty(TyKind::Unit, span),
                span,
            ),
            "io.awaitTask" => builtin_fn(
                vec![builtin_task(builtin_process_result(span), span)],
                builtin_process_result(span),
                span,
            ),
            "io.awaitTasks" => builtin_fn(
                vec![builtin_list(
                    builtin_task(builtin_process_result(span), span),
                    span,
                )],
                builtin_list(builtin_process_result(span), span),
                span,
            ),
            "io.awaitAny" => builtin_fn(
                vec![builtin_list(
                    builtin_task(builtin_process_result(span), span),
                    span,
                )],
                builtin_process_result(span),
                span,
            ),
            "io.awaitTaskWithTimeout" => builtin_fn(
                vec![
                    builtin_task(builtin_process_result(span), span),
                    builtin_ty(TyKind::Int, span),
                ],
                builtin_option(builtin_process_result(span), span),
                span,
            ),
            "io.processSuccess" => builtin_fn(
                vec![builtin_process_result(span)],
                builtin_ty(TyKind::Bool, span),
                span,
            ),
            "io.processStdout" => builtin_fn(
                vec![builtin_process_result(span)],
                builtin_ty(TyKind::String, span),
                span,
            ),
            "io.processCode" => builtin_fn(
                vec![builtin_process_result(span)],
                builtin_ty(TyKind::Int, span),
                span,
            ),
            "io.processStderr" => builtin_fn(
                vec![builtin_process_result(span)],
                builtin_ty(TyKind::String, span),
                span,
            ),
            // === Stream<T> APIs (Phase 4) ===
            "io.streamList" => builtin_fn(
                vec![builtin_list(builtin_param(0, "T", span), span)],
                builtin_stream(builtin_param(0, "T", span), span),
                span,
            ),
            "io.streamLines" => builtin_fn(
                vec![builtin_ty(TyKind::String, span)],
                builtin_stream(builtin_string(span), span),
                span,
            ),
            "io.streamCommand" => builtin_fn(
                vec![builtin_command(span)],
                builtin_stream(builtin_string(span), span),
                span,
            ),
            "io.streamBytes" => builtin_fn(
                vec![builtin_ty(TyKind::String, span)],
                builtin_stream(builtin_bytes(span), span),
                span,
            ),
            "io.streamMap" => builtin_forall(
                vec!["A", "B"],
                builtin_fn(
                    vec![
                        builtin_stream(builtin_param(0, "A", span), span),
                        builtin_fn(
                            vec![builtin_param(0, "A", span)],
                            builtin_param(1, "B", span),
                            span,
                        ),
                    ],
                    builtin_stream(builtin_param(1, "B", span), span),
                    span,
                ),
                span,
            ),
            "io.streamFilter" => builtin_forall(
                vec!["T"],
                builtin_fn(
                    vec![
                        builtin_stream(builtin_param(0, "T", span), span),
                        builtin_fn(
                            vec![builtin_param(0, "T", span)],
                            builtin_ty(TyKind::Bool, span),
                            span,
                        ),
                    ],
                    builtin_stream(builtin_param(0, "T", span), span),
                    span,
                ),
                span,
            ),
            "io.streamTake" => builtin_fn(
                vec![
                    builtin_stream(builtin_param(0, "T", span), span),
                    builtin_ty(TyKind::Int, span),
                ],
                builtin_stream(builtin_param(0, "T", span), span),
                span,
            ),
            "io.streamDrop" => builtin_fn(
                vec![
                    builtin_stream(builtin_param(0, "T", span), span),
                    builtin_ty(TyKind::Int, span),
                ],
                builtin_stream(builtin_param(0, "T", span), span),
                span,
            ),
            "io.streamCollect" => builtin_fn(
                vec![builtin_stream(builtin_param(0, "T", span), span)],
                builtin_list(builtin_param(0, "T", span), span),
                span,
            ),
            "io.streamPipe" => builtin_fn(
                vec![
                    builtin_stream(builtin_string(span), span),
                    builtin_command(span),
                ],
                builtin_process_result(span),
                span,
            ),
            "io.streamForEach" => builtin_fn(
                vec![
                    builtin_stream(builtin_param(0, "T", span), span),
                    builtin_fn(
                        vec![builtin_param(0, "T", span)],
                        builtin_ty(TyKind::Unit, span),
                        span,
                    ),
                ],
                builtin_ty(TyKind::Unit, span),
                span,
            ),
            "io.streamFold" => builtin_forall(
                vec!["T", "A"],
                builtin_fn(
                    vec![
                        builtin_stream(builtin_param(0, "T", span), span),
                        builtin_param(1, "A", span),
                        builtin_fn(
                            vec![builtin_param(1, "A", span), builtin_param(0, "T", span)],
                            builtin_param(1, "A", span),
                            span,
                        ),
                    ],
                    builtin_param(1, "A", span),
                    span,
                ),
                span,
            ),
            "io.streamWithTimeout" => builtin_fn(
                vec![
                    builtin_stream(builtin_param(0, "T", span), span),
                    builtin_ty(TyKind::Int, span),
                ],
                builtin_stream(builtin_option(builtin_param(0, "T", span), span), span),
                span,
            ),
            "io.homeDir" => builtin_fn(Vec::new(), builtin_string_option(span), span),
            "fetch.url" | "fetch.path" => builtin_fn(
                vec![builtin_ty(TyKind::String, span)],
                builtin_fetch_result(span),
                span,
            ),
            "fetch.urlWithHash" | "fetch.pathWithHash" | "fetch.git" => builtin_fn(
                vec![
                    builtin_ty(TyKind::String, span),
                    builtin_ty(TyKind::String, span),
                ],
                builtin_fetch_result(span),
                span,
            ),
            "fetch.gitWithHash" => builtin_fn(
                vec![
                    builtin_ty(TyKind::String, span),
                    builtin_ty(TyKind::String, span),
                    builtin_ty(TyKind::String, span),
                ],
                builtin_fetch_result(span),
                span,
            ),
            "Map.empty" => {
                let k = builtin_param(0, "k", span);
                let v = builtin_param(1, "v", span);
                builtin_forall(Vec::from(["k", "v"]), builtin_map(k, v, span), span)
            }
            "Map.singleton" => {
                let k = builtin_param(0, "k", span);
                let v = builtin_param(1, "v", span);
                builtin_forall(
                    Vec::from(["k", "v"]),
                    builtin_fn(vec![k.clone(), v.clone()], builtin_map(k, v, span), span),
                    span,
                )
            }
            "Map.fromList" => {
                let k = builtin_param(0, "k", span);
                let v = builtin_param(1, "v", span);
                let pair = builtin_ty(TyKind::Tuple(vec![k.clone(), v.clone()]), span);
                builtin_forall(
                    Vec::from(["k", "v"]),
                    builtin_fn(
                        vec![builtin_list(pair, span)],
                        builtin_map(k, v, span),
                        span,
                    ),
                    span,
                )
            }
            "Map.get" => {
                let k = builtin_param(0, "k", span);
                let v = builtin_param(1, "v", span);
                builtin_forall(
                    Vec::from(["k", "v"]),
                    builtin_fn(
                        vec![k.clone(), builtin_map(k, v.clone(), span)],
                        builtin_option(v, span),
                        span,
                    ),
                    span,
                )
            }
            "Map.getWithDefault" => {
                let k = builtin_param(0, "k", span);
                let v = builtin_param(1, "v", span);
                builtin_forall(
                    Vec::from(["k", "v"]),
                    builtin_fn(
                        vec![k.clone(), v.clone(), builtin_map(k, v.clone(), span)],
                        v,
                        span,
                    ),
                    span,
                )
            }
            "Map.contains" => {
                let k = builtin_param(0, "k", span);
                let v = builtin_param(1, "v", span);
                builtin_forall(
                    Vec::from(["k", "v"]),
                    builtin_fn(
                        vec![k.clone(), builtin_map(k, v, span)],
                        builtin_ty(TyKind::Bool, span),
                        span,
                    ),
                    span,
                )
            }
            "Map.size" | "Map.isEmpty" => {
                let k = builtin_param(0, "k", span);
                let v = builtin_param(1, "v", span);
                let ret_ty = match name {
                    "Map.size" => builtin_ty(TyKind::Int, span),
                    _ => builtin_ty(TyKind::Bool, span),
                };
                builtin_forall(
                    Vec::from(["k", "v"]),
                    builtin_fn(vec![builtin_map(k, v, span)], ret_ty, span),
                    span,
                )
            }
            "Map.values" => {
                let k = builtin_param(0, "k", span);
                let v = builtin_param(1, "v", span);
                builtin_forall(
                    Vec::from(["k", "v"]),
                    builtin_fn(
                        vec![builtin_map(k, v.clone(), span)],
                        builtin_list(v, span),
                        span,
                    ),
                    span,
                )
            }
            "Map.insert" => {
                let k = builtin_param(0, "k", span);
                let v = builtin_param(1, "v", span);
                builtin_forall(
                    Vec::from(["k", "v"]),
                    builtin_fn(
                        vec![
                            k.clone(),
                            v.clone(),
                            builtin_map(k.clone(), v.clone(), span),
                        ],
                        builtin_map(k, v, span),
                        span,
                    ),
                    span,
                )
            }
            "Map.remove" => {
                let k = builtin_param(0, "k", span);
                let v = builtin_param(1, "v", span);
                builtin_forall(
                    Vec::from(["k", "v"]),
                    builtin_fn(
                        vec![k.clone(), builtin_map(k.clone(), v.clone(), span)],
                        builtin_map(k, v, span),
                        span,
                    ),
                    span,
                )
            }
            "Map.union" | "Map.intersection" | "Map.difference" => {
                let k = builtin_param(0, "k", span);
                let v = builtin_param(1, "v", span);
                let map_kv = builtin_map(k, v, span);
                builtin_forall(
                    Vec::from(["k", "v"]),
                    builtin_fn(vec![map_kv.clone(), map_kv.clone()], map_kv, span),
                    span,
                )
            }
            "Set.empty" => {
                let a = builtin_param(0, "a", span);
                builtin_forall(Vec::from(["a"]), builtin_set(a, span), span)
            }
            "Set.singleton" => {
                let a = builtin_param(0, "a", span);
                builtin_forall(
                    Vec::from(["a"]),
                    builtin_fn(vec![a.clone()], builtin_set(a, span), span),
                    span,
                )
            }
            "Set.fromList" => {
                let a = builtin_param(0, "a", span);
                builtin_forall(
                    Vec::from(["a"]),
                    builtin_fn(
                        vec![builtin_list(a.clone(), span)],
                        builtin_set(a, span),
                        span,
                    ),
                    span,
                )
            }
            "Set.contains" => {
                let a = builtin_param(0, "a", span);
                builtin_forall(
                    Vec::from(["a"]),
                    builtin_fn(
                        vec![a.clone(), builtin_set(a, span)],
                        builtin_ty(TyKind::Bool, span),
                        span,
                    ),
                    span,
                )
            }
            "Set.size" | "Set.isEmpty" => {
                let a = builtin_param(0, "a", span);
                let ret_ty = match name {
                    "Set.size" => builtin_ty(TyKind::Int, span),
                    _ => builtin_ty(TyKind::Bool, span),
                };
                builtin_forall(
                    Vec::from(["a"]),
                    builtin_fn(vec![builtin_set(a, span)], ret_ty, span),
                    span,
                )
            }
            "Set.insert" | "Set.remove" => {
                let a = builtin_param(0, "a", span);
                builtin_forall(
                    Vec::from(["a"]),
                    builtin_fn(
                        vec![a.clone(), builtin_set(a.clone(), span)],
                        builtin_set(a, span),
                        span,
                    ),
                    span,
                )
            }
            "Set.union" | "Set.intersection" | "Set.difference" | "Set.symmetricDifference" => {
                let a = builtin_param(0, "a", span);
                let set_a = builtin_set(a, span);
                builtin_forall(
                    Vec::from(["a"]),
                    builtin_fn(vec![set_a.clone(), set_a.clone()], set_a, span),
                    span,
                )
            }
            "Set.isSubset" | "Set.isSuperset" | "Set.isDisjoint" => {
                let a = builtin_param(0, "a", span);
                let set_a = builtin_set(a, span);
                builtin_forall(
                    Vec::from(["a"]),
                    builtin_fn(
                        vec![set_a.clone(), set_a],
                        builtin_ty(TyKind::Bool, span),
                        span,
                    ),
                    span,
                )
            }
            _ if name.contains('.') => return Some(self.fresh_var()),
            _ => return None,
        };

        Some(instantiate(&polymorphic, &mut || self.fresh_var()))
    }
}
