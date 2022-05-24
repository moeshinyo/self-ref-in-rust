//! 
//! 提供加载动态链接库并获取函数的接口。

use crate::result::Result;
use std::{path::Path, marker::PhantomData, borrow::Borrow};

/// 表示一个已加载的动态链接库。
pub struct Library;

impl Library {
    /// 加载一个动态链接库，并返回对应的[`Library`]实例。
    pub fn open<P: AsRef<Path>>(_path: P) -> Result<Self> {
        Ok(Library)
    }
}

/// 表示一个源自[`Library`]的函数实例。
pub struct Function<L, F> {
    _library: PhantomData<L>, 
    _marker: PhantomData<F>, 
}

impl<L, F: Dummy> Function<L, F> {

    /// 转换为函数指针类型。
    pub unsafe fn get_raw_fn(&self) -> F {
        F::dummy()
    }

    /// 从一个已加载的动态链接库中获取一个特定名字的API。
    pub fn from_ref(_library: impl Borrow<L>, _name: impl AsRef<str>) -> Result<Self> {
        Ok(Function{
            _library: PhantomData, 
            _marker: PhantomData, 
        })
    }

    /// 从一个已加载的动态链接库中获取一个特定名字的API。
    pub unsafe fn from_raw(_library: L, _name: impl AsRef<str>) -> Result<Self> {
        Ok(Function{
            _library: PhantomData, 
            _marker: PhantomData, 
        })
    }

    /// 返回一个空函数，该函数返回对应类型的默认值。
    pub fn dummy() -> Self {
        Function { _library: PhantomData, _marker: PhantomData }
    }
}

/// 为函数指针（fn）类型提供默认值。
pub trait Dummy {

    /// 返回一个函数指针，指向一个无任何行为的函数。
    fn dummy() -> Self;
}


macro_rules! impl_dummy {
    ($($argty: ident), *; $abi: literal) => {

        impl<R: ::std::default::Default, $($argty), *> Dummy for extern $abi fn($($argty), *) -> R {
            fn dummy() -> Self {
                extern $abi fn _dummy<R: ::std::default::Default, $($argty), *>($(_: $argty), *) -> R { 
                    <R as ::std::default::Default>::default()
                }
                _dummy
            }
        }
    };
}

macro_rules! generate_dummy_impls {
    ($first: ident, $($rest: ident), *; $abi: literal) => {
        impl_dummy!($first, $($rest), *; $abi);
        generate_dummy_impls!($($rest), *; $abi);
    };
    ($only: ident; $abi: literal) => {
        impl_dummy!($only; $abi);
        generate_dummy_impls!($abi);
    };
    ($abi: literal) => {
        impl_dummy!(; $abi);
    };
}

generate_dummy_impls!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q; "C");
