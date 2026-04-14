// Fix 3: pick 列表中出现重复字段名应报编译错误，而非生成含重复字段的结构体
use typeshaper::{typeshaper, typex};

#[typeshaper]
pub struct User {
    pub id: u64,
    pub name: String,
    pub age: u8,
}

// [id, id] 重复，应当被拒绝
typex!(UserDup = User & [id, id]);

fn main() {}
