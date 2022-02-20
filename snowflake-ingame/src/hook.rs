use std::mem::ManuallyDrop;

pub trait HookChain<'a, T> {
    fn fp_next(&mut self) -> &'a T;
}

pub trait HookHandle : Sized {
    fn persist(self) -> ManuallyDrop<Self> {
        ManuallyDrop::new(self)
    }
}
