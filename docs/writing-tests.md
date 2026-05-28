# Writing Tests with the `expect!` DSL

Audience: contributors (human or LLM) adding on-target self-tests for
services in this repository.

This repository is `#![no_std]` / `#![no_main]` embedded firmware. There is
no `cargo test` and no host harness. All tests are **on-target self-tests**
that run inside the firmware itself, log results via `defmt` over RTT, and
are gated behind the `test-runner` cargo feature on each platform binary.

The test harness lives in `platform-common` and consists of:

- A minimal [`TestRunner`](../platform/embedded-service-test/src/lib.rs) that
  counts passes/failures and prints a summary line.
- A generic [`expect!`](../platform/embedded-service-test/src/lib.rs) macro that
  evaluates an expression and applies a *verb* against it (e.g. `eq`,
  `ok_in_range`).
- A generic [`embedded_service_test!`](../platform/embedded-service-test/src/lib.rs)
  macro that works for **any** service trait (async or sync) and
  auto-generates the row name from the method call. Thin per-service
  aliases (`battery_tests!`, `sensor_tests!`, `fan_tests!`,
  `time_alarm_tests!`) exist for ergonomics but are not required — prefer
  `embedded_service_test!` for new tests.

The reference suite at
[platform/platform-common/src/mock/test_runner.rs](../platform/platform-common/src/mock/test_runner.rs)
is the canonical example. Read it before adding new tests.

---

## 1. Where to put tests

| Goal | Location |
|---|---|
| Exercise a mock service against its trait | `platform-common/src/mock/test_runner.rs` |
| Exercise a real driver/service on a board | A new module under the platform crate (e.g. `platform/dev-imxrt/src/selftest.rs`), spawned from `main.rs` when `--features test-runner` is set |

Tests must compile and link into the firmware binary, so:

- No `std`, no `alloc`, no `#[test]`, no `#[cfg(test)]`.
- All allocations are static (`StaticCell`, `OnceLock`).
- All logging goes through `defmt`.

## 2. Entry point pattern

```rust
use defmt::info;
use embassy_time::{Duration, Timer};
use embedded_service_test::TestRunner;

pub async fn run(services: MyServices) -> ! {
    // Let async services settle (poll loops, caches, state machines).
    Timer::after(Duration::from_secs(3)).await;

    info!("[test] starting test suite");
    let mut runner = TestRunner::new();

    test_subsystem_a(&services, &mut runner).await;
    test_subsystem_b(&services, &mut runner).await;

    runner.summary();   // prints "[test] passed: X, failed: Y"

    // Idle forever — never return from a test entry point.
    loop { Timer::after(Duration::from_secs(60)).await; }
}
```

Each `test_*` function takes `&mut TestRunner` and writes rows to it via
one of the DSL macros.

## 3. The `expect!` macro (generic form)

Use `expect!` directly when no service-specific wrapper applies (e.g.
when calling a free function, or composing several calls into one
expression).

```rust
use embedded_service_test::expect;

expect!(runner; {
    "fan max rpm"  => fan.max_rpm().await,                       eq 6000u16;
    "sensor temp"  => sensor.temperature_immediate().await,      ok_in_range 20.0f32..=40.0;
    "voltage band" => battery.battery_status(BAT).await
                          .map(|b| b.battery_present_voltage),   ok_in_range 11_000u32..=13_500;
});
```

Statement shape: `"name" => <value-expr>, <verb> <verb-args>;`

- `"name"` is any `&str` expression (typically a literal).
- `<value-expr>` is any expression. It is evaluated once.
- `<verb>` and its args follow — see the verb table below.
- Each row terminates with `;`.

## 4. Verbs

These verbs are accepted by `expect!` and by every service-specific
wrapper (inside the `( … )` group on each row).

| Verb                  | Passes when                                     |
|-----------------------|-------------------------------------------------|
| `eq <expr>`           | `actual == expr`                                |
| `ne <expr>`           | `actual != expr`                                |
| `gt <expr>`           | `actual >  expr`                                |
| `ge <expr>`           | `actual >= expr`                                |
| `lt <expr>`           | `actual <  expr`                                |
| `le <expr>`           | `actual <= expr`                                |
| `in_range <range>`    | `range.contains(&actual)`                       |
| `is_ok`               | `actual.is_ok()`                                |
| `is_err`              | `actual.is_err()`                               |
| `ok_eq <expr>`        | `actual == Ok(expr)`                            |
| `ok_gt <expr>`        | `actual` is `Ok(v)` and `v >  expr`             |
| `ok_ge <expr>`        | `actual` is `Ok(v)` and `v >= expr`             |
| `ok_lt <expr>`        | `actual` is `Ok(v)` and `v <  expr`             |
| `ok_le <expr>`        | `actual` is `Ok(v)` and `v <= expr`             |
| `ok_in_range <range>` | `actual` is `Ok(v)` and `range.contains(&v)`    |

Rules of thumb:

- **No predicate verb exists.** If you need to check a boolean accessor on
  a bitfield, project to it and compare with `eq true` / `eq false` (see
  §5, `~>` projection).
- Prefer the `ok_*` verbs to `is_ok` + a separate comparison — they share
  one row and produce a single, more informative failure message.
- Use range verbs for any value that has natural tolerance (sensor reads,
  rpm round-trips, elapsed-time deltas). Avoid `eq` on floats unless the
  value is constructed deterministically.

## 5. Service-specific row syntax

The generic [`embedded_service_test!`] macro (and the legacy per-service aliases)
accepts rows that name the method directly. The macro inserts the service
handle and `.await` (for `async;` mode), and uses `stringify!(method(args))`
as the row name.

Pick the mode based on whether the trait's methods are `async`:

```rust
use embedded_service_test::embedded_service_test;

// Async trait: every method call gets `.await`
embedded_service_test!(async; runner, battery; {
    battery_info(BAT)                              => ( is_ok );
    battery_info(BAT) -> design_voltage            => ( ok_eq 12_000u32 );
    battery_status(BAT) -> battery_present_voltage => ( ok_in_range 11_000u32..=13_500 );
    battery_status(BAT) -> |b| b.battery_present_rate
                                                   => ( ok_le 5_000u32 );
});

// Sync trait: methods are plain functions
embedded_service_test!(sync; runner, time_alarm; {
    get_capabilities() ~> |c| c.ac_wake_implemented() => ( eq true );
    get_real_time()                                   => ( is_ok );
});
```

The macro is **generic** — it works for any service trait. You do not need
to add a per-service wrapper to test a new trait; just call
`embedded_service_test!` with the appropriate mode.

Wrap the verb in parentheses so the macro parser can find the row
boundary:

```rust
embedded_service_test!(async; runner, battery; {
    battery_info(BAT)                              => ( is_ok );
    battery_info(BAT) -> design_voltage            => ( ok_eq 12_000u32 );
    battery_status(BAT) -> battery_present_voltage => ( ok_in_range 11_000u32..=13_500 );
    battery_status(BAT) -> |b| b.battery_present_rate
                                                   => ( ok_le 5_000u32 );
});
```

### Row shapes

| Shape | Meaning |
|---|---|
| `method(args) => ( verb args );` | Apply verb to the call's return value. |
| `method(args) -> field => ( verb args );` | Call returns `Result<T,_>`; apply verb to `Ok(v.field)`. |
| `method(args) -> \|v\| expr => ( verb args );` | Call returns `Result<T,_>`; apply verb to `Ok(<expr with v bound to inner>)`. |
| `method(args) ~> field => ( verb args );` | **Infallible** field projection. Call returns `T` (not `Result`); apply verb to `value.field`. |
| `method(args) ~> \|v\| expr => ( verb args );` | **Infallible** closure projection. Use for bitfield accessor methods. |
| `let name = method(args);` | Infallible capture — binds `name` to the return value, records a passing row. |
| `let name = try method(args);` | Fallible capture — on `Ok(inner)` binds `name = inner`; on `Err` records the row and **`return`s** from the enclosing fn. |
| `sleep <duration-expr>;` | Expands to `embassy_time::Timer::after(<dur>).await;`. Requires the enclosing fn to be `async`. |

### `->` vs `~>` (critical distinction)

- `->` is for methods returning `Result`. The projection is applied inside
  `.map(|v| …)`. Compare with `ok_*` verbs.
- `~>` is for methods returning a plain value (not `Result`). The
  projection is applied directly. Compare with plain verbs (`eq`, `ne`,
  `lt`, …).

Picking the wrong arrow yields a compiler error like
`<Type> is not an iterator` or `expected Result, found <Type>`.

### `let … = try` captures

Use this when later rows depend on a value that may fail to read:

```rust
embedded_service_test!(sync; runner, tas; {
    let first = try get_real_time();  // on Err: record row and return from fn

    sleep Duration::from_secs(2);

    get_real_time() -> |t| t.datetime.unix_timestamp()
                            .saturating_sub(first.datetime.unix_timestamp())
                                                => ( ok_ge 1 );
    set_real_time(first)                        => ( is_ok );
});
```

The captured binding (`first` above) is in scope for every row that
follows in the same macro invocation.

## 6. Writing a good row

- **One assertion per row.** Don't chain `&&` inside `eq`. Split into
  multiple rows so a failure pinpoints the broken assertion.
- **Cover the error path.** Issue at least one call with an invalid id /
  out-of-range argument and assert `is_err`.
- **Round-trip writes.** If a setter exists, follow it with the
  corresponding getter and a tolerance check.
- **Tolerate jitter.** For sampled values, use `in_range` or `ok_in_range`
  with a small window — never `eq`.
- **Use literal-typed integers** (`6000u16`, `12_000u32`, `1.0f32`) so
  type inference doesn't drift. Mismatches surface as
  `expected u32, found i32` from inside the macro expansion.
- **Name captures meaningfully** — `let cached = temperature();` is good;
  `let x = …;` is not.

## 7. Behaviour over time

To assert that something *changes* (or *doesn't change*) between two
reads, use `let` + `sleep` + a comparison against the captured value:

```rust
embedded_service_test!(async; runner, sensor; {
    disable_sampling()                  => ( eq () );
    sleep Duration::from_millis(600);
    let cached = temperature();
    sleep Duration::from_millis(600);
    temperature()                       => ( eq cached );   // unchanged

    enable_sampling()                   => ( eq () );
    sleep Duration::from_millis(600);
    temperature()                       => ( ne cached );   // changed
});
```

Compare against the captured variable directly with `eq` / `ne` — the
failure log prints both the captured and the actual value.

## 8. Gating tests into a platform binary

Each platform crate exposes a `test-runner` cargo feature. Pattern in the
platform's `Cargo.toml`:

```toml
[features]
default = []
test-runner = []
```

In `main.rs`, gate the test entry point and the normal UART/service
plumbing on the feature so a feature-on build replaces the production
event loop with the self-test loop:

```rust
#[cfg(not(feature = "test-runner"))]
spawner.must_spawn(uart_task(/* … */));

#[cfg(feature = "test-runner")]
spawner.must_spawn(selftest_task(services));
```

Build & flash:

```sh
cd platform/<name>
cargo build --features test-runner
cargo run   --features test-runner --release
```

Watch the RTT log for the `[test]` lines and the final
`[test] passed: N, failed: M` summary.

## 9. Failure diagnostics

Every verb arm emits a `defmt::debug!` line on entry with the actual and
expected values, and a `defmt::error!` line on mismatch. Enable
`DEFMT_LOG=debug` (or per-crate) when iterating on a new test to see the
actual values without having to add temporary prints.

The runner's final `summary()` prints the running totals; the per-row
`record(name, ok)` log identifies each pass/fail by name.

## 10. Adding tests for a new service

The DSL is generic — you do **not** need to add a per-service wrapper
macro to test a new trait. Just call `embedded_service_test!` with the
appropriate mode:

```rust
use embedded_service_test::embedded_service_test;

embedded_service_test!(async; runner, my_service; {
    do_thing(42)         => ( is_ok );
    read_value()         => ( ok_in_range 0u32..=100 );
    read_value() -> kind => ( ok_eq Kind::Normal );
});
```

If you find yourself writing the same `embedded_service_test!(async; runner, foo; {...})`
boilerplate in many call sites, you can add a thin alias macro modelled on
the existing [`battery_tests!`](../platform/embedded-service-test/src/lib.rs)
definition — but for one-off tests this is unnecessary.

Do **not** add new verb arms unless an assertion shape is genuinely
missing from the table in §4 — prefer projections + existing verbs.

## 11. Checklist for an LLM adding tests

Before submitting:

- [ ] Every trait method on the service under test has at least one row.
- [ ] At least one row exercises an error path (`is_err`).
- [ ] Setters are followed by matching getters with a tolerance check.
- [ ] No `eq` on floats or sampled values; use `in_range` / `ok_in_range`.
- [ ] No `->` on infallible methods (use `~>`); no `~>` on `Result` methods.
- [ ] All captured `let` bindings are referenced by a later row.
- [ ] `runner.summary()` is called exactly once at the end of `run()`.
- [ ] The entry point is gated behind `--features test-runner` in the
      platform binary.
- [ ] `cargo build --features test-runner` succeeds on every affected
      platform crate.
