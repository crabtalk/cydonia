//! Jobs on keyed lanes: one key's in the order queued, other keys beside them.

use std::{
    sync::{Arc, Mutex, mpsc},
    time::Duration,
};

use cydonia_cupola::lanes::Lanes;

const PATIENCE: Duration = Duration::from_secs(5);

#[test]
fn one_keys_jobs_run_in_the_order_queued() {
    let lanes = Lanes::default();
    let ran = Arc::new(Mutex::new(Vec::new()));
    let jobs: Vec<_> = (0..100)
        .map(|n| {
            let ran = ran.clone();
            lanes.run("a", move || ran.lock().unwrap().push(n))
        })
        .collect();
    for job in futures::executor::block_on(futures::future::join_all(jobs)) {
        job.unwrap();
    }
    assert_eq!(*ran.lock().unwrap(), (0..100).collect::<Vec<_>>());
}

#[test]
fn a_held_lane_does_not_hold_another() {
    let lanes = Lanes::default();
    let (release, held) = mpsc::channel::<()>();
    let slow = lanes.run("a", move || held.recv_timeout(PATIENCE).is_ok());
    let queued = lanes.run("a", || "after");
    let (done, other) = mpsc::channel();
    let _other = lanes.run("b", move || done.send(()).unwrap());
    other.recv_timeout(PATIENCE).unwrap();
    release.send(()).unwrap();
    assert!(futures::executor::block_on(slow).unwrap());
    assert_eq!(futures::executor::block_on(queued).unwrap(), "after");
}

#[test]
fn a_panicking_job_fails_alone() {
    let lanes = Lanes::default();
    let panicked = lanes.run("a", || -> i32 { panic!("on purpose") });
    let next = lanes.run("a", || 1);
    assert!(futures::executor::block_on(panicked).is_err());
    assert_eq!(futures::executor::block_on(next).unwrap(), 1);
}

#[test]
fn a_job_on_two_keys_waits_for_both_and_holds_both() {
    let lanes = Lanes::default();
    let ran = Arc::new(Mutex::new(Vec::new()));
    let (release, held) = mpsc::channel::<()>();
    let push = |name: &'static str| {
        let ran = ran.clone();
        move || ran.lock().unwrap().push(name)
    };
    let first = lanes.run("b", {
        let push = push("b first");
        move || {
            held.recv_timeout(PATIENCE).unwrap();
            push();
        }
    });
    let both = lanes.run_on(["a", "b"], push("both"));
    let after = lanes.run("a", push("a after"));
    release.send(()).unwrap();
    let (first, both, after) =
        futures::executor::block_on(futures::future::join3(first, both, after));
    first.unwrap();
    both.unwrap();
    after.unwrap();
    assert_eq!(*ran.lock().unwrap(), ["b first", "both", "a after"]);
}

#[test]
fn jobs_on_the_same_keys_in_opposite_orders_all_run() {
    let lanes = Lanes::default();
    let jobs: Vec<_> = (0..50)
        .map(|n| {
            let keys = match n % 2 {
                0 => ["a", "b"],
                _ => ["b", "a"],
            };
            lanes.run_on(keys, move || n)
        })
        .collect();
    let ran: Vec<_> = futures::executor::block_on(futures::future::join_all(jobs))
        .into_iter()
        .map(Result::unwrap)
        .collect();
    assert_eq!(ran, (0..50).collect::<Vec<_>>());
}
