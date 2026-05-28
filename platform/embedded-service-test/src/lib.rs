#![no_std]
//! Declarative on-target self-test DSL for ODP embedded-services traits.
//!
//! Provides a small DSL backed by [`TestRunner`] for expressing "send a
//! request, expect a result" style checks against any in-process API.
//!
//! Two layers are provided:
//!
//! * The generic [`expect!`] macro accepts any expression to evaluate and
//!   apply a verb against. It is service-agnostic.
//! * The generic [`embedded_service_test!`] macro generates the boilerplate for
//!   one row per trait method on any service handle, auto-deriving the
//!   test name from the method call and inserting the correct `.await`
//!   prefix. It works for any trait — there is no need to add a
//!   per-service wrapper.
//!
//! Thin per-service aliases ([`battery_tests!`], [`sensor_tests!`],
//! [`fan_tests!`], [`time_alarm_tests!`]) are provided for ergonomics but
//! are equivalent to `embedded_service_test!(async; ...)` or
//! `embedded_service_test!(sync; ...)`.
//!
//! # Quick start (generic)
//!
//! ```ignore
//! use embedded_service_test::{TestRunner, expect};
//!
//! let mut runner = TestRunner::new();
//! expect!(runner; {
//!     "fan max rpm"  => fan.max_rpm().await,                        eq 6000u16;
//!     "sensor temp"  => sensor.temperature_immediate().await,       ok_in_range 20.0f32..=40.0;
//!     "voltage band" => battery.battery_status(BAT_ID).await
//!                          .map(|b| b.battery_present_voltage),     ok_in_range 11_000..=13_500;
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
//! | `gt <expr>`       | `actual > expr`                                  |
//! | `ge <expr>`       | `actual >= expr`                                 |
//! | `lt <expr>`       | `actual < expr`                                  |
//! | `le <expr>`       | `actual <= expr`                                 |
//! | `in_range <range>` | `range.contains(&actual)`                       |
//! | `is_ok`           | `actual.is_ok()`                                 |
//! | `is_err`          | `actual.is_err()`                                |
//! | `ok_eq <expr>`    | `actual == Ok(expr)`                             |
//! | `ok_gt <expr>`    | `actual` is `Ok(v)` and `v > expr`               |
//! | `ok_ge <expr>`    | `actual` is `Ok(v)` and `v >= expr`              |
//! | `ok_lt <expr>`    | `actual` is `Ok(v)` and `v < expr`               |
//! | `ok_le <expr>`    | `actual` is `Ok(v)` and `v <= expr`              |
//! | `ok_in_range <range>` | `actual` is `Ok(v)` and `range.contains(&v)` |
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
//! Inside [`embedded_service_test!`] (and the per-service aliases
//! [`battery_tests!`] / [`sensor_tests!`] / [`fan_tests!`] /
//! [`time_alarm_tests!`]) each row has one of these shapes, with the verb
//! expression wrapped in parentheses so the parser can find the row
//! boundary unambiguously:
//!
//! * `<method>(<args>)                              => ( <verb> <verb-args> );`
//! * `<method>(<args>) -> <field>                   => ( <verb> <verb-args> );`
//! * `<method>(<args>) -> |<v>| <projection-expr>   => ( <verb> <verb-args> );`
//! * `<method>(<args>) ~> <field>                   => ( <verb> <verb-args> );`
//!   Infallible field projection: applies the verb to `value.<field>`.
//!   Use for methods that don't return `Result`.
//! * `<method>(<args>) ~> |<v>| <projection-expr>   => ( <verb> <verb-args> );`
//!   Infallible closure projection. Use for methods that don't return
//!   `Result` (e.g. bitfield accessors).
//! * `let <name> = <method>(<args>);`
//!   Infallible capture: binds `<name>` to the return value and records a
//!   passing row.
//! * `let <name> = try <method>(<args>);`
//!   Fallible capture: on `Ok(inner)` binds `<name>` to `inner` and records
//!   a passing row; on `Err` records the row and `return`s from the
//!   enclosing fn so subsequent rows are skipped.
//! * `sleep <duration-expr>;`
//!   Expands to `embassy_time::Timer::after(<duration-expr>).await;`. Used
//!   to wait between rows (e.g. to let a sampled value update). Requires
//!   the enclosing fn to be `async`.
//!
//! `<verb> <verb-args>` is anything `expect!` accepts (e.g. `is_ok`,
//! `eq 0u16`, `ok_in_range 1..=10`, `gt 0`).

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
/// docs][crate] for the list of supported verbs.
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
            ::defmt::debug!(
                "[test] {=str}: actual={:?} expected={:?}",
                __name, __actual, __expected,
            );
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
            ::defmt::debug!(
                "[test] {=str}: actual={:?} expected!={:?}",
                __name, __actual, __expected,
            );
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

    // ---- gt / ge / lt / le <expr> -----------------------------------------
    ($runner:expr; $name:expr => $value:expr, gt $expected:expr; $($rest:tt)*) => {
        $crate::__expect_cmp!($runner; $name => $value, > $expected, "> "; $($rest)*);
    };
    ($runner:expr; $name:expr => $value:expr, ge $expected:expr; $($rest:tt)*) => {
        $crate::__expect_cmp!($runner; $name => $value, >= $expected, ">="; $($rest)*);
    };
    ($runner:expr; $name:expr => $value:expr, lt $expected:expr; $($rest:tt)*) => {
        $crate::__expect_cmp!($runner; $name => $value, < $expected, "< "; $($rest)*);
    };
    ($runner:expr; $name:expr => $value:expr, le $expected:expr; $($rest:tt)*) => {
        $crate::__expect_cmp!($runner; $name => $value, <= $expected, "<="; $($rest)*);
    };

    // ---- ok_gt / ok_ge / ok_lt / ok_le <expr> -----------------------------
    ($runner:expr; $name:expr => $value:expr, ok_gt $expected:expr; $($rest:tt)*) => {
        $crate::__expect_ok_cmp!($runner; $name => $value, > $expected, "> "; $($rest)*);
    };
    ($runner:expr; $name:expr => $value:expr, ok_ge $expected:expr; $($rest:tt)*) => {
        $crate::__expect_ok_cmp!($runner; $name => $value, >= $expected, ">="; $($rest)*);
    };
    ($runner:expr; $name:expr => $value:expr, ok_lt $expected:expr; $($rest:tt)*) => {
        $crate::__expect_ok_cmp!($runner; $name => $value, < $expected, "< "; $($rest)*);
    };
    ($runner:expr; $name:expr => $value:expr, ok_le $expected:expr; $($rest:tt)*) => {
        $crate::__expect_ok_cmp!($runner; $name => $value, <= $expected, "<="; $($rest)*);
    };

    // ---- in_range <range> --------------------------------------------------
    ($runner:expr; $name:expr => $value:expr, in_range $range:expr; $($rest:tt)*) => {
        {
            let __name: &str = $name;
            let __actual = $value;
            let __range = $range;
            let __ok = __range.contains(&__actual);
            ::defmt::debug!(
                "[test] {=str}: actual={:?} expected in {=str}",
                __name, __actual, stringify!($range),
            );
            if !__ok {
                ::defmt::error!(
                    "[test] FAIL detail: {=str}: actual={:?} out of range {=str}",
                    __name, __actual, stringify!($range),
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
            ::defmt::debug!(
                "[test] {=str}: actual={:?} expected Ok(_)",
                __name, __actual,
            );
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
            ::defmt::debug!(
                "[test] {=str}: actual={:?} expected Err(_)",
                __name, __actual,
            );
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
            ::defmt::debug!(
                "[test] {=str}: actual={:?} expected Ok({:?})",
                __name, __actual, __expected,
            );
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

    // ---- ok_in_range <range> -----------------------------------------------
    ($runner:expr; $name:expr => $value:expr, ok_in_range $range:expr; $($rest:tt)*) => {
        {
            let __name: &str = $name;
            let __actual = $value;
            let __range = $range;
            let __ok = match &__actual {
                Ok(__v) => __range.contains(__v),
                Err(_) => false,
            };
            ::defmt::debug!(
                "[test] {=str}: actual={:?} expected Ok(in {=str})",
                __name, __actual, stringify!($range),
            );
            if !__ok {
                ::defmt::error!(
                    "[test] FAIL detail: {=str}: actual={:?} not Ok(in {=str})",
                    __name, __actual, stringify!($range),
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
/// * `<method>(<args>) ~> <field> => <verb> <verb-args>;`
///   Infallible field projection (no `Result` unwrap). Names the row
///   `"<method>()~><field>"` and applies the verb to `value.<field>`.
///
/// * `<method>(<args>) ~> |<v>| <projection-expr> => <verb> <verb-args>;`
///   Infallible closure projection. Names the row after the projection
///   expression.
///
/// Each verb is one of the verbs supported by [`expect!`].
#[doc(hidden)]
pub mod _service_dsl_docs {}

/// Tests for any service handle, async or sync.
///
/// The first token of the body selects the calling convention:
///
/// * `embedded_service_test!(async; runner, svc; { ... })` — every method call is
///   `svc.method(args).await`. Use for traits whose methods return
///   futures.
/// * `embedded_service_test!(sync;  runner, svc; { ... })` — every method call is
///   `svc.method(args)`. Use for traits whose methods are plain
///   synchronous functions.
///
/// The row syntax inside `{ ... }` is identical for both modes — see the
/// [module docs][crate] for the full list of row shapes.
///
/// The enclosing function must be `async` in both modes (the `sync` mode
/// still allows `sleep <dur>;` rows, which `.await` a timer).
///
/// # Example
///
/// ```ignore
/// embedded_service_test!(async; runner, battery; {
///     battery_info(BAT)                              => ( is_ok );
///     battery_info(BAT) -> design_voltage            => ( ok_eq 12_000u32 );
/// });
///
/// embedded_service_test!(sync; runner, time_alarm; {
///     get_capabilities() ~> |c| c.ac_wake_implemented() => ( eq true );
/// });
/// ```
#[macro_export]
macro_rules! embedded_service_test {
    (async; $runner:expr, $svc:expr; { $($body:tt)* }) => {
        $crate::__svc_async_tests!($runner, $svc; $($body)*);
    };
    (sync;  $runner:expr, $svc:expr; { $($body:tt)* }) => {
        $crate::__svc_sync_tests!($runner, $svc; $($body)*);
    };
}

/// Tests for a [`battery_service_interface::BatteryService`] handle.
///
/// Thin alias for `embedded_service_test!(async; ...)`. See the [module
/// docs][crate] for row syntax.
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

    // let name = try method(args);
    // Fallible capture: records pass/fail on Ok-ness and on success binds
    // `name` to the inner Ok value. On Err, records the row and returns
    // from the enclosing fn so subsequent rows are skipped.
    ($runner:expr, $svc:expr;
     let $name:ident = try $method:ident ( $($args:tt)* ); $($rest:tt)*) => {
        let $name = match $svc.$method($($args)*).await {
            Ok(__v) => {
                $runner.record(
                    concat!("let ", stringify!($name), " = try ", stringify!($method($($args)*))),
                    true,
                );
                __v
            }
            Err(_) => {
                $runner.record(
                    concat!("let ", stringify!($name), " = try ", stringify!($method($($args)*))),
                    false,
                );
                return;
            }
        };
        $crate::__svc_async_tests!($runner, $svc; $($rest)*);
    };

    // let name = method(args);
    // Infallible capture: binds `name` to the return value verbatim and
    // records a passing row. Use for methods that don't return Result.
    ($runner:expr, $svc:expr;
     let $name:ident = $method:ident ( $($args:tt)* ); $($rest:tt)*) => {
        let $name = $svc.$method($($args)*).await;
        $runner.record(
            concat!("let ", stringify!($name), " = ", stringify!($method($($args)*))),
            true,
        );
        $crate::__svc_async_tests!($runner, $svc; $($rest)*);
    };

    // sleep <duration-expr>;
    ($runner:expr, $svc:expr;
     sleep $dur:expr; $($rest:tt)*) => {
        ::embassy_time::Timer::after($dur).await;
        $crate::__svc_async_tests!($runner, $svc; $($rest)*);
    };

    // method(args) ~> |v| projection => (verb vargs);   -- infallible
    ($runner:expr, $svc:expr;
     $method:ident ( $($args:tt)* ) ~> |$v:ident| $proj:expr => ( $($vargs:tt)+ ); $($rest:tt)*) => {
        $crate::expect!($runner; {
            concat!(stringify!($method), "(..) ~> ", stringify!($proj))
                => { let $v = $svc.$method($($args)*).await; $proj },
                $($vargs)+;
        });
        $crate::__svc_async_tests!($runner, $svc; $($rest)*);
    };

    // method(args) ~> field => (verb vargs);            -- infallible
    ($runner:expr, $svc:expr;
     $method:ident ( $($args:tt)* ) ~> $field:ident => ( $($vargs:tt)+ ); $($rest:tt)*) => {
        $crate::expect!($runner; {
            concat!(stringify!($method), "()~>", stringify!($field))
                => $svc.$method($($args)*).await.$field,
                $($vargs)+;
        });
        $crate::__svc_async_tests!($runner, $svc; $($rest)*);
    };

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

    // let name = try method(args);
    // Sync fallible variant — same semantics as the async one but without
    // `.await`. The enclosing fn must still be async so that `sleep` rows
    // can `await` on `embassy_time::Timer`.
    ($runner:expr, $svc:expr;
     let $name:ident = try $method:ident ( $($args:tt)* ); $($rest:tt)*) => {
        let $name = match $svc.$method($($args)*) {
            Ok(__v) => {
                $runner.record(
                    concat!("let ", stringify!($name), " = try ", stringify!($method($($args)*))),
                    true,
                );
                __v
            }
            Err(_) => {
                $runner.record(
                    concat!("let ", stringify!($name), " = try ", stringify!($method($($args)*))),
                    false,
                );
                return;
            }
        };
        $crate::__svc_sync_tests!($runner, $svc; $($rest)*);
    };

    // let name = method(args);
    // Sync infallible variant.
    ($runner:expr, $svc:expr;
     let $name:ident = $method:ident ( $($args:tt)* ); $($rest:tt)*) => {
        let $name = $svc.$method($($args)*);
        $runner.record(
            concat!("let ", stringify!($name), " = ", stringify!($method($($args)*))),
            true,
        );
        $crate::__svc_sync_tests!($runner, $svc; $($rest)*);
    };

    // sleep <duration-expr>;
    ($runner:expr, $svc:expr;
     sleep $dur:expr; $($rest:tt)*) => {
        ::embassy_time::Timer::after($dur).await;
        $crate::__svc_sync_tests!($runner, $svc; $($rest)*);
    };

    // method(args) ~> |v| projection => (verb vargs);   -- infallible
    ($runner:expr, $svc:expr;
     $method:ident ( $($args:tt)* ) ~> |$v:ident| $proj:expr => ( $($vargs:tt)+ ); $($rest:tt)*) => {
        $crate::expect!($runner; {
            concat!(stringify!($method), "(..) ~> ", stringify!($proj))
                => { let $v = $svc.$method($($args)*); $proj },
                $($vargs)+;
        });
        $crate::__svc_sync_tests!($runner, $svc; $($rest)*);
    };

    // method(args) ~> field => (verb vargs);            -- infallible
    ($runner:expr, $svc:expr;
     $method:ident ( $($args:tt)* ) ~> $field:ident => ( $($vargs:tt)+ ); $($rest:tt)*) => {
        $crate::expect!($runner; {
            concat!(stringify!($method), "()~>", stringify!($field))
                => $svc.$method($($args)*).$field,
                $($vargs)+;
        });
        $crate::__svc_sync_tests!($runner, $svc; $($rest)*);
    };

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

// ---- comparison-verb helpers ----------------------------------------------
//
// `__expect_cmp!` / `__expect_ok_cmp!` factor out the boilerplate for the
// gt/ge/lt/le and ok_gt/ok_ge/ok_lt/ok_le verbs so each variant in
// `__expect_inner!` is a one-liner. `$op` is the literal comparison token
// passed straight through, and `$op_str` is its printable form for log
// messages.

#[macro_export]
#[doc(hidden)]
macro_rules! __expect_cmp {
    ($runner:expr; $name:expr => $value:expr, $op:tt $expected:expr, $op_str:expr; $($rest:tt)*) => {
        {
            let __name: &str = $name;
            let __actual = $value;
            let __expected = $expected;
            let __ok = __actual $op __expected;
            ::defmt::debug!(
                "[test] {=str}: actual={:?} expected {=str} {:?}",
                __name, __actual, $op_str, __expected,
            );
            if !__ok {
                ::defmt::error!(
                    "[test] FAIL detail: {=str}: actual={:?} not {=str} {:?}",
                    __name, __actual, $op_str, __expected,
                );
            }
            $runner.record(__name, __ok);
        }
        $crate::__expect_inner!($runner; $($rest)*);
    };
}

#[macro_export]
#[doc(hidden)]
macro_rules! __expect_ok_cmp {
    ($runner:expr; $name:expr => $value:expr, $op:tt $expected:expr, $op_str:expr; $($rest:tt)*) => {
        {
            let __name: &str = $name;
            let __actual = $value;
            let __expected = $expected;
            let __ok = match &__actual {
                Ok(__v) => *__v $op __expected,
                Err(_) => false,
            };
            ::defmt::debug!(
                "[test] {=str}: actual={:?} expected Ok({=str} {:?})",
                __name, __actual, $op_str, __expected,
            );
            if !__ok {
                ::defmt::error!(
                    "[test] FAIL detail: {=str}: actual={:?} not Ok({=str} {:?})",
                    __name, __actual, $op_str, __expected,
                );
            }
            $runner.record(__name, __ok);
        }
        $crate::__expect_inner!($runner; $($rest)*);
    };
}
