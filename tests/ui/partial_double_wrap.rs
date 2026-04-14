// Fix 2: 对 Partial 类型再次应用 `?` 应报编译错误，而非静默产生 Option<Option<T>>
use typeshaper::{typeshaper, typex};

#[typeshaper]
pub struct User {
    pub id: u64,
    pub name: String,
}

typex!(UserDraft = User?);

// 对已经是 Partial 的类型再次应用 `?`，应当被拒绝
typex!(UserDoubleDraft = UserDraft?);

fn main() {}
