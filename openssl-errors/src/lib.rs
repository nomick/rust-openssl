//! Custom error library support for the `openssl` crate.
//!
//! OpenSSL allows third-party libraries to integrate with its error API. This crate provides a safe interface to that.
//!
//! # Examples
//!
//! ```
//! use openssl_errors::{openssl_errors, put_error};
//! use openssl::error::Error;
//!
//! // Errors are organized at the top level into "libraries". The
//! // openssl_errors! macro can define these.
//! //
//! // Libraries contain a set of functions and reasons. The library itself,
//! // its functions, and its definitions all all have an associated message
//! // string. This string is what's shown in OpenSSL errors.
//! //
//! // The macro creates a type for each library with associated constants for
//! // its functions and reasons.
//! openssl_errors! {
//!     pub library MyLib("my cool library") {
//!         functions {
//!             FIND_PRIVATE_KEY("find_private_key");
//!         }
//!
//!         reasons {
//!             IO_ERROR("IO error");
//!             BAD_PASSWORD("invalid private key password");
//!         }
//!     }
//! }
//!
//! // The put_error! macro pushes errors onto the OpenSSL error stack.
//! put_error!(MyLib::FIND_PRIVATE_KEY, MyLib::BAD_PASSWORD);
//!
//! // Prints `error:80001002:my cool library:find_private_key:invalid private key password:src/lib.rs:27:`
//! println!("{}", Error::get().unwrap());
//!
//! // You can also optionally attach an extra string of context using the
//! // standard Rust format syntax.
//! let tries = 2;
//! put_error!(MyLib::FIND_PRIVATE_KEY, MyLib::IO_ERROR, "tried {} times", tries);
//!
//! // Prints `error:80001001:my cool library:find_private_key:IO error:src/lib.rs:34:tried 2 times`
//! println!("{}", Error::get().unwrap());
//! ```
#![warn(missing_docs)]
#![doc(html_root_url = "https://docs.rs/openssl-errors/0.1")]

use libc::{c_char, c_int};
use std::borrow::Cow;
use std::marker::PhantomData;
use std::ptr;

#[doc(hidden)]
pub mod export {
    pub use libc::{c_char, c_int};
    pub use openssl_sys::{
        init, ERR_get_next_error_library, ERR_load_strings, ERR_PACK, ERR_STRING_DATA,
    };
    pub use std::borrow::Cow;
    pub use std::option::Option;
    pub use std::ptr::null;
    pub use std::sync::Once;
}

/// An OpenSSL error library.
pub trait Library {
    /// Returns the ID assigned to this library by OpenSSL.
    fn id() -> c_int;
}

/// A function declaration, parameterized by its error library.
pub struct Function<T>(c_int, PhantomData<T>);

impl<T> Function<T> {
    /// Creates a function from its raw identifier.
    #[inline]
    pub const fn from_raw(raw: c_int) -> Function<T> {
        Function(raw, PhantomData)
    }

    /// Returns the function's raw identifier.
    #[inline]
    pub const fn as_raw(&self) -> c_int {
        self.0
    }
}

/// A reason declaration, parameterized by its error library.
pub struct Reason<T>(c_int, PhantomData<T>);

impl<T> Reason<T> {
    /// Creates a reason from its raw identifier.
    #[inline]
    pub const fn from_raw(raw: c_int) -> Reason<T> {
        Reason(raw, PhantomData)
    }

    /// Returns the reason's raw identifier.
    #[inline]
    pub const fn as_raw(&self) -> c_int {
        self.0
    }
}

/// This is not considered part of this crate's public API. It is subject to change at any time.
///
/// # Safety
///
/// `file` and `message` must be null-terminated.
#[doc(hidden)]
pub unsafe fn __put_error<T>(
    func: Function<T>,
    reason: Reason<T>,
    file: &'static str,
    line: u32,
    message: Option<Cow<'static, str>>,
) where
    T: Library,
{
    openssl_sys::ERR_put_error(
        T::id(),
        func.as_raw(),
        reason.as_raw(),
        file.as_ptr() as *const c_char,
        line as c_int,
    );
    let data = match message {
        Some(Cow::Borrowed(s)) => Some((s.as_ptr() as *const c_char as *mut c_char, 0)),
        Some(Cow::Owned(s)) => {
            let ptr = openssl_sys::CRYPTO_malloc(
                s.len() as _,
                concat!(file!(), "\0").as_ptr() as *const c_char,
                line!() as c_int,
            ) as *mut c_char;
            if ptr.is_null() {
                None
            } else {
                ptr::copy_nonoverlapping(s.as_ptr(), ptr as *mut u8, s.len());
                Some((ptr, openssl_sys::ERR_TXT_MALLOCED))
            }
        }
        None => None,
    };
    if let Some((ptr, flags)) = data {
        openssl_sys::ERR_set_error_data(ptr, flags | openssl_sys::ERR_TXT_STRING);
    }
}

/// Pushes an error onto the OpenSSL error stack.
///
/// A function and reason are required, and must be associated with the same error library. An additional formatted
/// message string can also optionally be provided.
#[macro_export]
macro_rules! put_error {
    ($function:expr, $reason:expr) => {
        unsafe {
            $crate::__put_error(
                $function,
                $reason,
                concat!(file!(), "\0"),
                line!(),
                $crate::export::Option::None,
            );
        }
    };
    ($function:expr, $reason:expr, $message:expr) => {
        unsafe {
            $crate::__put_error(
                $function,
                $reason,
                concat!(file!(), "\0"),
                line!(),
                $crate::export::Option::Some($crate::export::Cow::Borrowed(
                    concat!($message, "\0"),
                )),
            );
        }
    };
    ($function:expr, $reason:expr, $message:expr, $($args:tt)*) => {
        unsafe {
            $crate::__put_error(
                $function,
                $reason,
                concat!(file!(), "\0"),
                line!(),
                $crate::export::Option::Some($crate::export::Cow::Owned(
                    format!(concat!($message, "\0"), $($args)*)),
                ),
            );
        }
    };
}

/// Defines custom OpenSSL error libraries.
///
/// The created libraries can be used with the `put_error!` macro to create custom OpenSSL errors.
#[macro_export]
macro_rules! openssl_errors {
    ($(
        $(#[$lib_attr:meta])*
        $lib_vis:vis library $lib_name:ident($lib_str:expr) {
            functions {
                $(
                    $(#[$func_attr:meta])*
                    $func_name:ident($func_str:expr);
                )*
            }

            reasons {
                $(
                    $(#[$reason_attr:meta])*
                    $reason_name:ident($reason_str:expr);
                )*
            }
        }
    )*) => {$(
        $(#[$lib_attr])*
        $lib_vis enum $lib_name {}

        impl $crate::Library for $lib_name {
            fn id() -> $crate::export::c_int {
                static INIT: $crate::export::Once = $crate::export::Once::new();
                static mut LIB_NUM: $crate::export::c_int = 0;
                static mut STRINGS: [
                    $crate::export::ERR_STRING_DATA;
                    2 + $crate::openssl_errors!(@count $($func_name;)* $($reason_name;)*)
                ] = [
                    $crate::export::ERR_STRING_DATA {
                        error: 0,
                        string: concat!($lib_str, "\0").as_ptr() as *const $crate::export::c_char,
                    },
                    $(
                        $crate::export::ERR_STRING_DATA {
                            error: $crate::export::ERR_PACK(0, $lib_name::$func_name.as_raw(), 0),
                            string: concat!($func_str, "\0").as_ptr() as *const $crate::export::c_char,
                        },
                    )*
                    $(
                        $crate::export::ERR_STRING_DATA {
                            error: $crate::export::ERR_PACK(0, 0, $lib_name::$reason_name.as_raw()),
                            string: concat!($reason_str, "\0").as_ptr() as *const $crate::export::c_char,
                        },
                    )*
                    $crate::export::ERR_STRING_DATA {
                        error: 0,
                        string: $crate::export::null(),
                    }
                ];

                unsafe {
                    INIT.call_once(|| {
                        $crate::export::init();
                        LIB_NUM = $crate::export::ERR_get_next_error_library();
                        STRINGS[0].error = $crate::export::ERR_PACK(LIB_NUM, 0, 0);
                        $crate::export::ERR_load_strings(LIB_NUM, STRINGS.as_mut_ptr());
                    });

                    LIB_NUM
                }
            }
        }

        impl $lib_name {
            $crate::openssl_errors!(@func_consts $lib_name; 1; $($(#[$func_attr])* $func_name;)*);
            $crate::openssl_errors!(@reason_consts $lib_name; 1; $($(#[$reason_attr])* $reason_name;)*);
        }
    )*};
    (@func_consts $lib_name:ident; $n:expr; $(#[$attr:meta])* $name:ident; $($tt:tt)*) => {
        $(#[$attr])*
        pub const $name: $crate::Function<$lib_name> = $crate::Function::from_raw($n);
        $crate::openssl_errors!(@func_consts $lib_name; $n + 1; $($tt)*);
    };
    (@func_consts $lib_name:ident; $n:expr;) => {};
    (@reason_consts $lib_name:ident; $n:expr; $(#[$attr:meta])* $name:ident; $($tt:tt)*) => {
        $(#[$attr])*
        pub const $name: $crate::Reason<$lib_name> = $crate::Reason::from_raw($n);
        $crate::openssl_errors!(@reason_consts $lib_name; $n + 1; $($tt)*);
    };
    (@reason_consts $lib_name:ident; $n:expr;) => {};
    (@count $i:ident; $($tt:tt)*) => {
        1 + $crate::openssl_errors!(@count $($tt)*)
    };
    (@count) => { 0 };
}
