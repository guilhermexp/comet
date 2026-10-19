//! Scaled adversarial inputs: never use the exponential/OOM demonstration sizes.
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::{
    Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use zeron_visual::{Visual, check};

struct Meter;
static TRACK: AtomicBool = AtomicBool::new(false);
static ALLOCATED: AtomicUsize = AtomicUsize::new(0);
static SERIAL: Mutex<()> = Mutex::new(());
#[global_allocator]
static ALLOCATOR: Meter = Meter;
unsafe impl GlobalAlloc for Meter {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if TRACK.load(Ordering::Relaxed) {
            ALLOCATED.fetch_add(layout.size(), Ordering::Relaxed);
        }
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if TRACK.load(Ordering::Relaxed) {
            ALLOCATED.fetch_add(size, Ordering::Relaxed);
        }
        unsafe { System.realloc(ptr, layout, size) }
    }
}
fn measured(script: &str) -> (Result<zeron_visual::Node, zeron_visual::VisualError>, usize) {
    ALLOCATED.store(0, Ordering::Relaxed);
    TRACK.store(true, Ordering::Relaxed);
    let result = Visual::load(script, 1).and_then(|mut v| v.render());
    TRACK.store(false, Ordering::Relaxed);
    (result, ALLOCATED.load(Ordering::Relaxed))
}

#[test]
fn native_empty_dag_is_bounded_without_rejecting_normal_nested_lists() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    // ~262k visits before the repair, never the reported depth-45 payload.
    let error = check(
        "local t={false,false}; for i=1,16 do t={t,t} end; return t",
        1,
    )
    .unwrap_err();
    assert!(error.message.contains("work budget"), "{error:?}");
    check(
        "local t={false, ui.text {'ok'}}; return {t, {nil, false}, t}",
        1,
    )
    .unwrap();
}

#[test]
fn native_span_copies_are_bounded_before_concatenation() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    // VM string is only 32 KiB; former native copies total ~32 MiB.
    let (result, allocated) =
        measured("local s=string.rep('x',32768); return ui.text {ui.span(table.create(512,s))}");
    assert!(result.is_err());
    assert!(
        allocated < 8 * 1024 * 1024,
        "native allocated {allocated} bytes"
    );
    check(
        "return ui.text { 'value ', ui.span {42, true, nil, weight=600}, false }",
        1,
    )
    .unwrap();
}

#[test]
fn native_print_has_line_and_lifetime_byte_budgets() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    for script in [
        "local s=string.rep('x',2048); for i=1,40 do print(s) end; return ui.text {'ok'}",
        "local s=string.rep('x',32768); print(s,s,s); return ui.text {'ok'}",
    ] {
        assert!(check(script, 1).is_err(), "unbounded captured print");
    }
    let mut v = Visual::load(
        "return function() print('ok',42,true,nil); return ui.text {'ok'} end",
        1,
    )
    .unwrap();
    v.render().unwrap();
    assert_eq!(v.logs(), ["ok\t42\ttrue\t"]);
}

#[test]
fn native_text_collections_and_lossy_utf8_are_bounded() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    for script in [
        "local s=string.rep('x',32768); return ui.tabs {options=table.create(512,s),on_change=function() end}",
        "local s=string.rep('x',32768); return ui.chart {x=table.create(512,s),series={}}",
        "local s=string.rep('x',32768); return ui.table {columns=table.create(512,s),rows={}}",
    ] {
        let (result, allocated) = measured(script);
        assert!(result.is_err());
        assert!(
            allocated < 8 * 1024 * 1024,
            "native allocated {allocated} bytes"
        );
    }
    assert!(
        check(
            "return ui.text {ui.span {string.rep(string.char(255),3000)}}",
            1
        )
        .is_err()
    );
    check(
        "return ui.text {ui.span {string.rep(string.char(255),100)}}",
        1,
    )
    .unwrap();
}

#[test]
fn scene_retained_text_is_bounded_and_log_budget_survives_render() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    assert!(check("return ui.scene {height=300,draw=function(s) local t=string.rep('x',8000); for i=1,140 do s:label({0,0,0},t) end end}",1).is_err());
    let mut v = Visual::load(
        "return function() print(string.rep('x',4096)); return ui.text {'ok'} end",
        1,
    )
    .unwrap();
    for _ in 0..16 {
        v.render().unwrap();
    }
    assert!(v.render().is_err());
    assert_eq!(v.logs().iter().map(String::len).sum::<usize>(), 65536);
    check(
        "return ui.text {ui.span {'áé🙂', 42, true, weight='bold', color='accent'}}",
        1,
    )
    .unwrap();
}
