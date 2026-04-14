// Fix 5: #[typeshaper(typo)] 的无效参数应报编译错误，而非静默走 non-export 分支
use typeshaper::typeshaper;

#[typeshaper(typo)]
pub struct User {
    pub id: u64,
    pub name: String,
}

fn main() {}
