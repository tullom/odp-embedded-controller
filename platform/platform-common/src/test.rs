//! Generic, declarative test runner intended for on-target self-tests.
//!
//! Provides a small DSL backed by [`TestRunner`] for expressing "send a
//! request, expect a result" style checks against any in-process API.
//!
//! Two layers are provided:
//!
//! * The generic [`expect!`] macro accepts any expression to evaluate and
//!   apply a verb against. It is service-agnostic.
//! * Service-specific wrappers in [`crate::test::services`] (e.g.
//!   [`battery_tests!`], [`sensor_tests!`], [`fan_tests!`],
//!   [`time_alarm_tests!`]) generate the boilerplate for one row per trait
//!   method, auto-derive the test name from the method call, and insert the
//!   correct `.await` / handle prefix.
//!
//! # Quick start (generic)
//!
//! ```ignore
//! use platform_common::test::TestRunner;
//! use platform_common::expect;
//!
//! let mut runner = TestRunner::new();
//! expect!(runner; {
//!     "fan max rpm"  => fan.max_rpm().await,                        eq 6000u16;
//!     "sensor temp"  => sensor.temperature_immediate().await,       ok_in 20.0f32..=40.0;
//!     "voltage band" => battery.battery_status(BAT_ID).await,
//!         ok_pred |b| (11_000..=13_500).contains(&b.battery_present_voltage);
//! });
//! runner.summary();
//! ```
//!
//! # Verbs supported by [`expect!`]
//!
//! | Verb              | Passes when                                      |
//! |-------------------|--------------------------------------------------|
//! | `eq <expr>`       | `actual == expr`                                 |
//! | `ne <expr>`       | `actual != expr`                                 |
//! | `in <range>`      | `range.contains(&actual)`                        |
//! | `is_ok`           | `actual.is_ok()`                                 |
//! | `is_err`          | `actual.is_err()`                                |
//! | `ok_eq <expr>`    | `actual == Ok(expr)`                             |
//! | `ok_in <range>`   | `actual` is `Ok(v)` and `range.contains(&v)`     |
//! | `pred \|v\| <expr>`     | binds `v: &Actual`; passes when `expr` is true |
//! | `ok_pred \|v\| <expr>`  | `actual` is `Ok(inner)`, binds `v: &Inner`; passes when `expr` is true |
//!
//! # Statement syntax
//!
//! `"name" => <value-expr>, <verb> <verb-args>;`
//!
//! `"name"` may be any `&str` expression — typically a string literal, but
//! the service-specific wrappers pass a `stringify!(method(args))` so each
//! row is named after the call it makes.
//!
//! # Service-specific row syntax
//!
//! Inside [`battery_tests!`] / [`sensor_tests!`] / [`fan_tests!`] /
//! [`time_alarm_tests!`] each row has one of these shapes, with the verb
//! expression wrapped in parentheses so the parser can find the row
//! boundary unambiguously:
//!
//! * `<method>(<args>)                              => ( <verb> <verb-args> );`
//! * `<method>(<args>) -> <field>                   => ( <verb> <verb-args> );`
//! * `<method>(<args>) -> |<v>| <projection-expr>   => ( <verb> <verb-args> );`
//!
//! `<verb> <verb-args>` is anything `expect!` accepts (e.g. `is_ok`,
//! `eq 0u16`, `ok_in 1..=10`, `ok_pred |x| *x > 0`).

use defmt::{error, info};

/// Accumulates pass/fail counts for a sequence of [`expect!`] checks.
///
/// `TestRunner` is intentionally minimal — the heavy lifting (capturing the
/// actual value, formatting the failure message, applying the verb) is done
/// by the [`expect!`] macro. The runner only tracks counts and logs the
/// per-test pass/fail line.
pub struct TestRunner {
    passed: u32,
    failed: u32,
}

impl Default for TestRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl TestRunner {
    /// Create a new, empty runner.
    pub const fn new() -> Self {
        Self { passed: 0, failed: 0 }
    }

    /// Number of checks that have passed so far.
    pub fn passed(&self) -> u32 {
        self.passed
    }

    /// Number of checks that have failed so far.
    pub fn failed(&self) -> u32 {
        self.failed
    }

    /// Total number of checks recorded so far.
    pub fn total(&self) -> u32 {
        self.passed + self.failed
    }

    /// Returns `true` if every check recorded so far has passed.
    pub fn all_passed(&self) -> bool {
        self.failed == 0
    }

    /// Record a single check outcome and log a one-line PASS/FAIL marker.
    pub fn record(&mut self, name: &str, ok: bool) {
        if ok {
            self.passed += 1;
            info!("[test] PASS: {=str}", name);
        } else {
            self.failed += 1;
            error!("[test] FAIL: {=str}", name);
        }
    }

    /// Emit a human-readable summary line via `defmt`.
    pub fn summary(&self) {
        info!(
            "[test] SUMMARY: {=u32} passed, {=u32} failed (total {=u32})",
            self.passed,
            self.failed,
            self.total()
        );
    }
}

/// Declarative test DSL.
///
/// Each statement has the shape `"name" => <expr>, <verb> <args>;` and is
/// expanded into an evaluation of `<expr>`, application of the verb to the
/// produced value, and a [`TestRunner::record`] call. See the [module
/// docs][crate::test] for the list of supported verbs.
#[macro_export]
macro_rules! expect {
    ($runner:expr; { $($body:tt)* }) => {
        $crate::__expect_inner!($runner; $($body)*);
    };
}

#[macro_export]
#[doc(hidden)]
macro_rules! __expect_inner {
    // ---- terminator --------------------------------------------------------
    ($runner:expr;) => {};

    // ---- eq <expr> ---------------------------------------------------------
    ($runner:expr; $name:expr => $value:expr, eq $expected:expr; $($rest:tt)*) => {
        {
            let __name: &str = $name;
            let __actual = $value;
            let __expected = $expected;
            let __ok = __actual == __expected;
            if !__ok {
                ::defmt::error!(
                    "[test] FAIL detail: {=str}: actual={:?} expected={:?}",
                    __name, __actual, __expected,
                );
            }
            $runner.record(__name, __ok);
        }
        $crate::__expect_inner!($runner; $($rest)*);
    };

    // ---- ne <expr> ---------------------------------------------------------
    ($runner:expr; $name:expr => $value:expr, ne $expected:expr; $($rest:tt)*) => {
        {
            let __name: &str = $name;
            let __actual = $value;
            let __expected = $expected;
            let __ok = __actual != __expected;
            if !__ok {
                ::defmt::error!(
                    "[test] FAIL detail: {=str}: actual={:?} unexpectedly equals {:?}",
                    __name, __actual, __expected,
                );
            }
            $runner.record(__name, __ok);
        }
        $crate::__expect_inner!($runner; $($rest)*);
    };

    // ---- in <range> --------------------------------------------------------
    ($runner:expr; $name:expr => $value:expr, in $range:expr; $($rest:tt)*) => {
        {
            let __name: &str = $name;
            let __actual = $value;
            let __range = $range;
            let __ok = __range.contains(&__actual);
            if !__ok {
                ::defmt::error!(
                    "[test] FAIL detail: {=str}: actual={:?} out of range",
                    __name, __actual,
                );
            }
            $runner.record(__name, __ok);
        }
        $crate::__expect_inner!($runner; $($rest)*);
    };

    // ---- is_ok -------------------------------------------------------------
    ($runner:expr; $name:expr => $value:expr, is_ok; $($rest:tt)*) => {
        {
            let __name: &str = $name;
            let __actual = $value;
            let __ok = __actual.is_ok();
            if !__ok {
                ::defmt::error!(
                    "[test] FAIL detail: {=str}: expected Ok(_), got {:?}",
                    __name, __actual,
                );
            }
            $runner.record(__name, __ok);
        }
        $crate::__expect_inner!($runner; $($rest)*);
    };

    // ---- is_err ------------------------------------------------------------
    ($runner:expr; $name:expr => $value:expr, is_err; $($rest:tt)*) => {
        {
            let __name: &str = $name;
            let __actual = $value;
            let __ok = __actual.is_err();
            if !__ok {
                ::defmt::error!(
                    "[test] FAIL detail: {=str}: expected Err(_), got {:?}",
                    __name, __actual,
                );
            }
            $runner.record(__name, __ok);
        }
        $crate::__expect_inner!($runner; $($rest)*);
    };

    // ---- ok_eq <expr> ------------------------------------------------------
    ($runner:expr; $name:expr => $value:expr, ok_eq $expected:expr; $($rest:tt)*) => {
        {
            let __name: &str = $name;
            let __actual = $value;
            let __expected = $expected;
            let __ok = matches!(&__actual, Ok(__v) if __v == &__expected);
            if !__ok {
                ::defmt::error!(
                    "[test] FAIL detail: {=str}: actual={:?} expected Ok({:?})",
                    __name, __actual, __expected,
                );
            }
            $runner.record(__name, __ok);
        }
        $crate::__expect_inner!($runner; $($rest)*);
    };

    // ---- ok_in <range> -----------------------------------------------------
    ($runner:expr; $name:expr => $value:expr, ok_in $range:expr; $($rest:tt)*) => {
        {
            let __name: &str = $name;
            let __actual = $value;
            let __range = $range;
            let __ok = match &__actual {
                Ok(__v) => __range.contains(__v),
                Err(_) => false,
            };
            if !__ok {
                ::defmt::error!(
                    "[test] FAIL detail: {=str}: actual={:?} not Ok(in range)",
                    __name, __actual,
                );
            }
            $runner.record(__name, __ok);
        }
        $crate::__expect_inner!($runner; $($rest)*);
    };

    // ---- pred |v| <bool> ---------------------------------------------------
    ($runner:expr; $name:expr => $value:expr, pred |$var:ident| $check:expr; $($rest:tt)*) => {
        {
            let __name: &str = $name;
            let __actual = $value;
            let __ok = { let $var = &__actual; $check };
            if !__ok {
                ::defmt::error!(
                    "[test] FAIL detail: {=str}: predicate failed for {:?}",
                    __name, __actual,
                );
            }
            $runner.record(__name, __ok);
        }
        $crate::__expect_inner!($runner; $($rest)*);
    };

    // ---- ok_pred |v| <bool> ------------------------------------------------
    ($runner:expr; $name:expr => $value:expr, ok_pred |$var:ident| $check:expr; $($rest:tt)*) => {
        {
            let __name: &str = $name;
            let __actual = $value;
            let __ok = match &__actual {
                Ok($var) => $check,
                Err(_) => false,
            };
            if !__ok {
                ::defmt::error!(
                    "[test] FAIL detail: {=str}: ok-predicate failed for {:?}",
                    __name, __actual,
                );
            }
            $runner.record(__name, __ok);
        }
        $crate::__expect_inner!($runner; $($rest)*);
    };
}

// ---------------------------------------------------------------------------
// Service-specific wrappers
// ---------------------------------------------------------------------------

/// Row syntax for service-specific test macros.
///
/// Each macro below accepts a brace-delimited list of statements of the form:
///
/// * `<method>(<args>) => <verb> <verb-args>;`
///   Calls `$handle.<method>(<args>)` (awaited for async services), names
///   the row `"<method>(<args>)"`, then applies the verb.
///
/// * `<method>(<args>) -> <field> => <verb> <verb-args>;`
///   Same, but maps over an `Ok(inner)` result to extract `inner.<field>`
///   before applying the verb. The name becomes `"<method>().<field>"`.
///
/// * `<method>(<args>) -> |<v>| <projection-expr> => <verb> <verb-args>;`
///   Maps over `Ok(inner)` with an arbitrary closure body that may reference
///   `<v>`. Use this for `b.cycle_count + b.something` style projections.
///
/// Each verb is one of the verbs supported by [`expect!`].
#[doc(hidden)]
pub mod _service_dsl_docs {}

/// Tests for a [`battery_service_interface::BatteryService`] handle.
///
/// All methods are async and fallible. See the [module docs][crate::test]
/// for row syntax.
#[macro_export]
macro_rules! battery_tests {
    ($runner:expr, $svc:expr; { $($body:tt)* }) => {
        $crate::__svc_async_tests!($runner, $svc; $($body)*);
    };
}

/// Tests for a [`thermal_service_interface::sensor::SensorService`] handle.
#[macro_export]
macro_rules! sensor_tests {
    ($runner:expr, $svc:expr; { $($body:tt)* }) => {
        $crate::__svc_async_tests!($runner, $svc; $($body)*);
    };
}

/// Tests for a [`thermal_service_interface::fan::FanService`] handle.
#[macro_export]
macro_rules! fan_tests {
    ($runner:expr, $svc:expr; { $($body:tt)* }) => {
        $crate::__svc_async_tests!($runner, $svc; $($body)*);
    };
}

/// Tests for a [`time_alarm_service_interface::TimeAlarmService`] handle.
///
/// All methods on `TimeAlarmService` are synchronous, so this macro does
/// NOT add a `.await` to the generated calls.
#[macro_export]
macro_rules! time_alarm_tests {
    ($runner:expr, $svc:expr; { $($body:tt)* }) => {
        $crate::__svc_sync_tests!($runner, $svc; $($body)*);
    };
}

#[macro_export]
#[doc(hidden)]
macro_rules! __svc_async_tests {
    ($runner:expr, $svc:expr;) => {};

    // method(args) -> |v| projection => (verb vargs);
    ($runner:expr, $svc:expr;
     $method:ident ( $($args:tt)* ) -> |$v:ident| $proj:expr => ( $($vargs:tt)+ ); $($rest:tt)*) => {
        $crate::expect!($runner; {
            concat!(stringify!($method), "(..) -> ", stringify!($proj))
                => $svc.$method($($args)*).await.map(|$v| $proj),
                $($vargs)+;
        });
        $crate::__svc_async_tests!($runner, $svc; $($rest)*);
    };

    // method(args) -> field => (verb vargs);
    ($runner:expr, $svc:expr;
     $method:ident ( $($args:tt)* ) -> $field:ident => ( $($vargs:tt)+ ); $($rest:tt)*) => {
        $crate::expect!($runner; {
            concat!(stringify!($method), "().", stringify!($field))
                => $svc.$method($($args)*).await.map(|__v| __v.$field),
                $($vargs)+;
        });
        $crate::__svc_async_tests!($runner, $svc; $($rest)*);
    };

    // method(args) => (verb vargs);
    ($runner:expr, $svc:expr;
     $method:ident ( $($args:tt)* ) => ( $($vargs:tt)+ ); $($rest:tt)*) => {
        $crate::expect!($runner; {
            stringify!($method($($args)*))
                => $svc.$method($($args)*).await,
                $($vargs)+;
        });
        $crate::__svc_async_tests!($runner, $svc; $($rest)*);
    };
}

#[macro_export]
#[doc(hidden)]
macro_rules! __svc_sync_tests {
    ($runner:expr, $svc:expr;) => {};

    // method(args) -> |v| projection => (verb vargs);
    ($runner:expr, $svc:expr;
     $method:ident ( $($args:tt)* ) -> |$v:ident| $proj:expr => ( $($vargs:tt)+ ); $($rest:tt)*) => {
        $crate::expect!($runner; {
            concat!(stringify!($method), "(..) -> ", stringify!($proj))
                => $svc.$method($($args)*).map(|$v| $proj),
                $($vargs)+;
        });
        $crate::__svc_sync_tests!($runner, $svc; $($rest)*);
    };

    // method(args) -> field => (verb vargs);
    ($runner:expr, $svc:expr;
     $method:ident ( $($args:tt)* ) -> $field:ident => ( $($vargs:tt)+ ); $($rest:tt)*) => {
        $crate::expect!($runner; {
            concat!(stringify!($method), "().", stringify!($field))
                => $svc.$method($($args)*).map(|__v| __v.$field),
                $($vargs)+;
        });
        $crate::__svc_sync_tests!($runner, $svc; $($rest)*);
    };

    // method(args) => (verb vargs);
    ($runner:expr, $svc:expr;
     $method:ident ( $($args:tt)* ) => ( $($vargs:tt)+ ); $($rest:tt)*) => {
        $crate::expect!($runner; {
            stringify!($method($($args)*))
                => $svc.$method($($args)*),
                $($vargs)+;
        });
        $crate::__svc_sync_tests!($runner, $svc; $($rest)*);
    };
}
