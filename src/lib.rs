//! # 引子
//! 
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
//! Rust提供了非常强的安全保证，不会允许这种事发生。让我们先构造一个`BundleService`的实例：
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
//! - 首先加载动态链接库获取`library`，并通过`dummy()`创建空白的`Function`s占位，构造了一个没有自引用的`bundle`；
//! - 然后通过`bundle.library`获取了真正的`func_*`函数，将它们赋值到`bundle`中，这样`bundle`就变成了一个自引用结构。
//! 
//! 尽管构造成功了，可一旦我们尝试把它移动到另一个变量中，就会得到一个编译错误：
//! 
//! ``` text
//! error[E0505]: cannot move out of `bundle` because it is borrowed
//! ```
//! 
//! 这意味着：
//! 
//! - 这个自引用结构无法被移动、修改（不暴露字段的情况下）；
//! - 这个自引用结构不能有接收`&mut self`的方法。
//! 
//! 也就是说Rust允许我们构造这样一个自引用结构，但不能移动它。这样的限制过于严格了，我们需要更加灵活的工具。
//! 
//! 
//! # Pinning
//! 
//! 标准库提供了Pinning相关基础设施，给予我们将类型（的实例）钉死在内存中的能力。如果对Pinning有疑问，
//! 可以看一下[这个关于Pinning的问答](<#关于pinning的问答>)。
//! 
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
//! - 首先将引用换成了裸指针，摆脱了借用规则的约束；
//! - 并通过`PhantomPinned`消除了`Unpin`的自动实现，使`BundleService`能够真正地被`Pin<P>`钉死在内存中。
//! 
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
//! impl BundleService {
//!     fn new() -> Pin<Box<Self>> {
//!         let mut boxed = Box::pin(BundleService {
//!             library: Library::open("service.dll").unwrap(), 
//!             func_login: Function::dummy(), 
//!             func_logout: Function::dummy(), 
//!             _marker: PhantomPinned, 
//!         });
//!         unsafe {
//!             let pinned = Pin::get_unchecked_mut(Pin::as_mut(&mut boxed));
//!             let raw_lib = &pinned.library as *const Library;
//!             pinned.func_login = Function::from_raw(raw_lib, "login").unwrap();
//!             pinned.func_logout = Function::from_raw(raw_lib, "logout").unwrap();
//!         }
//!         boxed
//!     }
//! }
//! ```
//! 
//! - 先通过`Box::pin`在堆上创建了一个没有自引用的`bundle`。此时堆上的`BundleService`已经被钉死了。用户在Safe Rust中无法
//!   通过`bundle`获取一个`&mut BundleService`，也就无法移动堆上的`BundleService`。
//! - 然后我们通过`unsafe`获取`&mut BundleService`，从`library`中获取真正的`func_*`，并用它们替换掉`dummy()`产生的空白函数
//!   完成对`bundle`的初始化。
//! 
//! 只要遵守`get_unchecked_mut`的安全约定并且不产生未定义行为，我们在这里使用`unsafe`是安全的。
//! 最终我们构造了一个`bundle`，它能够被自由移动，但它指向的`BundleService`被钉死在了内存中。
//! 
//! 内存布局是这样的：
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
//! 这里使用`Box`只是因为方便，如果介意堆分配的开销，在用户侧构造`Pin<&mut BundleService>`将`BundleService`
//! 钉死在调用栈中也是可行的，但这会更棘手一些。
//! 
//! ---
//! 
//! 其实`Pin<P>`不太适合我们的场景，它是为Async Rust设计的，目的是将引用或指针从一段Unsafe代码（如：异步运行时的内部实现）
//! 通过一段未知的用户代码传递到另一段Unsafe代码（如：编译器为异步函数实现`Future`时生成的代码）中时，保证「指向的值在内存中
//! 位置不会改变」的承诺不会被未知的用户代码破坏。
//! 
//! 
//! # 通过堆分配消除自引用
//! 
//! 基于Pinning的方案的确实现了我们的目标，但由于应用场景的差异，它做了许多没有必要的事。
//! 事实上，只要不把指针暴露给用户，仅需将被引用的数据放在堆中，就不会形成自引用了。
//! 
//! 想象这样一种内存布局：
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
//! 类似于Pinning的方案，在这种设计中，移动`BundleService`并不会造成引用失效。
//! 
//! 我们可以选择通过`Box::into_raw`实现这个方案：
//! 
//! ```
//! # use self_ref_in_rust::{loading::{Library, Function}, error::Result};
//! # use std::ffi::c_void;
//! pub struct BundleService {  
//!     library: *const Library, 
//!     func_login: Function<*const Library, extern "C" fn(*const c_void) -> i32>,
//!     func_logout: Function<*const Library, extern "C" fn() -> i32>,
//! }
//! impl BundleService {
//!     fn new() -> Self {
//!         let library = Box::into_raw(Box::new(Library::open("service.dll").unwrap()))
//!             as *const _;
//!         Self {
//!             library: library, 
//!             func_login: unsafe { Function::from_raw(library, "login").unwrap() },
//!             func_logout: unsafe { Function::from_raw(library, "logout").unwrap() },
//!         }
//!     }
//! }
//! ```
//! 
//! - 创建一个`Box<Library>`，通过`Box::into_raw`将其转换为裸指针，便可以在`func_*`中共享`Library`了。 
//! 
//! 当然，也不要忘了回收资源，否则会造成内存泄漏：
//! 
//! 
//! ```
//! # use self_ref_in_rust::{loading::{Library, Function}, error::Result};
//! # use std::ffi::c_void;
//! # pub struct BundleService {  
//! #     library: *const Library, 
//! #     func_login: Function<*const Library, extern "C" fn(*const c_void) -> i32>,
//! #     func_logout: Function<*const Library, extern "C" fn() -> i32>,
//! # }
//! impl Drop for BundleService {
//!     fn drop(&mut self) {
//!         let _library = unsafe { Box::from_raw(self.library as *mut Library) };
//!         self.func_login = Function::dummy();
//!         self.func_logout = Function::dummy();
//!     }
//! }
//! ```
//! 
//! - 在`BundleService`的`drop`实现中通过`Box::from_raw`取回`Box<Library>`，它会在离开作用域后释放堆上的`Library`。
//! - 在`_library`被释放前，先释放`func_*`，保证被引用的`Library`的生命周期严格长于引用它的`Function`s。
//! 
//! ---
//! 
//! 也可以选择使用引用计数实现这个方案：
//! 
//! ```
//! # use self_ref_in_rust::{loading::{Library, Function}, error::Result};
//! # use std::{ffi::c_void, rc::Rc};
//! pub struct BundleService {  
//!     library: Rc<Library>, 
//!     func_login: Function<Rc<Library>, extern "C" fn(*const c_void) -> i32>,
//!     func_logout: Function<Rc<Library>, extern "C" fn() -> i32>,
//! }
//! impl BundleService {
//!     fn new() -> Self {
//!         let library = Rc::new(Library::open("service.dll").unwrap());
//!         Self {
//!             library: library.clone(), 
//!             func_login: Function::from_ref(library.clone(), "login").unwrap(),
//!             func_logout: Function::from_ref(library.clone(), "logout").unwrap(),
//!         }
//!     }
//! }
//! ```
//! 
//! 使用引用计数就不再需要手动释放资源了，也避免了使用`unsafe`，只是会带来一定的运行时开销。不过比起操纵裸指针带来的风险与
//! 心智负担，引用计数微不足道的开销通常都是能够接受的。
//! 
//! # 成熟的解决方案
//! 
//! 在C++中处理自引用结构会比较轻松，由于存在移动构造函数，类型的设计者可以在自引用结构被移动时更新类型中存在的指针。
//! 在Rust中处理自引用结构是比较麻烦的，但也存在许多针对自引用结构的库，允许我们避开`unsafe`方便地实现自己的自引用结构。
//! 
//! 譬如[rental](<https://docs.rs/rental/0.5.6/rental/index.html#example>)就有一个相似的例子：
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
//! # 结语
//! 
//! 一方面，还是建议使用成熟的库，上文出现的Unsafe实现们作为参考就好。另一方面，可以考虑一下真的需要自引用结构吗？许多时候
//! 自引用结构也可以拆开封装，向用户暴露两个结构可能能够提供更多的灵活性呢。
//! 
//! ---
//! 
//! ## 关于Pinning的问答
//! 
//! 1. Rust提供了默认的移动语义，不考虑Copy的情况下，类型（的实例）都是可以Move的。对此`Pin<P>`也不例外，为何它能阻止用户Move
//! 一个变量呢？
//! 
//! > 关键就在于`P`，它不是我们想钉死在内存中的那个类型，而是对应类型的指针。Safe Rust中的可变性总是独占的，`Pin<P>`阻止用户从`P`中
//! 获取`&mut T`，那么用户便无法修改`P`指向的`T`——失去了Move的能力。
//! 
//! 2. 那这样类型`T`的设计者怎么执行那些不移动`T`，但是需要`&mut T`的操作呢？
//! 
//! > 答案不是很优雅，`Pin<P>`提供了一些工具函数执行这样的操作，但更普遍的是通过`unsafe`获取`&mut T`完成相关操作，健全性由类型的设计者保证。
//! 
//! 3. 用户等到`Pin<P>`离开作用域后，移动原变量，然后再构造一个`Pin<P>`，不就绕过Pinning的约束了吗？
//! 
//! > 对于`Pin<Box<T>>`来说不存在这个问题，因为`Box<T>`是访问其指向的数据的唯一方式。对于普通的引用而言，这个顾虑是存在的，所以对变量直接
//! 构造一个`P<&mut T>`需要`unsafe`，健全性由用户自行保证。
//! 
//! 4. 将变量钉死是通过借用规则实现的，修改钉死的变量是通过`unsafe`实现的，那`Pin<P>`存在的意义是什么呢？
//! 
//! > `Pin<P>`提供一种保证，将「值在内存中的位置不再改变」的承诺在两段`unsafe`代码间的用户代码中传递，基本上是针对Async Rust设计的。
//! 
//! [回到Pinning一节](<#pinning>)


pub mod loading;
pub mod error;
#[allow(dead_code)]
mod factory;


