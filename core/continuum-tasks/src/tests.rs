use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use uuid::Uuid;

use crate::*;

#[derive(Default)]
struct Ends(Mutex<Vec<(TaskRow, TaskEnd)>>);
impl EndSink for Ends {
    fn ended(&self, row: &TaskRow, end: &TaskEnd) {
        self.0.lock().push((row.clone(), end.clone()));
    }
}

fn table() -> (Arc<TaskRegistry>, Arc<Ends>) {
    let registry = TaskRegistry::new();
    let ends = Arc::new(Ends::default());
    registry.add_sink(ends.clone());
    (registry, ends)
}

fn cmd() -> TaskKind {
    TaskKind::Command("code/cargo/test".to_string())
}

// what this catches: an end that reaches nobody, or reaches them twice. Ending delivers
// exactly one end to the sink, with the owner recorded at start, and leaves the table.
#[test]
fn an_end_is_delivered_once_to_its_recorded_owner_and_leaves_the_table() {
    let (registry, ends) = table();
    let aris = Owner(Uuid::new_v4());
    let (guard, _watch) = registry.start(Some(aris), cmd());
    assert_eq!(registry.list(Some(aris)).len(), 1);
    guard.end(TaskEnd::Done(serde_json::json!({"passed": 148})));
    let ends = ends.0.lock();
    assert_eq!(ends.len(), 1);
    assert_eq!(ends[0].0.owner, Some(aris));
    assert!(ends[0].1.succeeded());
    assert!(registry.list(None).is_empty(), "an ended task leaves the table");
}

// what this catches: the silent paths. A runner that panics, or drops its guard without an
// end (an aborted future, a dead process), still ends the task, as Lost, exactly once.
#[test]
fn a_panic_or_a_dropped_runner_ends_the_task_as_lost() {
    let (registry, ends) = table();
    let (guard, _w1) = registry.start(None, cmd());
    let panicked = std::thread::spawn(move || {
        let _held = guard;
        panic!("the build script panicked");
    })
    .join();
    assert!(panicked.is_err());
    let (dropped, _w2) = registry.start(None, cmd());
    drop(dropped);
    let ends = ends.0.lock();
    assert_eq!(ends.len(), 2);
    assert!(matches!(&ends[0].1, TaskEnd::Lost(why) if why.contains("panicked")));
    assert!(matches!(&ends[1].1, TaskEnd::Lost(why) if why.contains("without reporting")));
}

// what this catches: a stop anyone can send, or one its runner never sees. The owner's stop
// reaches the runner; another owner's stop is refused; a stop of an ended task says so.
#[test]
fn only_the_owner_or_the_substrate_stops_a_task_and_the_runner_sees_it() {
    let (registry, ends) = table();
    let (aiko, benchy) = (Owner(Uuid::new_v4()), Owner(Uuid::new_v4()));
    let (guard, _watch) = registry.start(Some(aiko), cmd());
    let id = guard.id();
    let stop = guard.stop_requested();
    assert_eq!(registry.stop(id, Some(benchy)), Err(StopRefused::NotOwner(id)));
    assert!(!*stop.borrow());
    assert_eq!(registry.stop(id, Some(aiko)), Ok(()));
    assert!(*stop.borrow(), "the runner sees the request");
    guard.end(TaskEnd::Stopped);
    assert_eq!(registry.stop(id, None), Err(StopRefused::NotRunning(id)));
    assert_eq!(ends.0.lock()[0].1, TaskEnd::Stopped);
}

// what this catches: the act's window becoming the work's limit. Inside the window the act
// gets the end inline; past it the act gets the handle, the work keeps running, and its end
// still reaches the sink.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn the_window_bounds_the_wait_never_the_work() {
    let (registry, ends) = table();
    let (quick, mut quick_watch) = registry.start(None, cmd());
    quick.end(TaskEnd::Done(serde_json::json!("ok")));
    assert_eq!(quick_watch.land(Duration::from_secs(1)).await, Landing::Landed(TaskEnd::Done(serde_json::json!("ok"))));

    let (slow, mut slow_watch) = registry.start(None, cmd());
    let id = slow.id();
    let runner = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(600)).await;
        slow.end(TaskEnd::Failed("exit 101".to_string()));
    });
    assert_eq!(slow_watch.land(Duration::from_secs(30)).await, Landing::Running(id));
    assert_eq!(registry.list(None).len(), 1, "still running past the act's window");
    runner.await.expect("runner");
    assert_eq!(ends.0.lock().len(), 2);
    assert_eq!(ends.0.lock()[1].1, TaskEnd::Failed("exit 101".to_string()));
}
