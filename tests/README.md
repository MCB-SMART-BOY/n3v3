<div align="center">

<img src="../assets/logo.svg" width="120" alt="n3v3 logo">

<h1>Integration Tests</h1>

<p><em>集成测试</em></p>

<p>
  <strong><a href="../README.md">Home</a></strong> ·
  <strong><a href="../docs/">Docs</a></strong>
</p>

</div>

---

This directory contains integration tests for the n3v3 language.
Some files validate a single compiler/runtime stage, while `end_to_end.rs`
is reserved for trustworthy smoke coverage of real frontend/runtime paths.
本目录包含 n3v3 语言的集成测试。
其中一部分测试单独验证某个编译器/运行时阶段，`end_to_end.rs`
只保留给真实前端与运行时路径的可信烟雾测试。

## 测试结构 / Test Structure

### [parser.rs](parser.rs)
**解析器测试 / Parser Tests**

测试完整的解析流程:Lexer → Parser → AST

Tests the complete parsing pipeline: Lexer → Parser → AST

涵盖内容 / Coverage:
- ✅ 基础函数定义 / Basic function definitions
- ✅ 记录字面量 / Record literals
- ✅ Trait 关联类型 / Trait associated types
- ✅ Impl 关联类型 / Impl associated types
- ✅ 模式匹配 / Pattern matching
- ✅ 泛型 / Generics
- ✅ 模块导入 / Module imports
- ✅ 管道操作符 / Pipe operator
- ✅ Derivation 语法 / Derivation syntax
- ✅ 错误恢复 / Error recovery

### [module_loading.rs](module_loading.rs)
**模块加载测试 / Module Loading Tests**

测试模块系统,包括循环依赖检测。

Tests the module system including circular dependency detection.

涵盖内容 / Coverage:
- ✅ 简单模块加载 / Simple module loading
- ✅ 嵌套模块 / Nested modules
- ✅ 循环依赖检测 / Circular dependency detection
- ✅ 错误消息格式 / Error message formatting
- ✅ self 导入 / self imports
- ✅ super 导入 / super imports
- ✅ crate 导入 / crate imports
- ✅ 模块未找到 / Module not found
- ✅ 菱形依赖 / Diamond dependencies

### [eval.rs](eval.rs)
**求值和 TCO 测试 / Evaluation and TCO Tests**

测试求值器,包括尾调用优化。

Tests the evaluator including tail call optimization.

涵盖内容 / Coverage:
- ✅ 基础算术 / Basic arithmetic
- ✅ 函数应用 / Function application
- ✅ 高阶函数 / Higher-order functions
- ✅ 闭包捕获 / Closure capture
- ✅ 模式匹配(列表/Option) / Pattern matching (lists/Option)
- ✅ 尾递归阶乘 / Tail-recursive factorial
- ✅ 尾递归求和 / Tail-recursive sum
- ✅ 互递归 / Mutual recursion
- ✅ 记录操作 / Record operations
- ✅ If 表达式 / If expressions
- ✅ 惰性求值 / Lazy evaluation
- ✅ 字符串操作 / String operations
- ✅ 布尔运算 / Boolean operations
- ✅ 比较运算 / Comparison operations
- ✅ 管道操作符求值 / Pipe operator evaluation
- ✅ 错误处理 / Error handling
- ✅ Match 表达式中的 TCO / TCO in match expressions
- ✅ If 表达式中的 TCO / TCO in if expressions

### [typeck.rs](typeck.rs)
**类型检查测试 / Type Checking Tests**

测试 Hindley-Milner 类型推导和 Trait 约束。

Tests Hindley-Milner type inference and trait constraints.

涵盖内容 / Coverage:
- ✅ 简单类型推导 / Simple type inference
- ✅ 函数类型推导 / Function type inference
- ✅ 多态函数 / Polymorphic functions
- ✅ 列表类型推导 / List type inference
- ✅ 记录类型推导 / Record type inference
- ✅ 高阶函数类型 / Higher-order function types
- ✅ Trait 约束 / Trait constraints
- ✅ 关联类型 / Associated types
- ✅ 类型错误(类型不匹配) / Type errors (mismatch)
- ✅ 类型错误(参数数量不匹配) / Type errors (arity)
- ✅ 递归函数类型 / Recursive function types
- ✅ 互递归类型 / Mutual recursion types
- ✅ Option 类型 / Option type
- ✅ Result 类型 / Result type
- ✅ 泛型函数实例化 / Generic function instantiation
- ✅ 类型注解一致性 / Type annotation consistency
- ✅ 闭包类型推导 / Closure type inference
- ✅ 嵌套泛型 / Nested generics
- ✅ If 分支类型检查 / If branch type checking
- ✅ Match 分支类型检查 / Match arm type checking
- ✅ 类型统一 / Unification
- ✅ Occurs check

### [end_to_end.rs](end_to_end.rs)
**端到端测试 / End-to-End Tests**

测试真实入口的烟雾路径: Frontend(parse → lower → typecheck) + HIR Runtime

Tests the trustworthy smoke path: Frontend(parse → lower → typecheck) + HIR
Runtime.

涵盖内容 / Coverage:
- ✅ 前端 parse 错误真实上报 / Real parser diagnostics through the frontend
- ✅ 前端 type 错误真实上报 / Real type diagnostics through the frontend
- ✅ Canonical HIR runtime smoke coverage for `list.sum`, record fields, recursion, pipelines, list and enum matching / canonical HIR runtime 的 `list.sum`、记录字段访问、递归、管道、列表匹配、枚举匹配等烟雾覆盖
- ✅ `lazy/force` 在前端与 HIR runtime 上的真实闭环回归

## 运行测试 / Running Tests

### 运行所有集成测试 / Run All Integration Tests
```bash
cargo test --tests
```

### 运行特定测试文件 / Run Specific Test File
```bash
cargo test --test parser
cargo test --test module_loading
cargo test --test frontend
cargo test --test hir
cargo test --test eval
cargo test --test typeck
cargo test --test end_to_end
```

### 运行单个测试 / Run Single Test
```bash
cargo test --test parser test_parse_fn
cargo test --test module_loading test_circular_dependency_detection
cargo test --test eval test_eval_recursive_factorial
```

### 显示测试输出 / Show Test Output
```bash
cargo test -- --show-output
```

### 运行测试并显示详细信息 / Run Tests with Verbose Output
```bash
cargo test -- --nocapture
```

## 测试原则 / Testing Principles

### 1. 完整性 / Completeness
每个测试应该验证完整的功能,而不仅仅是部分行为。

Each test should verify complete functionality, not just partial behavior.

### 2. 独立性 / Independence
测试应该互相独立,可以以任意顺序运行。

Tests should be independent and runnable in any order.

### 3. 可读性 / Readability
测试代码应该清晰,作为功能的文档和示例。

Test code should be clear, serving as documentation and examples.

### 4. 真实性 / Realism
测试应该使用真实的用例,而不是人为构造的例子。

Tests should use realistic use cases, not artificial examples.

## 测试覆盖 / Test Coverage

当前集成测试覆盖多个层次:

Current integration test coverage spans multiple layers:

- **解析器 / Parser**: `parser.rs`
- **模块加载 / Module Loading**: `module_loading.rs`
- **前端分析 / Frontend analysis**: `frontend.rs`
- **HIR 降级 / HIR lowering**: `hir.rs`
- **求值器 / Evaluator**: `eval.rs`
- **类型检查 / Type Checker**: `typeck.rs`, `module_typeck.rs`
- **端到端烟雾测试 / End-to-End smoke tests**: `end_to_end.rs`

精确测试数量会持续变化，以 `cargo test --tests` 的实际输出为准。
Exact test counts change over time; use `cargo test --tests` as the source of truth.

## 添加新测试 / Adding New Tests

添加新集成测试的步骤:

Steps to add new integration tests:

1. **选择合适的测试文件** / Choose appropriate test file
   - 解析相关 → `parser.rs`
   - 模块相关 → `module_loading.rs`
   - 前端分析 → `frontend.rs`
   - HIR 降级 → `hir.rs`
   - 求值相关 → `eval.rs`
   - 类型相关 → `typeck.rs` / `module_typeck.rs`
   - 真实烟雾流程 → `end_to_end.rs`

2. **编写测试函数** / Write test function
   ```rust
   #[test]
   fn test_parse_let() {
       let (file, diags) = n3v3_parser::parse("let x = 42;");
       assert!(diags.is_empty());
       assert_eq!(file.items.len(), 1);
   }
   ```

3. **运行并验证** / Run and verify
   ```bash
   cargo test --test parser test_parse_let
   ```

4. **更新本 README** / Update this README
   - 在相应部分添加新测试描述
   - 不要保留已经失真的 coverage 说法

## 已知问题 / Known Issues

某些能力仍未完全实现,因此测试里会显式标出当前缺口:

Some capabilities are still incomplete, so tests should name current gaps explicitly:

- ⚠️ 完整的 Trait 系统 / Complete trait system
- ⚠️ 代数数据类型 (ADTs) / Algebraic Data Types
- ⚠️ 记录模式匹配 / Record pattern matching
- ⚠️ `list.fold` type checking accepts `(init, func, list)` but evaluation reads `(init, list, func)`; do not treat it as end-to-end supported / `list.fold` 的类型检查参数顺序为 `(init, func, list)`，求值器按 `(init, list, func)` 读取；不要将其视为已闭环
- ⚠️ 某些内置函数 / Some built-in functions
- ⚠️ 完整的错误恢复 / Complete error recovery

不要新增“无论成功失败都算通过”的测试。
如果某项功能还没闭环，要么测试已经支持的子集，要么把缺口明确写成已知分叉哨兵。

Do not add tests that pass regardless of success or failure.
If a feature is incomplete, either test the supported subset or write the gap down as an explicit divergence sentinel.

## 贡献指南 / Contributing

添加测试时请遵循:

When adding tests, please follow:

1. **清晰的测试名称** / Clear test names
   - 使用描述性名称:`test_<feature>_<scenario>`
   - 例如:`test_tail_recursion_factorial`

2. **完整的注释** / Complete comments
   - 说明测试目的
   - 解释预期行为

3. **适当的断言** / Appropriate assertions
   - 使用具体断言而不是宽泛的检查
   - 验证错误消息内容

4. **真实示例** / Realistic examples
   - 使用实际代码模式
   - 避免过于简化的例子

---

**测试你的代码,让 n3v3 更健壮!** 🧪

**Test your code, make n3v3 more robust!** 🧪
