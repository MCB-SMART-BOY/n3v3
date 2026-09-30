# n3v3 Formal Verification

用 [Lean 4](https://lean-lang.org/) 对 n3v3 语言核心语义的形式化验证。

## 目录结构

```
formal/
├── n3v3.lean                   — 主模块（导入所有子模块）
├── n3v3/
│   ├── Spec/
│   │   ├── Syntax.lean         — 核心语法（类型、值、表达式、模式、BinOp、Effect）
│   │   ├── Typing.lean v4      — 类型检查规则（12 条 BinOp 全覆盖）
│   │   ├── Eval.lean v2        — 大步操作语义（27 条规则，含 matchOn_fallthrough）
│   │   └── Effects.lean v4.3   — 效果求值语义（34 条规则，含 5 stream Phase C）
│   ├── Proofs/
│   │   ├── Values.lean v4      — ValueTyping + EnvMatches（谓词参数化）
│   │   ├── Context.lean v4     — env_matches_lookup 引理
│   │   ├── Safety.lean v18     — 类型安全（13 个已验证 case）
│   │   ├── SafetyLemmas.lean   — 已验证的模式匹配引理（5 个）
│   │   └── EffectProperties.lean — 效果规则属性（cancel/pure/streamCollect）
│   ├── Verify/
│   │   ├── Path.lean           — 路径遍历安全（M-1）
│   │   ├── Environ.lean        — 环境变量过滤（M-4）
│   │   ├── Limits.lean         — 缓冲区大小限制（H-1, H-2）
│   │   └── Stream.lean         — 流式语义安全
│   ├── Refinement/
│   │   ├── Types.lean          — Rust-Lean 精化关系
│   │   ├── Path.lean           — 路径解析精化（M-1）
│   │   ├── Environ.lean        — 环境过滤精化（M-4）
│   │   ├── Limits.lean         — 大小限制精化（H-1, H-2）
│   │   └── Stream.lean         — 流式精化
│   └── Tests/
│       └── Eval.lean           — 可执行规范求值器（11 个自测试）
├── lakefile.lean               — Lean 项目配置
├── lake-manifest.json          — 依赖清单
└── README.md                   — 本文件
```

## 构建

```bash
# 安装 Lean 4
curl https://raw.githubusercontent.com/leanprover/elan/master/elan-init.sh -sSf | sh

# 构建（21 个 Lean 模块；lake build 报告 22 个构建目标，全部通过）
cd formal
lake build
```

## 当前状态（v4）

| 层次 | 模块数 | 状态 | 说明 |
|------|--------|------|------|
| Spec（规范） | 4 | ✅ | Syntax, Typing v4, Eval v2, Effects v4.3 (34 rules, +5 stream Phase C) |
| Proofs（证明） | 5 | ✅ | Values, Context, Safety v18, SafetyLemmas, EffectProperties |
| Verify（安全） | 4 | ✅ | Path (M-1), Environ (M-4), Limits (H-1/H-2), Stream |
| Refinement（精化） | 5 | ✅ | Types, Path, Environ, Limits, Stream |
| Tests（测试） | 1 | ✅ | 可执行规范求值器 |
| 顶层（顶层模块） | 2 | ✅ | n3v3.lean, lakefile.lean |
| **合计** | **21** | ✅ | 21 个 `.lean` 文件（= `scripts/counts.sh` 的 `lean_modules`）；`lake build` 报告 22 个构建目标并全部通过 |

## 形式化范围

### 已验证（机器检查）
- 基本类型：Int, Bool, String, Unit, Float, Char, **Bytes**
- 函数类型：Fn(param)(ret)(effect)
- λ 演算：变量、抽象、应用（lam 情况）
- let 绑定
- 二元运算：**12/12 BinOp 全覆盖**（+ - * / % == && || |>）
- 管道 |> （lam 情况）
- 模式匹配：通配符、字面量（int/bool）、布尔全覆盖（两臂 + fallthrough）
- 效果系统：**34 条 EffectEval 规则（v4.3: +5 stream Phase C: streamCollect/streamPipe/streamForEach/streamFold/streamWithTimeout）**（阻塞、流式、超时、Bytes I/O、stream、cancel/awaitAny）
- 类型安全定理：type_safety（空上下文，13 个已验证 case）
- 安全审计 5 项（含 M-2 kill 统一）全部 Lean 验证：H-1, H-2, M-1, M-4

### 已知限制
- app/pipe 非 lam 情况：需良基递归（已文档化）
- 通用 matchOn：受 Lean 4.29 mutual inductive 限制
- 无构造子模式（Some/None, Ok/Err）：Pattern 类型简化

## 与 Rust 的对照

| Lean 模块 | Rust 对应 | 精化桥 |
|-----------|----------|--------|
| Spec/Syntax.lean | crates/n3v3-hir/ | — |
| Spec/Typing.lean | crates/n3v3-typeck/ | — |
| Spec/Eval.lean | crates/n3v3-eval/ | — |
| Spec/Effects.lean v4.3 (34 rules, +5 stream Phase C) | crates/n3v3-std/src/io/ | — |
| Verify/Path.lean | resolve_redirect_path | Refinement/Path.lean |
| Verify/Environ.lean | configured_process_command | Refinement/Environ.lean |
| Verify/Limits.lean | MAX_*_BYTES checks | Refinement/Limits.lean |
