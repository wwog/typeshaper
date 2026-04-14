pub trait TypeshaperInto<T> {
    fn typeshaper_into(self) -> T;
}

pub trait TypeshaperExt: Sized {
    fn project<T>(self) -> T
    where
        Self: TypeshaperInto<T>,
    {
        <Self as TypeshaperInto<T>>::typeshaper_into(self)
    }
}

impl<T> TypeshaperExt for T {}
