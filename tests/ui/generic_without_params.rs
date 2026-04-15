use typeshaper::{typeshaper, typex};

#[typeshaper]
pub struct Wrapped<T> {
    pub value: T,
    pub tag: String,
}

// ERROR: Wrapped 是泛型，但 typex! 中没有声明泛型参数。
typex!(WrappedNoTag = Wrapped - [tag]);
