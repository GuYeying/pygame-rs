#[cfg(feature = "threading")]
mod threading {
    use crate::lock::OnceCell;

    pub struct StaticCell<T: 'static> {
        inner: OnceCell<T>,
    }

    impl<T> StaticCell<T> {
        #[doc(hidden)]
        pub const fn _from_once_cell(inner: OnceCell<T>) -> Self {
            Self { inner }
        }

        pub fn get(&'static self) -> Option<&'static T> {
            self.inner.get()
        }

        pub fn set(&'static self, value: T) -> Result<(), T> {
            self.inner.set(value)
        }

        pub fn get_or_init<F>(&'static self, f: F) -> &'static T
        where
            F: FnOnce() -> T,
        {
            self.inner.get_or_init(f)
        }

        pub fn get_or_try_init<F, E>(&'static self, f: F) -> Result<&'static T, E>
        where
            F: FnOnce() -> Result<T, E>,
        {
            if let Some(val) = self.inner.get() {
                return Ok(val);
            }
            let val = f()?;
            let _ = self.inner.set(val);
            Ok(self.inner.get().unwrap())
        }
    }

    #[macro_export]
    macro_rules! static_cell {
        ($($(#[$attr:meta])* $vis:vis static $name:ident: $t:ty;)+) => {
            $($(#[$attr])*
            $vis static $name: $crate::static_cell::StaticCell<$t> =
                $crate::static_cell::StaticCell::_from_once_cell($crate::lock::OnceCell::new());)+
        };
    }
}
#[cfg(feature = "threading")]
pub use threading::*;

// [pygame-rs 补丁③] android(bionic) 上每个 thread_local! 静态占一个
// pthread key（PTHREAD_KEYS_MAX=1024，进程共享）——static_cell! 为每个
// 内建类型生成一个 thread_local 静态，genesis 初始化类型群时键池耗尽，
// 启动即崩 `fatal runtime error: out of TLS keys, aborting`。
// 修复：android 上改用 no_std 变体（全局 OnceCell + unsafe impl Sync，
// 零 pthread key）。安全性契约：本引擎嵌入 = GIL 单线程 + 每进程单
// 解释器 + 恒同线程驱动——契约下"每线程一份"与"全局一份"可观察行为
// 完全相同。桌面/网页保留原 thread_local 实现（行为测试依赖每线程
// 独立 genesis）。数据与上游化讨论见 rustpython上游贡献PR准备.md。
#[cfg(all(not(feature = "threading"), feature = "std", not(target_os = "android")))]
mod non_threading {
    use crate::lock::OnceCell;
    use std::thread::LocalKey;

    pub struct StaticCell<T: 'static> {
        inner: &'static LocalKey<OnceCell<&'static T>>,
    }

    fn leak<T>(x: T) -> &'static T {
        Box::leak(Box::new(x))
    }

    impl<T> StaticCell<T> {
        #[doc(hidden)]
        #[must_use]
        pub const fn _from_local_key(inner: &'static LocalKey<OnceCell<&'static T>>) -> Self {
            Self { inner }
        }

        #[must_use]
        pub fn get(&'static self) -> Option<&'static T> {
            self.inner.with(|x| x.get().copied())
        }

        pub fn set(&'static self, value: T) -> Result<(), T> {
            self.inner.with(|x| {
                if x.get().is_some() {
                    Err(value)
                } else {
                    let _ = x.set(leak(value));
                    Ok(())
                }
            })
        }

        pub fn get_or_init<F>(&'static self, f: F) -> &'static T
        where
            F: FnOnce() -> T,
        {
            self.inner.with(|x| *x.get_or_init(|| leak(f())))
        }

        pub fn get_or_try_init<F, E>(&'static self, f: F) -> Result<&'static T, E>
        where
            F: FnOnce() -> Result<T, E>,
        {
            self.inner.with(|x| {
                if let Some(val) = x.get() {
                    Ok(*val)
                } else {
                    let val = leak(f()?);
                    let _ = x.set(val);
                    Ok(val)
                }
            })
        }
    }

    #[macro_export]
    macro_rules! static_cell {
        ($($(#[$attr:meta])* $vis:vis static $name:ident: $t:ty;)+) => {
            $($(#[$attr])*
            $vis static $name: $crate::static_cell::StaticCell<$t> = {
                ::std::thread_local! {
                     $vis static $name: $crate::lock::OnceCell<&'static $t> = const {
                         $crate::lock::OnceCell::new()
                     };
                }
                $crate::static_cell::StaticCell::_from_local_key(&$name)
            };)+
        };
    }
}
#[cfg(all(not(feature = "threading"), feature = "std", not(target_os = "android")))]
pub use non_threading::*;

// Same as `threading` variant, but wraps unsync::OnceCell with Sync.
// [pygame-rs 补丁③] android 亦复用本变体（全局单元，零 pthread key）。
#[cfg(all(not(feature = "threading"), any(not(feature = "std"), target_os = "android")))]
mod no_std {
    use crate::lock::OnceCell;

    // unsync::OnceCell is !Sync, but without std there can be no threads.
    struct SyncOnceCell<T>(OnceCell<T>);
    // SAFETY: Without std, threading is impossible.
    unsafe impl<T> Sync for SyncOnceCell<T> {}

    pub struct StaticCell<T: 'static> {
        inner: SyncOnceCell<T>,
    }

    impl<T> StaticCell<T> {
        #[doc(hidden)]
        pub const fn _from_once_cell(inner: OnceCell<T>) -> Self {
            Self {
                inner: SyncOnceCell(inner),
            }
        }

        pub fn get(&'static self) -> Option<&'static T> {
            self.inner.0.get()
        }

        pub fn set(&'static self, value: T) -> Result<(), T> {
            self.inner.0.set(value)
        }

        pub fn get_or_init<F>(&'static self, f: F) -> &'static T
        where
            F: FnOnce() -> T,
        {
            self.inner.0.get_or_init(f)
        }

        pub fn get_or_try_init<F, E>(&'static self, f: F) -> Result<&'static T, E>
        where
            F: FnOnce() -> Result<T, E>,
        {
            if let Some(val) = self.inner.0.get() {
                return Ok(val);
            }
            let val = f()?;
            let _ = self.inner.0.set(val);
            Ok(self.inner.0.get().unwrap())
        }
    }

    #[macro_export]
    macro_rules! static_cell {
        ($($(#[$attr:meta])* $vis:vis static $name:ident: $t:ty;)+) => {
            $($(#[$attr])*
            $vis static $name: $crate::static_cell::StaticCell<$t> =
                $crate::static_cell::StaticCell::_from_once_cell($crate::lock::OnceCell::new());)+
        };
    }
}
#[cfg(all(not(feature = "threading"), any(not(feature = "std"), target_os = "android")))]
pub use no_std::*;
