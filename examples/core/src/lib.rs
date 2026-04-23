use typeshaper::typeshaper;

/// 用户账户——带有敏感字段，对外只暴露部分信息。
///
/// `#[typeshaper(export)]` 同时生成伴生宏 `typeshaper_import_User!()`，
/// 其他 crate 调用该宏即可将字段元数据注册到自己的编译期 HashMap，
/// 随后直接使用 `typex!()` 对 `User` 做各种类型变换。
#[typeshaper(export)]
#[derive(Debug, Clone, PartialEq)]
pub struct User {
    pub id: u64,
    pub name: String,
    pub email: String,
    pub role: String,
    pub active: bool,
    pub status: Status,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    Active,
    Inactive,
}

/// 收货地址——用于演示跨 crate 的 Merge / Diff 操作。
#[typeshaper(export)]
#[derive(Debug, Clone, PartialEq)]
pub struct Address {
    pub street: String,
    pub city: String,
    pub country: String,
}

/// 订单——`shipping` 字段类型为 `Address`，演示字段类型为结构体时各操作的行为。
/// typeshaper 将 `Address` 视为普通不透明类型，与 `String`、`u64` 无异：
/// 生成的结构体中原样复现该类型，转换时直接移动该字段值。
#[typeshaper(export)]
#[derive(Debug, Clone, PartialEq)]
pub struct Order {
    pub id: u64,
    pub user_id: u64,
    pub amount_cents: u64,
    pub shipping: Address,
    pub cancelled: bool,
}
