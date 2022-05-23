//! 最近一位朋友遇到了这样一个问题：设计类型时，将具有引用关系的两个类型“打包”到同一个结构体中，产生了难以解决的编译错误。
//! 社区中也常常有同学问遇到类似的问题，在这篇文章中我们将从零开始，探究该如何设计这类自引用的类型，并逐步改进我们的设计。
//! 
//! 想象这样一个场景：在设计一个运行时调用动态链接库中的API的接口时，需要先加载动态链接库获得一个`Library`结构，
//! 再从`Library`中获取多个`Function`s（`Function`内部包含`Library`的引用），最后将`Library`与`Function`s放置到
//! 结构体`BundleService`中，于`BundleService`中将`Function`s封装为暴露给用户的方法。
//! 
//! 需求描述非常直观，简直就是把类型定义用文字写了出来，于是我们很快写出了这样一段定义：
//! 
//! ```
//! # use self_ref_in_rust::{loading::{Library, Function}, error::Result};
//! # use std::ffi::c_void;
//! pub struct BundleService<'a> {  
//!     library: Library, 
//!     func_login: Function<&'a Library, extern "C" fn(*const c_void) -> i32>,
//!     func_logout: Function<&'a Library, extern "C" fn() -> i32>,
//! }
//! ```
//! 
//! 其中`library`是我们加载的动态链接库，`func_*`存储从动态链接库中获取的API，它包含一个`Library`类型的引用。
//! 若`func_*`中的`&'a Library`指向了字段`library`，那`BundleService`便成为了一个自引用结构，内存布局是这样的：
//! 
//! ``` text
//! +----------BundleService-------------+-------
//! | library | func_login | func_logout |  ...
//! +---------+------------+-------------+-------
//!     Λ           |             |
//!     +-----------+-------------+
//! ```
//! 
//! 那如果我们把`BundleService`移动到另一个内存位置，`func_*`不就指向被移动前的`library`了吗？像这样：
//! 
//! ``` text
//! +---------+------------+-------------+-------+----------BundleService-------------+
//! |         |            |             |  ...  | library | func_login | func_logout |
//! +---------+------------+-------------+-------+---------+------------+-------------+
//!      Λ                                                       |             |
//!      +-------------------------------------------------------+-------------+
//! ```
//! 
//! Rust提供了非常强的安全保证，不会允许这种事发生。让我们构造一个`BundleService`的实例看看会发生什么：
//! 
//! ```
//! # use self_ref_in_rust::{loading::{Library, Function}, error::Result};
//! # use std::ffi::c_void;
//! # pub struct BundleService<'a> { 
//! #     library: Library, 
//! #     func_login: Function<&'a Library, extern "C" fn(*const c_void) -> i32>,
//! #     func_logout: Function<&'a Library, extern "C" fn() -> i32>,
//! # }
//! let mut bundle = BundleService {
//!     library: Library::open("service.dll").unwrap(), 
//!     func_login: Function::dummy(), 
//!     func_logout: Function::dummy(), 
//! };
//! bundle.func_login = Function:: from_ref(&bundle.library, "login").unwrap();
//! bundle.func_logout = Function:: from_ref(&bundle.library, "logout").unwrap();
//! ```
//! 
//! 首先我们加载动态链接库获取`library`，并通过`dummy()`创建空白的`Function`s占位，构造一个没有自引用的`bundle`。然后再
//! 通过`bundle.library`获取真正的`func_*`函数，将它们赋值到`bundle`中。这样就构造成功了，我们获得了一个自引用的`bundle`。
//! 但如果我们尝试把它移动到另一个变量中，会得到一个编译错误。Rust允许我们构造这样一个自引用结构，但不能移动它。如果用户没有
//! 移动`bundle`的需求，这个设计看起来就够用了。
//! 
//! 然而，由于目前Rust没有完成Placement New这样的特性，我们无法为它编写一个`new`函数，因为从`new`函数中返回它也是Move操作。
//! 并且，我们无法在`bundle`上调用接收`&mut self`的方法，因为`bundle`的一个字段已经被借用了，这给`BundleService`的实现带
//! 来了更多的限制。
//! 
//! 
//! ## Pinning
//! 
//! 标准库提供了Pinning相关基础设施，给予我们将类型（的实例）钉死在内存中的能力。如果对Pinning有疑问，可以看一下[这个问答](<#关于pinning的问答>)。
//! 让我们先定义一个支持Pinned的`BundleService`类型：
//! 
//! ```
//! # use self_ref_in_rust::{loading::{Library, Function}, error::Result};
//! # use std::{ffi::c_void, marker::PhantomPinned};
//! pub struct BundleService {  
//!     library: Library, 
//!     func_login: Function<*const Library, extern "C" fn(*const c_void) -> i32>,
//!     func_logout: Function<*const Library, extern "C" fn() -> i32>,
//!     _marker: PhantomPinned, 
//! }
//! ```
//! 
//! 我们将引用换成了裸指针，摆脱了借用规则的约束；并通过`PhantomPinned`消除了`Unpin`的自动实现，使`BundleService`能够
//! 真正地被`Pin<P>`钉死在内存中。违反别名规则（Alias Rules）是未定义行为，现在没有了借用检查器的帮助，我们应当更加小心。
//! 接下来让我们为它构建一个Pinned的实例：
//! 
//! ```
//! # use self_ref_in_rust::{loading::{Library, Function}, error::Result};
//! # use std::{ffi::c_void, pin::Pin, marker::PhantomPinned};
//! # pub struct BundleService { 
//! #     library: Library, 
//! #     func_login: Function<*const Library, extern "C" fn(*const c_void) -> i32>,
//! #     func_logout: Function<*const Library, extern "C" fn() -> i32>,
//! #     _marker: PhantomPinned, 
//! # }
//! let mut boxed = Box::pin(BundleService {
//!     library: Library::open("service.dll").unwrap(), 
//!     func_login: Function::dummy(), 
//!     func_logout: Function::dummy(), 
//!     _marker: PhantomPinned, 
//! });
//! unsafe {
//!     let pinned = Pin::get_unchecked_mut(Pin::as_mut(&mut boxed));
//!     let raw_lib = &pinned.library as *const Library;
//!     pinned.func_login = Function::from_raw(raw_lib, "login").unwrap();
//!     pinned.func_logout = Function::from_raw(raw_lib, "logout").unwrap();
//! }
//! let bundle: Pin<Box<BundleService>> = boxed;
//! ```
//! 
//! 我们先通过`dummy()`在堆上创建一个没有自引用的`bundle`，由于使用了`Box::pin`，此时在堆中的`BundleService`已经被钉死了。
//! 我们可以任意移动`bundle`这个变量，因为它只是一个指针，并且用户无法通过`bundle`移动真正的`BundleService`。然而，我们需要
//! 通过`library`获取真正的`func_*`，并用它们替换掉`dummy()`产生的空白函数。在这里我们通过`unsafe`实现这个目的，只要遵守
//! `get_unchecked_mut`的安全约定并且不产生未定义行为即可。最终我们构造了一个Pinned且自引用的`bundle`，内存布局是这样的：
//! 
//! ``` text
//! +--------+-------
//! | bundle |  ...
//! +--------+-------
//!     |
//!     +------------+
//!                  V
//! +-----------BundleService------------+
//! | library | func_login | func_logout |
//! +---------+------------+-------------+
//!     Λ           |             |
//!     +-----------+-------------+
//! ```
//! 
//! 现在要实现`new`函数就很简单了，我们只需将这个构建好的`bundle`返回即可。这里使用`Box`只是因为方便，如果介意
//! 堆分配的开销，在用户侧构造`Pin<&mut BundleService>`将`BundleService`钉死在调用栈中也是可行的，但这会更
//! 棘手一些。
//! 
//! 其实`Pin<P>`不太适合我们的场景，它是为Async Rust设计的，目的是将引用或指针从一段Unsafe代码（如：异步运行时的内部实现）
//! 通过一段未知的用户代码传递到另一段Unsafe代码（如：编译器为异步函数实现`Future`时生成的代码）时，保证「指向的值在内存中
//! 位置不会改变」的承诺不会被未知的用户代码破坏。
//! 
//! 
//! # 通过堆分配消除自引用
//! 
//! 事实上，只要不把指针暴露给用户，仅需将被引用的数据放在堆中，就不会形成自引用了。想象这样一种内存布局：
//! 
//! ``` text
//! +-------BundleService------+-------
//! | func_login | func_logout |  ...
//! +------------+-------------+-------
//!       /             |
//!      +--------------+
//!      V 
//! +---------+
//! | library |
//! +---------+
//! ```
//! 
//! 我们可以选择使用裸指针实现它：
//! 
//! ```
//! # use self_ref_in_rust::{loading::{Library, Function}, error::Result};
//! # use std::ffi::c_void;
//! pub struct BundleService {  
//!     library: *const Library, 
//!     func_login: Function<*const Library, extern "C" fn(*const c_void) -> i32>,
//!     func_logout: Function<*const Library, extern "C" fn() -> i32>,
//! }
//! ```
//! 
//! 创建一个`Box<Library>`，通过`Box::into_raw`将其转换为裸指针，便可以在`func_*`中共享`Library`了。
//! 最后我们在`BundleService`的`drop`实现中通过`Box::from_raw`取回`Box`，它会在离开作用域后释放堆上
//! 的`Library`。
//! 
//! 也可以选择使用引用计数实现它：
//! 
//! ```
//! # use self_ref_in_rust::{loading::{Library, Function}, error::Result};
//! # use std::{ffi::c_void, rc::Rc};
//! pub struct BundleService {  
//!     library: Rc<Library>, 
//!     func_login: Function<Rc<Library>, extern "C" fn(*const c_void) -> i32>,
//!     func_logout: Function<Rc<Library>, extern "C" fn() -> i32>,
//! }
//! ```
//! 
//! 使用裸指针需要用到`unsafe`，带来了更大的心智负担；使用引用计数更轻松一些，但带来了额外的性能惩罚。
//! 
//! 
//! # 成熟的解决方案
//! 
//! 在C++中处理自引用结构会比较轻松，由于存在移动构造函数，类型的设计者可以在自引用结构被移动时更新类型中存在的指针。
//! 在Rust中处理自引用结构是比较麻烦的，但存在许多针对自引用结构的库，允许我们避开`unsafe`高效地实现自己的自引用结构。
//! 譬如rental就有一个相似的例子：
//! 
//! ``` ignore
//! rental! {
//!     pub mod rent_libloading {
//!         use libloading;
//!    
//!         #[rental(deref_suffix)] // This struct will deref to the Deref::Target of Symbol.
//!         pub struct RentSymbol<S: 'static> {
//!             lib: Box<libloading::Library>, // Library is boxed for StableDeref.
//!             sym: libloading::Symbol<'lib, S>, // The 'lib lifetime borrows lib.
//!         }
//!     }
//! }
//!
//! fn main() {
//!     let lib = libloading::Library::new("my_lib.so").unwrap(); // Open our dylib.
//!     if let Ok(rs) = rent_libloading::RentSymbol::try_new(
//!         Box::new(lib),
//!         |lib| unsafe { lib.get::<extern "C" fn()>(b"my_symbol") }) // Loading symbols is unsafe.
//!     {
//!         (*rs)(); // Call our function
//!     };
//! }
//! ```
//! 
//! 简直方便极了有木有！
//! 
//! 
//! ## 结语
//! 
//! 一方面，还是建议使用成熟的库，上文出现的Unsafe实现们作为参考就好。另一方面，可以考虑一下真的需要自引用结构吗？许多时候
//! 自引用结构也可以拆开封装，向用户暴露两个结构可能能够提供更多的灵活性呢。
//! 
//! 
//! ## 关于Pinning的问答
//! 
//! 1. Rust提供了默认的移动语义，不考虑Copy的情况下，类型（的实例）都是可以Move的。对此`Pin<P>`也不例外，为何它能阻止用户Move
//! 一个变量呢？
//! 
//! 关键就在于`P`，它不是我们想钉死在内存中的那个类型，而是对应类型的指针。Safe Rust中的可变性总是独占的，`Pin<P>`阻止用户从`P`中
//! 获取`&mut T`，那么用户便无法修改`P`指向的`T`——失去了Move的能力。
//! 
//! 2. 那这样类型`T`的设计者怎么执行那些不移动`T`，但是需要`&mut T`的操作呢？
//! 
//! 答案不是很优雅，`Pin<P>`提供了一些工具函数执行这样的操作，但更普遍的是通过`unsafe`获取`&mut T`完成相关操作，健全性由类型的设计者保证。
//! 
//! 3. 用户等到`Pin<P>`离开作用域后，移动原变量，然后再构造一个`Pin<P>`，不就绕过Pinning的约束了吗？
//! 
//! 对于`Pin<Box<T>>`来说不存在这个问题，因为`Box<T>`是访问其指向的数据的唯一方式。对于普通的引用而言，这个顾虑是存在的，所以对变量直接
//! 构造一个`P<&mut T>`需要`unsafe`，健全性由用户自行保证。
//! 
//! 4. 将变量钉死是通过别名规则（Alias Rules）实现的，修改钉死的变量是通过`unsafe`实现的，那`Pin<P>`存在的意义是什么呢？
//! 
//! `Pin<P>`提供一种保证，将「值在内存中的位置不再改变」的承诺在两段`unsafe`代码间的用户代码中传递，基本上是针对Async Rust设计的。
//! 
//! [回到Pinning一节](<#pinning>)


pub mod loading;
pub mod error;
#[allow(dead_code)]
mod factory;


