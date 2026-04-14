# typeshaper — Claude 上下文文件

## 项目目标

用户希望在 Rust 中实现类似 TypeScript 工具类型（Omit、Pick 等）的功能，同时要求：

- **优雅书写**：不只是实现功能，要有优雅的表达方式
- **表达式语法**：用类代数运算符统一入口，而非多个独立宏
- **零前置条件**：除一次性标注源结构体外，不需要任何元数据填充
- **可组合**：生成的类型可以继续作为后续操作的源类型

## 整体架构

```
sculpt/ ← 主 crate（lib），包名 typeshaper
├── src/
│ ├── lib.rs ← 重导出所有公开 API
│ └── transform.rs ← TypeshaperInto<T> 和 TypeshaperExt trait
└── typeshaper-macros/ ← 过程宏子 crate（包名 typeshaper-macros）
 └── src/
 ├── lib.rs ← 注册宏入口：typeshaper 属性宏 + typex! 函数宏
 ├── parse.rs ← ShapeInput / ShapeExpr：解析宏输入
 ├── expand.rs ← 代码生成逻辑，每种操作一个函数
 └── state.rs ← 全局类型注册表（编译期 HashMap）
```

## 公开 API

### 属性宏 `#[typeshaper]`

标注源结构体，将其字段信息注册到编译期全局注册表，结构体本身原样保留。

```rust
#[typeshaper]
#[derive(Debug, Clone)]
pub struct User {
 pub id: u64,
 pub name: String,
 pub age: u8,
}
```

### 函数宏 `typex!(Target = Expr)`

类型代数表达式，统一入口：

| 语法 | 操作 | 生成的 impl |
|-------------------|----------|----------------------------------|
| `T` | Rebuild | `TypeshaperInto<Target> for T` |
| `T - [f1, f2]` | Omit | `TypeshaperInto<Target> for T` |
| `T & [f1, f2]` | Pick | `TypeshaperInto<Target> for T` |
| `A + B` | Merge | `From<(A, B)> for Target` |
| `T?` | Partial | `From<T> for Target` |
| `T!` | Required | `TryFrom<T> for Target` |
| `A % B` | Diff | `TypeshaperInto<Target> for A` |

生成的类型自动注册，可继续用作后续 `typex!()` 的源类型。

### 扩展方法 `.project()`

`TypeshaperExt` trait 提供的便捷方法，类型由绑定推断：

```rust
let public: UserPublic = user.project();
```

## 关键设计决策

### 编译期全局注册表

过程宏之间无法通过参数传递信息，用 `once_cell::sync::Lazy<Mutex<HashMap>>` 实现编译期共享状态。
`#[typeshaper]` 写入注册表，`typex!()` 从注册表读取字段定义。

**文件**：`typeshaper-macros/src/state.rs`

### `FieldDef` 的 `unwrapped_ty`

`Partial`（`T?`）将所有字段包裹为 `Option<T>`，同时在 `FieldDef::unwrapped_ty` 中保存原始类型字符串。
`Required`（`T!`）读取此字段恢复原始类型，并校验所有字段确实来自 `Partial`。

### 操作符方向统一

`Partial` 和 `Required` 都使用后缀操作符（`T?` / `T!`），方向一致。

### 错误传播

`expand.rs` 中各操作函数返回 `Result<TokenStream, TokenStream>`，使用 `?` 传播编译错误，
最终在 `expand_shape` 中统一解包。

### `extern crate self as typeshaper`

主 crate 用此声明让过程宏生成代码中的 `::typeshaper::TypeshaperInto` 路径在测试和最终用户的 crate 中均可解析。


## 文件结构（完整）

```
sculpt/
├── Cargo.toml name = "typeshaper"
├── CLAUDE.md 本文件
├── README.md
├── src/
│ ├── lib.rs
│ └── transform.rs
├── tests/
│ └── usage.rs 集成测试，覆盖全部 6 种操作
└── typeshaper-macros/ 目录名，包名 typeshaper-macros
 ├── Cargo.toml name = "typeshaper-macros", proc-macro = true
 └── src/
 ├── lib.rs
 ├── parse.rs
 ├── expand.rs
 └── state.rs
```

## 当前状态（v0.3.0）

- [x] Rebuild（`T`）
- [x] Omit（`T - [fields]`）
- [x] Pick（`T & [fields]`）
- [x] Merge（`A + B`）
- [x] Partial（`T?`）
- [x] Required（`T!`）
- [x] Diff（`A % B`）
- [x] 全部集成测试通过（23 个测试）

## 未来方向（用户提及，尚未实现）

- **零拷贝 View**：生成引用字段的视图结构体（类似 slice），与 `.project()`（owned）对应，
 暂定命名 `.view()`，仅支持 Omit / Pick / Diff。设计未定。
- **更多工具类型**：`Readonly`（字段不可变）、`Record<K, V>`、`Exclude`、`NonNullable` 等。
