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
typeshaper = "0.3"
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
typex!(#[derive(Debug, Clone)] UserPublic  = User - [password_hash, created_at]);

// 只保留两个字段
typex!(#[derive(Debug, Clone)] UserSummary = User & [id, name]);

// 所有字段变可选
typex!(#[derive(Debug, Clone)] UserPatch   = User?);
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
typex!(#[derive(Debug, Clone)] OrderSnapshot = User + Address);

let snapshot = OrderSnapshot::from((user, address));
```

只需要"User 有而 Address 没有"的字段：

```rust
typex!(#[derive(Debug, Clone)] UserOnly = User % Address);  // Diff
```

---

## 表达式可以链式组合

```rust
// 先去掉 password_hash，再把剩余字段全部变可选
typex!(#[derive(Debug)] UserSafePatch = User - [password_hash]?);

// 去掉 password_hash 之后，只留摘要字段
typex!(#[derive(Debug)] UserSafeDto = User - [password_hash] & [id, name, email]);

// 括号控制结合方向：先 Partial 再 Required（等价于恢复非可选）
typex!(#[derive(Debug)] UserRestored = (User - [password_hash])?!);
```

---

## 增量更新的完整循环

```rust
// 可选版本用于更新接口
typex!(#[derive(Debug, Clone)] UserPatch    = User?);

// 验证通过后恢复为必填版本
typex!(#[derive(Debug, Clone)] UserVerified = UserPatch!);

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
typex!(#[derive(Debug, Clone)] UserPublic = User - [password_hash, created_at]);
typex!(#[derive(Debug, Clone)] UserPatch  = User?);
```

---

## 参考文档

### 安装

```toml
[dependencies]
typeshaper = "0.3"
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
| `T - [f1, f2]` | **Omit** | 移除列出的字段 | `TypeshaperInto<Target> for T` |
| `T & [f1, f2]` | **Pick** | 只保留列出的字段 | `TypeshaperInto<Target> for T` |
| `A + B` | **Merge** | 合并 A 和 B 的全部字段（不允许重名） | `From<(A, B)> for Target` |
| `T?` | **Partial** | 所有字段变为 `Option<_>` | `From<T> for Target` |
| `T!` | **Required** | 还原 Partial 的 `Option<_>` | `TryFrom<T> for Target` |
| `A % B` | **Diff** | A 有而 B 没有的字段 | `TypeshaperInto<Target> for A` |

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
typex!( [#[attr...]]  TargetName  =  Expr );
```

- **属性**（可选）：写在 `TargetName` 前，原样附加到生成的结构体，支持叠放多个。`typex!()` 不会自动添加任何 `#[derive]`，全部由调用方声明。
- **TargetName**：生成的结构体名称，同时注册到编译期注册表，可继续作为后续 `typex!()` 的输入。
- **Expr**：类型代数表达式，见上表。

```rust
typex!(
    #[derive(Debug, Clone, PartialEq)]
    #[serde(rename_all = "camelCase")]
    UserPublicDto = User & [id, name, email]
);
```

---

### 转换方法

`TypeshaperExt` trait 为所有类型自动实现，通过类型推断选择目标：

```rust
let public: UserPublic = user.project();   // 等价于 user.typeshaper_into()
```

`Merge` 使用元组 `From`，`Partial` 使用 `From`，`Required` 使用 `TryFrom`：

```rust
let snapshot = OrderSnapshot::from((user, address));
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
