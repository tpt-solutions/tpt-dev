//! `machine!`: a declarative front end for [`Machine`](crate::Machine).

/// Builds a [`Machine`](crate::Machine) from a short description.
///
/// The statement binds the machine to the name given first, and binds each
/// variable and state to its own name in the surrounding scope. Variables and
/// states are declared in the order listed, and transitions are added last.
///
/// A guard is one comparison between a declared variable and an integer
/// expression, such as `when x > 0`. Comparisons are `<`, `<=`, `>`, `>=`,
/// `==` and `!=`. A transition with no `when` clause is always enabled.
///
/// A guard whose constant cannot be encoded in the solver's range becomes a
/// guard that is never enabled. The analysis then reports a deadlock where a
/// guard was dropped, so the result errs toward reporting a deadlock and never
/// misses one.
///
/// # Examples
///
/// ```
/// use tpt_dev_automata::machine;
///
/// machine! {
///     m {
///         vars: [x];
///         states: [idle, busy];
///         finals: [idle];
///         transitions: [
///             idle -> busy : "start" when x > 0;
///             busy -> idle : "stop" when x <= 0;
///             busy -> busy : "tick" when x > 0;
///         ];
///     }
/// }
///
/// let report = m.analyze(idle).unwrap();
/// assert!(report.is_deadlock_free());
/// ```
///
/// A machine that is missing a guard is reported as deadlocking:
///
/// ```
/// use tpt_dev_automata::{DeadlockKind, machine};
///
/// machine! {
///     m {
///         vars: [x];
///         states: [idle, busy];
///         finals: [];
///         transitions: [
///             idle -> busy : "start" when x > 0;
///             busy -> idle : "stop" when x < 0;
///         ];
///     }
/// }
///
/// let report = m.analyze(idle).unwrap();
/// assert_eq!(report.deadlocks.len(), 2);
/// assert!(matches!(report.deadlocks[0].kind, DeadlockKind::GuardsIncomplete { .. }));
/// ```
#[macro_export]
macro_rules! machine {
    (
        $m:ident {
            vars: [$($var:ident),* $(,)?];
            states: [$($state:ident),+ $(,)?];
            finals: [$($fin:ident),* $(,)?];
            transitions: [
                $($from:ident -> $to:ident : $event:literal
                    $(when $gv:ident $op:tt $gr:expr)? ;)*
            ];
        }
    ) => {
        #[allow(unused_mut)]
        let mut $m = $crate::Machine::new();
        $(
            #[allow(unused_variables)]
            let $var = $m.declare(::core::stringify!($var));
        )*
        $(
            #[allow(unused_variables)]
            let $state = $m.state(::core::stringify!($state));
        )+
        $( $m.mark_final($fin); )*
        $(
            {
                let guard: ::core::option::Option<$crate::Formula> =
                    ::core::option::Option::None
                    $(.or(::core::option::Option::Some(
                        $crate::__automata_guard!($m, $gv, $op, $gr)
                    )))?;
                // Every state here comes from this machine, so the transition
                // cannot name an unknown state.
                let _ = $m.transition($from, $to, $event, guard);
            }
        )*
    };
}

/// Builds the formula for one guard comparison. Used by [`machine!`].
#[doc(hidden)]
#[macro_export]
macro_rules! __automata_guard {
    ($m:ident, $v:ident, <, $r:expr) => {
        $crate::__automata_guard!(@cmp $m, $v, lt, $r)
    };
    ($m:ident, $v:ident, <=, $r:expr) => {
        $crate::__automata_guard!(@cmp $m, $v, le, $r)
    };
    ($m:ident, $v:ident, >, $r:expr) => {
        $crate::__automata_guard!(@cmp $m, $v, gt, $r)
    };
    ($m:ident, $v:ident, >=, $r:expr) => {
        $crate::__automata_guard!(@cmp $m, $v, ge, $r)
    };
    ($m:ident, $v:ident, ==, $r:expr) => {
        $crate::__automata_guard!(@cmp $m, $v, eq, $r)
    };
    ($m:ident, $v:ident, !=, $r:expr) => {{
        let eq = $m.arena_mut().eq(
            &$crate::Linear::var($v),
            &$crate::Linear::constant($crate::Rational::from_int($r)),
        );
        match eq {
            ::core::option::Option::Some(f) => $m.arena_mut().not(f),
            ::core::option::Option::None => $m.arena_mut().constant(false),
        }
    }};
    (@cmp $m:ident, $v:ident, $method:ident, $r:expr) => {{
        let f = $m.arena_mut().$method(
            &$crate::Linear::var($v),
            &$crate::Linear::constant($crate::Rational::from_int($r)),
        );
        match f {
            ::core::option::Option::Some(f) => f,
            ::core::option::Option::None => $m.arena_mut().constant(false),
        }
    }};
}
