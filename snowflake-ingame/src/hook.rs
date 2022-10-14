use std::mem::ManuallyDrop;

pub trait HookChain<'a, T> {
    /// Gets a pointer to the next method in the call chain.
    /// Every hook must advance the call chain for the next hook and return
    /// the result.
    fn fp_next(&mut self) -> &'a T;
}

pub trait HookHandle: Sized {
    /// Persist the hook and do not remove it on drop.
    fn persist(self) -> ManuallyDrop<Self> {
        ManuallyDrop::new(self)
    }
}

macro_rules! hook_make_chain {
     (chain $chain_name:ident for $fn_hook:ty) => {
         static $chain_name: std::sync::LazyLock<
            std::sync::RwLock<indexmap::map::IndexMap<std::primitive::usize, $fn_hook>>,
        > = std::sync::LazyLock::new(|| std::sync::RwLock::new(indexmap::map::IndexMap::new()));
     };
     ($vis:vis chain $chain_name:ident for $fn_hook:ty) => {
         $vis static $chain_name: std::sync::LazyLock<
            std::sync::RwLock<indexmap::map::IndexMap<std::primitive::usize, $fn_hook>>,
        > = std::sync::LazyLock::new(|| std::sync::RwLock::new(indexmap::map::IndexMap::new()));
     };
    (struct $fn_hook:ty) => {
         std::sync::LazyLock<
            std::sync::RwLock<indexmap::map::IndexMap<std::primitive::usize, $fn_hook>>,
        >
     };
    () => { std::sync::LazyLock::new(|| std::sync::RwLock::new(indexmap::map::IndexMap::new())); }
}
macro_rules! hook_define {
    (chain $chain_name:ident with $fn_hook:ty => $context_name:ident) => {
        // Make context
        pub struct $context_name<'a> {
            chain: std::iter::Rev<indexmap::map::Iter<'a, std::primitive::usize, $fn_hook>>,
        }

        // Impl chain
        impl<'a> crate::hook::HookChain<'a, $fn_hook> for $context_name<'a> {
            fn fp_next(&mut self) -> &'a $fn_hook {
                let (_, fp) = unsafe { self.chain.next().unwrap_unchecked() };
                fp
            }
        }

        // Make chain
        crate::hook::hook_make_chain!(chain $chain_name for $fn_hook);
    };

    (pub chain $chain_name:ident with $fn_hook:ty => $context_name:ident) => {
        // Make context
        pub struct $context_name<'a> {
            chain: std::iter::Rev<indexmap::map::Iter<'a, std::primitive::usize, $fn_hook>>,
        }

        // Impl chain
        impl<'a> crate::hook::HookChain<'a, $fn_hook> for $context_name<'a> {
            fn fp_next(&mut self) -> &'a $fn_hook {
                let (_, fp) = unsafe { self.chain.next().unwrap_unchecked() };
                fp
            }
        }
        // Make chain
        crate::hook::hook_make_chain!(pub chain $chain_name for $fn_hook);
    };
}

macro_rules! hook_impl_fn {
    (fn $fn_name:ident($($args:ident : $args_t:ty),+) -> $ret:ty => ($chain_name:ident, $detour:ident, $context:ident)) => {
        fn $fn_name($($args: $args_t),*) -> $ret {
            if let Ok(chain) = $chain_name.read() {
                if let Some((_, next)) = chain.last() {
                    let mut iter = chain.iter().rev();
                    // Advance the chain to the next call.
                    iter.next();
                    return next($($args),*,$context { chain: iter });
                }
            }
            $detour.call($($args),*)
        }
    }
}

macro_rules! hook_link_chain {
    ($(box link $chain:ident with $detour:ident => $($args:ident),*);*;) => {
        $(
            $chain.write()?.insert(0, Box::new(|$($args),*, _next| {
                $detour.call($($args),*)
            }));
        )*
    };
    ($(link $chain:ident with $detour:ident => $($args:ident),*);*;) => {
        $(
            $chain.write()?.insert(0, |$($args),*, _next| {
                $detour.call($($args),*)
            });
        )*
    };
}

macro_rules! hook_key {
    ($fn:ident) => {
        $fn as *const () as usize
    };
    (box $fn:ident) => {
        &*$fn as *const _ as *const () as usize
    };
}

pub(crate) use hook_define;
pub(crate) use hook_impl_fn;
pub(crate) use hook_key;
pub(crate) use hook_link_chain;
pub(crate) use hook_make_chain;