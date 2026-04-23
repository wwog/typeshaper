# typeshaper

`typeshaper` 让你用一行表达式从已有结构体派生新的结构体类型——省略字段、挑选字段、合并两个结构体、将所有字段置为可选，或恢复为必填。生成的类型自动获得转换 impl，可链式组合。

## 你有没有写过这样的代码

```rust
pub struct User {
    pub id: u64,
    pub name: String,
    pub email: String,
    pub password_hash: String,
    pub role: String,
    pub active: bool,
    pub created_at: i64,
}
```

然后 API 层要用，不能把 `password_hash` 暴露出去，于是你复制了一份：

```rust
pub struct UserPublic {
    pub id: u64,
    pub name: String,
    pub email: String,
    // password_hash 不要
    pub role: String,
    pub active: bool,
    pub created_at: i64,
}

impl From<User> for UserPublic {
    fn from(u: User) -> Self {
        Self {
            id: u.id,
            name: u.name,
            email: u.email,
            role: u.role,
            active: u.active,
            created_at: u.created_at,
        }
    }
}
```

然后搜索功能只需要 id 和 name，又复制一份：

```rust
pub struct UserSummary {
    pub id: u64,
    pub name: String,
}
// ... 又一个 From ...
```

然后增量更新接口要求所有字段可选，又复制一份：

```rust
pub struct UserPatch {
    pub id: Option<u64>,
    pub name: Option<String>,
    pub email: Option<String>,
    pub password_hash: Option<String>,
    // ...
}
// ... 又一个 From ...
```

`User` 加了一个字段，`UserPublic`、`UserPatch`、`UserSummary` 全部都要跟着改——改结构体、改 `From`、改可能遗漏的测试。

这只是 `User`。你还有 `Order`、`Product`、`Article`、`Comment`……

---

## 换一种写法

```toml
[dependencies]
typeshaper = "0.1"
```

```rust
use typeshaper::{TypeshaperExt, typeshaper, typex};

#[typeshaper]
#[derive(Debug, Clone)]
pub struct User {
    pub id: u64,
    pub name: String,
    pub email: String,
    pub password_hash: String,
    pub role: String,
    pub active: bool,
    pub created_at: i64,
}

// 去掉两个字段
typex!(#[derive(Debug, Clone)] pub UserPublic  = User - [password_hash, created_at]);

// 只保留两个字段
typex!(#[derive(Debug, Clone)] pub UserSummary = User & [id, name]);

// 所有字段变可选
typex!(#[derive(Debug, Clone)] pub UserPatch   = User?);
```

转换直接用：

```rust
let user: User = /* 从数据库来的 */;

let public:  UserPublic  = user.clone().project();  // 自动去掉 password_hash、created_at
let summary: UserSummary = user.clone().project();  // 只有 id 和 name
let patch    = UserPatch::from(user);               // 所有字段变 Option
```

`User` 加字段，三行 `typex!()` 不用改，新字段自动跟着走。

---

## 故事还没完：两个来源合并成一个

订单快照需要同时包含用户信息和地址信息：

```rust
#[typeshaper]
pub struct Address {
    pub street: String,
    pub city: String,
    pub country: String,
}

// 把 User 和 Address 合进一个新类型
typex!(#[derive(Debug, Clone)] pub OrderSnapshot = User + Address);

let snapshot = OrderSnapshot::from((user, address));
// 也可以通过元组直接调用 .project()：
let snapshot = (user, address).project::<OrderSnapshot>();
```

只需要"User 有而 Address 没有"的字段：

```rust
typex!(#[derive(Debug, Clone)] pub UserOnly = User % Address);  // Diff
```

---

## 表达式可以链式组合

```rust
// 先去掉 password_hash，再把剩余字段全部变可选
typex!(#[derive(Debug)] pub UserSafePatch = User - [password_hash]?);

// 去掉 password_hash 之后，只留摘要字段
typex!(#[derive(Debug)] pub UserSafeDto = User - [password_hash] & [id, name, email]);

// 括号控制结合方向：先 Partial 再 Required（等价于恢复非可选）
typex!(#[derive(Debug)] pub UserRestored = (User - [password_hash])?!);
```

---

## 增量更新的完整循环

```rust
// 可选版本用于更新接口
typex!(#[derive(Debug, Clone)] pub UserPatch    = User?);

// 验证通过后恢复为必填版本
typex!(#[derive(Debug, Clone)] pub UserVerified = UserPatch!);

// ---

let patch = UserPatch {
    name: Some("alice".into()),
    email: Some("new@example.com".into()),
    // 其他字段留 None，表示"不更新"
    ..Default::default()
};

// 如果所有字段都已填写，可以恢复为强类型
match UserVerified::try_from(patch) {
    Ok(verified) => { /* 提交 */ }
    Err(e)       => { /* 告知哪个字段缺失 */ }
}
```

---

## 跨 crate：模型定义在一个地方

领域层定义模型，API 层按需派生，不复制结构体：

```rust
// core-crate/src/lib.rs
#[typeshaper(export)]          // export 多生成一个伴生宏
#[derive(Debug, Clone)]
pub struct User { /* ... */ }
// 自动生成并导出：pub macro typeshaper_import_User!()
```

```rust
// api-crate/src/lib.rs
use core_crate::{User, typeshaper_import_User};

typeshaper_import_User!();  // 把 User 的字段元数据注册到本 crate

// 之后完全和本地 #[typeshaper] 类型一样用
typex!(#[derive(Debug, Clone)] pub UserPublic = User - [password_hash, created_at]);
typex!(#[derive(Debug, Clone)] pub UserPatch  = User?);
```

---

## 重导出并附加新属性

有时你需要给一个你没有编写、也无法修改的结构体添加属性。

典型场景是 FFI 绑定。以 [`napi-rs`](https://napi.rs/) 为例：它要求每个导出到 Node.js 的结构体都标注 `#[napi]`，而你的领域模型在 `core-crate` 里，FFI 层的 `napi-crate` 绝不能去改动它——给别人的包直接加属性既跨越了 crate 边界，也根本无法编译通过。

用 typeshaper，一个没有任何运算符的裸源表达式（`T`）会原样重建结构体，同时附加你指定的新属性：

```rust
// napi-crate/src/lib.rs
use core_crate::{User, typeshaper_import_User};

typeshaper_import_User!();

// 字段与 User 完全相同，无需手动复制任何字段
typex!(#[napi] pub UserNapi = User);

// 先去掉敏感字段，再经由 napi 导出
typex!(#[napi] pub UserPublicNapi = User - [password_hash]);
```

`User → UserNapi` 的转换 impl 自动生成，领域结构体保持干净，FFI 层自行持有注解。

同理，凡是不能写在源 crate 里的属性，都可以用这个方式附加：`#[repr(C)]`（C FFI）、`#[pyclass]`（PyO3）、第三方 crate 提供的自定义 `#[derive]` 等。

---

## 泛型支持

当源结构体带有类型参数时，在 `typex!()` 中必须**显式**声明——同时写在目标名称和源节点上。这是有意为之：隐式继承在多个类型参数来自不同源类型时会产生错误的结构体。

### 基本类型参数

```rust
#[typeshaper]
#[derive(Debug, Clone)]
pub struct Wrapper<T> {
    pub inner: T,
    pub label: String,
    pub count: usize,
}

// <T> 同时出现在目标名称和源节点上
typex!(#[derive(Debug, Clone)] pub WrapperNoLabel<T>  = Wrapper<T> - [label]);
typex!(#[derive(Debug, Clone)] pub WrapperPartial<T>  = Wrapper<T>?);
typex!(#[derive(Debug, Clone)] pub WrapperRequired<T> = WrapperPartial<T>!);

let w = Wrapper { inner: 42u32, label: "hi".into(), count: 3 };
let no_label: WrapperNoLabel<u32> = w.project();
```

### 多类型参数（Merge）

合并两个泛型类型时，目标声明所有参数，每个源节点使用自己的参数：

```rust
#[typeshaper]
pub struct Person<T> { pub name: T, pub age: u8 }

#[typeshaper]
pub struct Addr<U> { pub city: U, pub zip: String }

// T 来自 Person，U 来自 Addr，目标同时声明两者
typex!(#[derive(Debug)] pub PersonWithAddr<T, U> = Person<T> + Addr<U>);

let full = PersonWithAddr::from((person, addr));
```

### 内联 trait bound 与 where 子句

```rust
typex!(pub PrintableValue<T: std::fmt::Display + Clone> = Printable<T> - [note]);

typex!(pub ConstrainedData<T> where T: Clone + PartialEq = Constrained<T> - [meta]);
```

### 生命周期参数

```rust
#[typeshaper]
pub struct Borrowed<'a> { pub name: &'a str, pub value: u32 }

typex!(pub BorrowedName<'a> = Borrowed<'a> & [name]);
```

### 跨 crate 泛型类型

泛型元数据已编码在伴生宏中，导入时自动还原，用法与本 crate 完全相同：

```rust
// core-crate
#[typeshaper(export)]
pub struct GenericModel<T> { pub id: u64, pub payload: T, pub hidden: bool }
```

```rust
// app-crate
typeshaper_import_GenericModel!();

typex!(#[derive(Debug)] pub ModelPublic<T> = GenericModel<T> - [hidden]);
typex!(#[derive(Debug)] pub ModelDraft<T>  = GenericModel<T>?);
```

> **编译期守卫**：忘记写类型参数会在编译时报错：
> ```
> typex!(Bad = Wrapper - [label]);
> //          ^^^^^^^ error: type `Wrapper` has generic parameters;
> //                  declare them explicitly, e.g. `Target<T> = Wrapper<T>`
> ```

---

## 参考文档

### 安装

```toml
[dependencies]
typeshaper = "0.1"
```

### 前置标注：`#[typeshaper]`

在源结构体上添加一次，字段信息写入编译期注册表，结构体本身原样保留。
支持两种形式：

| 形式 | 作用 |
|------|------|
| `#[typeshaper]` | 本 crate 内使用 |
| `#[typeshaper(export)]` | 本 crate 内使用 + 生成 `typeshaper_import_T!()` 供其他 crate 调用 |

```rust
#[typeshaper]
#[derive(Debug, Clone, PartialEq)]
pub struct User {
    pub id: u64,
    pub name: String,
    pub age: u8,
    pub email: String,
}
```

`#[typeshaper]` 可以叠放在任意其他属性之上，不影响结构体的任何已有行为。

---

### 操作符速查

| 语法 | 名称 | 含义 | 生成的 impl |
|------|------|------|-------------|
| `T` | **Rebuild** | 原样复制所有字段；附加新属性 | `TypeshaperInto<Target> for T` |
| `T - [f1, f2]` | **Omit** | 移除列出的字段 | `TypeshaperInto<Target> for T` |
| `T & [f1, f2]` | **Pick** | 只保留列出的字段 | `TypeshaperInto<Target> for T` |
| `A + B` | **Merge** | 合并 A 和 B 的全部字段（不允许重名） | `From<(A, B)> for Target` + `TypeshaperInto<Target> for (A, B)` |
| `T?` | **Partial** | 所有字段变为 `Option<_>` | `From<T> for Target` |
| `T!` | **Required** | 还原 Partial 的 `Option<_>` | `TryFrom<T> for Target`（源无 `Option` 字段时为 `From<T>`） |
| `A % B` | **Diff** | A 有而 B 没有的字段（按字段名**和**类型同时匹配） | `TypeshaperInto<Target> for A` |

**组合规则**

操作符左结合，可加括号改变结合方向：

```rust
// User - [age] & [id, name]  等价于  (User - [age]) & [id, name]
typex!(Dto      = User - [age] & [id, name]);

// 括号让右侧先求值
typex!(Full     = User + (Badge - [label]));

// 后缀链式
typex!(Draft    = User - [password_hash]?);
typex!(Roundtrip = (User - [password_hash])?!);
```

---

### `typex!()` 语法

```
typex!( [#[attr...]]  [vis]  TargetName[<Params>] [where ...]  =  Expr );
```

- **属性**（可选）：写在 `TargetName` 前，原样附加到生成的结构体，支持叠放多个。`typex!()` 不会自动添加任何 `#[derive]`，全部由调用方声明。
- **可见性**（可选）：`pub`、`pub(crate)`、`pub(super)` 等。**省略时默认为私有**——生成的结构体仅在同一模块内可访问。需要在模块外使用的类型，必须显式写 `pub`。
- **TargetName**：生成的结构体名称，同时注册到编译期注册表，可继续作为后续 `typex!()` 的输入。
- **`<Params>`**（可选）：目标类型的显式泛型或生命周期参数——当 `Expr` 中任意源类型是泛型时必须填写。支持内联 bound（`T: Clone + Debug`）和单独 `where` 子句两种写法。
- **Expr**：类型代数表达式，见上表。涉及泛型源类型的节点必须携带匹配的类型参数：`Source<T>`、`Source<'a>` 等。

```rust
typex!(
    #[derive(Debug, Clone, PartialEq)]
    #[serde(rename_all = "camelCase")]
    pub UserPublicDto = User & [id, name, email]
);
```

---

### 转换方法

`TypeshaperExt` trait 为所有类型自动实现，通过类型推断选择目标：

```rust
let public: UserPublic = user.project();   // 等价于 user.typeshaper_into()
```

`Merge` 使用元组 `From` 或 `.project()`，`Partial` 使用 `From`，`Required` 使用 `TryFrom`：

```rust
let snapshot = OrderSnapshot::from((user, address));
let snapshot = (user, address).project::<OrderSnapshot>(); // 同样可用
let draft    = UserPatch::from(user);
let verified = UserVerified::try_from(draft)?;
```

---

### 跨 crate 详细用法

**导出端**

```rust
// core-crate/src/lib.rs
use typeshaper::typeshaper;

#[typeshaper(export)]
#[derive(Debug, Clone)]
pub struct User {
    pub id: u64,
    pub name: String,
    pub role: String,
    pub active: bool,
}
// 编译后自动导出：pub macro typeshaper_import_User!()
```

**导入端**

```rust
// app-crate/src/lib.rs
use typeshaper::typex;
use core_crate::{User, typeshaper_import_User};

typeshaper_import_User!();  // 仅一次，写在模块顶层

typex!(#[derive(Debug, Clone)] UserPublic = User - [role, active]);
typex!(#[derive(Debug, Clone)] UserPatch  = User?);
```

多个类型分别调用各自的伴生宏，跨 crate Merge / Diff 同样支持：

```rust
use core_crate::{Address, typeshaper_import_Address};

typeshaper_import_User!();
typeshaper_import_Address!();

typex!(#[derive(Debug, Clone)] OrderSnapshot = User + Address);
typex!(#[derive(Debug, Clone)] UserOnly      = User % Address);
```

| | 本 crate 内 | 跨 crate |
|---|---|---|
| 源类型标注 | `#[typeshaper]` | `#[typeshaper(export)]` |
| 调用方前置步骤 | 无 | `typeshaper_import_T!()` |
| `typex!()` 语法 | 完全相同 | 完全相同 |

---

### 已支持的操作

- [x] Omit — `T - [fields]`
- [x] Pick — `T & [fields]`
- [x] Merge — `A + B`
- [x] Partial — `T?`
- [x] Required — `T!`
- [x] Diff — `A % B`
- [x] 表达式组合与链式操作
- [x] 属性透传
- [x] 跨 crate 导出 / 导入
- [x] 泛型、生命周期与 trait bound — 要求显式声明类型参数
