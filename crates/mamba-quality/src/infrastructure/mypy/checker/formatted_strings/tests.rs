//! Exercise the real Checker observation slot and its private Drop guard.
use super::{
    types::{ObservedExpressionTypes, Type},
    Observations,
};
use std::{
    collections::HashMap,
    panic::{catch_unwind, AssertUnwindSafe},
    sync::{Arc, Mutex},
};

fn values(value: Option<Type>) -> ObservedExpressionTypes {
    Arc::new(Mutex::new(HashMap::from([(1, value)])))
}

fn assert_current(observations: &Observations, expected: Option<&ObservedExpressionTypes>) {
    match (observations.current(), expected) {
        (None, None) => {}
        (Some(actual), Some(expected)) => assert!(
            Arc::ptr_eq(&actual, expected),
            "restore the exact Checker map"
        ),
        _ => panic!("Checker observation slot state differs"),
    }
}

#[test]
fn checker_frame_restores_inactive_after_normal_drop() {
    let observations = Observations::default();
    let frame = values(None);
    assert_current(&observations, None);
    {
        let _guard = observations.scope(Some(frame.clone()));
        assert_current(&observations, Some(&frame));
        assert!(
            observations.0.try_borrow_mut().is_ok(),
            "scope guard must not hold a RefCell borrow"
        );
    }
    assert_current(&observations, None);
}

#[test]
fn checker_nested_frames_and_suspension_restore_their_prior_owner() {
    let observations = Observations::default();
    let outer = values(Some(Type::Bytes));
    let inner = values(None);
    let outer_guard = observations.scope(Some(outer.clone()));
    {
        let _inner_guard = observations.scope(Some(inner.clone()));
        assert_current(&observations, Some(&inner));
        assert_eq!(inner.lock().expect("inner map").get(&1), Some(&None));
    }
    assert_current(&observations, Some(&outer));
    {
        let _suspension = observations.scope(None);
        assert_current(&observations, None);
    }
    assert_current(&observations, Some(&outer));
    drop(outer_guard);
    assert_current(&observations, None);
}

fn early_none_result(observations: &Observations) -> Option<Type> {
    let _inner_guard = observations.scope(Some(values(None)));
    None
}

#[test]
fn checker_frame_restores_after_early_none() {
    let observations = Observations::default();
    let outer = values(Some(Type::Bytes));
    let _outer_guard = observations.scope(Some(outer.clone()));
    let result = early_none_result(&observations);
    assert_eq!(result, None);
    assert_current(&observations, Some(&outer));
}

#[test]
fn checker_frame_restores_after_mutation_and_controlled_panic() {
    let observations = Observations::default();
    let outer = values(Some(Type::Bytes));
    let inner = values(None);
    let replacement = values(Some(Type::Str));
    let outer_guard = observations.scope(Some(outer.clone()));
    let failure = catch_unwind(AssertUnwindSafe(|| {
        let _inner_guard = observations.scope(Some(inner.clone()));
        assert_current(&observations, Some(&inner));
        observations.0.replace(Some(replacement.clone()));
        assert_current(&observations, Some(&replacement));
        panic!("controlled Checker observation panic");
    }));
    assert!(failure.is_err());
    assert_current(&observations, Some(&outer));
    assert!(observations.0.try_borrow_mut().is_ok());
    drop(outer_guard);
    assert_current(&observations, None);
}
