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

fn scene_from(script: &str) -> Result<zeron_visual::Scene, zeron_visual::VisualError> {
    let mut v = Visual::load(script, 1)?;
    let node = v.render()?;
    let zeron_visual::NodeKind::Scene(scene) = node.kind else {
        panic!("scene")
    };
    v.draw_scene(scene.draw, 0.0)
}

#[test]
fn axes_individual_copy_is_clipped_before_allocation() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    // Prepare the shared Lua string before metering native draw copies.
    let mut v = Visual::load("local t=string.rep('x',524288); return ui.scene {height=200,draw=function(s) s:axes {labels={t,t,t}} end}",1).unwrap();
    let zeron_visual::NodeKind::Scene(node) = v.render().unwrap().kind else {
        panic!("scene")
    };
    ALLOCATED.store(0, Ordering::Relaxed);
    TRACK.store(true, Ordering::Relaxed);
    let scene = v.draw_scene(node.draw, 0.0);
    TRACK.store(false, Ordering::Relaxed);
    let allocated = ALLOCATED.load(Ordering::Relaxed);
    let scene = scene.unwrap();
    let labels: Vec<_> = scene
        .project(
            zeron_visual::Camera {
                yaw: 0.5,
                pitch: 0.3,
                zoom: 1.0,
            },
            400.,
            400.,
        )
        .prims
        .into_iter()
        .filter_map(|p| match p {
            zeron_visual::Prim::Label { text, .. } => Some(text),
            _ => None,
        })
        .collect();
    eprintln!(
        "AXES_NATIVE_ALLOCATED={allocated} LABEL_BYTES={:?}",
        labels.iter().map(String::len).collect::<Vec<_>>()
    );
    assert_eq!(labels.len(), 3);
    assert!(labels.iter().all(|s| s.len() == 8000));
    assert!(allocated < 256 * 1024, "native allocations {allocated}");
}

#[test]
fn axes_aggregate_includes_default_xyz_and_preserves_missing_labels() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let aggregate_rejected = check("return ui.scene {height=200,draw=function(s) local t=string.rep('x',8000); for i=1,44 do s:axes {labels={t,t,t}} end end}",1).is_err();
    let prefix = "return ui.scene {height=200,draw=function(s) local t=string.rep('x',8000); for i=1,131 do s:label({0,0,0},t) end; ";
    let defaults_rejected = check(
        &format!("{prefix}s:label({{0,0,0}},string.rep('x',574)); s:axes() end}}"),
        1,
    )
    .is_err();
    eprintln!(
        "AXES_AGGREGATE_REJECTED={aggregate_rejected} DEFAULT_XYZ_REJECTED={defaults_rejected}"
    );
    check(
        &format!("{prefix}s:label({{0,0,0}},string.rep('x',573)); s:axes() end}}"),
        1,
    )
    .unwrap();
    for (options, expected) in [
        ("", vec!["x", "y", "z"]),
        ("{labels=nil}", vec!["x", "y", "z"]),
        ("{labels={'east',nil,'up'}}", vec!["east", "up"]),
        ("{labels={}}", vec![]),
    ] {
        let scene = scene_from(&format!(
            "return ui.scene {{height=200,draw=function(s) s:axes({options}) end}}"
        ))
        .unwrap();
        let actual: Vec<_> = scene
            .project(
                zeron_visual::Camera {
                    yaw: 0.5,
                    pitch: 0.3,
                    zoom: 1.0,
                },
                400.,
                400.,
            )
            .prims
            .into_iter()
            .filter_map(|p| match p {
                zeron_visual::Prim::Label { text, .. } => Some(text),
                _ => None,
            })
            .collect();
        assert_eq!(actual, expected);
    }
    assert!(aggregate_rejected && defaults_rejected);
}

#[test]
fn tabs_and_link_default_copies_consume_the_view_budget() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let mut rejected = Vec::new();
    for (make, explicit) in [
        (
            "ui.tabs {options={s},on_change=function()end}",
            "ui.tabs {options={s},value='',on_change=function()end}",
        ),
        ("ui.link {url=s}", "ui.link {url=s,label=''}"),
    ] {
        let prefix = "local s='https://'..string.rep('x',7992); local nodes={}; ";
        let script = format!("{prefix}for i=1,80 do nodes[i]={make} end; return nodes");
        let result = check(&script, 1);
        eprintln!("DEFAULT_ROUTE={make} REJECTED={}", result.is_err());
        if let Err(error) = &result {
            assert!(error.message.contains("text byte budget"), "{error:?}");
        }
        rejected.push(result.is_err());
        check(
            &format!("{prefix}for i=1,80 do nodes[i]={explicit} end; return nodes"),
            1,
        )
        .unwrap();
    }
    let mut v=Visual::load("return {ui.tabs {options={'a','b'},on_change=function()end},ui.link {url='https://example.invalid'}}",1).unwrap();
    let zeron_visual::NodeKind::Stack(stack) = v.render().unwrap().kind else {
        panic!("stack")
    };
    let zeron_visual::NodeKind::Tabs { options, value, .. } = &stack.children[0].kind else {
        panic!("tabs")
    };
    assert_eq!(*value, options[0]);
    let zeron_visual::NodeKind::Link { label, url } = &stack.children[1].kind else {
        panic!("link")
    };
    assert_eq!(label, url);
    assert_eq!(rejected, [true, true]);
}

#[test]
fn numeric_children_and_canvas_text_share_retained_byte_limits() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let numeric = check("return table.create(3500,1e308)", 1).is_err();
    check("return {42,12.5,1e308}", 1).unwrap();
    let canvas=check("return ui.canvas {height=200,draw=function(g) local t=string.rep('x',8000); for i=1,132 do g:text(0,0,t) end end}",1).is_err();
    eprintln!("NUMERIC_REJECTED={numeric} CANVAS_REJECTED={canvas}");
    check("return ui.canvas {height=200,draw=function(g) g:text(0,0,'ok\\nline'); g:text(0,10,42); g:text(0,20,true); g:text(0,30,nil) end}",1).unwrap();
    assert!(numeric && canvas);
}

#[test]
fn duplicate_surface_keys_fail_publication() {
    // Shares the metered allocator with the measured tests.
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let surface = |key: &str| {
        [
            format!("ui.canvas {{key='{key}', height=60, draw=function() end}}"),
            format!(
                "ui.scene {{key='{key}', height=60, draw=function(s) s:box {{at={{0,0,0}}}} end}}"
            ),
            format!(
                "ui.chart {{key='{key}', type='bar', height=120, series={{{{values={{1,2,3}}}}}}}}"
            ),
            format!(
                "ui.shader {{key='{key}', height=60, code=[[void mainImage(out vec4 c, in vec2 p) {{ c = vec4(1.0); }}]]}}"
            ),
        ]
    };
    // Every pairing is valid with distinct keys, so a failure below is the
    // shared key alone: all variants share one id domain.
    let pairs: Vec<(String, String, String)> = surface("same")
        .into_iter()
        .flat_map(|first| {
            surface("same")
                .into_iter()
                .zip(surface("other"))
                .map(move |(other, distinct)| (first.clone(), other, distinct))
        })
        .collect();
    for (first, _, distinct) in &pairs {
        check(&format!("return ui.column {{{first}, {distinct}}}"), 1)
            .unwrap_or_else(|e| panic!("{first} + {distinct}: {e}"));
    }
    let accepted: Vec<String> = pairs
        .iter()
        .filter_map(|(first, other, _)| {
            match check(&format!("return ui.column {{{first}, {other}}}"), 1) {
                Err(error) if error.message.contains("duplicate surface key 'same'") => None,
                result => Some(format!("{first} + {other}: {result:?}")),
            }
        })
        .collect();
    assert!(
        accepted.is_empty(),
        "{} duplicate pairs not refused:\n{}",
        accepted.len(),
        accepted.join("\n")
    );
}

#[test]
fn duplicate_surface_key_from_a_control_fails_the_rerender() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let mut v = Visual::load(
        "local dup=false
         return function() return ui.column {
           ui.button {label='change', on_click=function() dup=true end},
           ui.canvas {key='a', height=60, draw=function() end},
           ui.canvas {key=dup and 'a' or 'b', height=60, draw=function() end}} end",
        1,
    )
    .unwrap();
    let tree = v.render().unwrap();
    let button = zeron_visual::children(&tree).next().unwrap();
    let zeron_visual::NodeKind::Button { on_click, .. } = button.kind else {
        panic!("button")
    };
    v.call(on_click, zeron_visual::HandlerArg::None).unwrap();
    let error = v.render().unwrap_err();
    assert!(
        error.message.contains("duplicate surface key 'a'"),
        "{error}"
    );
    // Non-surface keys, unkeyed surfaces and a surface sharing a non-surface
    // node's key are unrelated to surface ids and stay accepted.
    check(
        "return ui.column {ui.text {'one', key='same'}, ui.text {'two', key='same'},
           ui.canvas {height=60, draw=function() end}, ui.canvas {height=60, draw=function() end},
           ui.button {label='ok', key='same', on_click=function() end},
           ui.column {key='same', ui.canvas {key='inner', height=60, draw=function() end}}}",
        1,
    )
    .unwrap();
}

/// A GLB placing one mesh `instances` times whose sparse accessors have no
/// base view: `normals` logical NORMAL entries from a 16-byte binary chunk.
fn sparse_normals_glb(normals: usize, instances: usize) -> Vec<u8> {
    let sparse = |count: usize, shape: &str, bounds: &str| {
        format!(
            r#"{{"componentType":5126,"count":{count},"type":"{shape}"{bounds},"sparse":{{"count":1,"indices":{{"bufferView":0,"componentType":5125}},"values":{{"bufferView":1}}}}}}"#
        )
    };
    let mut json = format!(
        r#"{{"asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":[{nodes}]}}],"nodes":[{{"mesh":0}}],"meshes":[{{"primitives":[{{"attributes":{{"POSITION":0,"NORMAL":1}}}}]}}],"buffers":[{{"byteLength":16}}],"bufferViews":[{{"buffer":0,"byteLength":4}},{{"buffer":0,"byteOffset":4,"byteLength":12}}],"accessors":[{},{}]}}"#,
        sparse(3, "VEC3", r#","min":[0,0,0],"max":[1,1,1]"#),
        sparse(normals, "VEC3", ""),
        nodes = vec!["0"; instances].join(",")
    )
    .into_bytes();
    while !json.len().is_multiple_of(4) {
        json.push(b' ');
    }
    let total = 12 + 8 + json.len() + 8 + 16;
    let mut glb = b"glTF".to_vec();
    for n in [2, total, json.len()] {
        glb.extend_from_slice(&(n as u32).to_le_bytes());
    }
    glb.extend_from_slice(b"JSON");
    glb.extend_from_slice(&json);
    glb.extend_from_slice(&16u32.to_le_bytes());
    glb.extend_from_slice(b"BIN\0");
    glb.extend_from_slice(&[0; 16]);
    glb
}

#[test]
fn sparse_model_expansion_is_admitted_before_native_collection() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let script = "return ui.scene {height=200, draw=function(s) s:model {asset='m.glb'} end}";
    let publish = |normals: usize, instances: usize| {
        let assets = zeron_visual::Assets::new(
            [(
                "m.glb".to_owned(),
                sparse_normals_glb(normals, instances).into(),
            )]
            .into_iter()
            .collect(),
        );
        ALLOCATED.store(0, Ordering::Relaxed);
        TRACK.store(true, Ordering::Relaxed);
        let result = zeron_visual::check_with_assets(script, 1, assets);
        TRACK.store(false, Ordering::Relaxed);
        (result, ALLOCATED.load(Ordering::Relaxed))
    };
    // A ~700-byte asset: 6M normals are 72 MB in one visit (> 64 MiB); 3M
    // normals over 8 instances are 8 x 36 MB of decoding (> 256 MiB). Both
    // were collected natively and accepted before.
    let unbounded: Vec<String> = [(6_000_000, 1), (3_000_000, 8)]
        .into_iter()
        .filter_map(|(normals, instances)| {
            let (result, allocated) = publish(normals, instances);
            let refused = result
                .as_ref()
                .is_err_and(|e| e.message.contains("expanded accessor byte budget"));
            (!refused || allocated >= 8 * 1024 * 1024).then(|| {
                format!("{normals}x{instances}: {result:?}; native allocated {allocated} bytes")
            })
        })
        .collect();
    assert!(unbounded.is_empty(), "{}", unbounded.join("\n"));
    publish(3, 8).0.unwrap();
}

/// A GLB whose POSITION is a base-less sparse accessor of `positions`
/// entries, with `indices` naming bufferView 0 (4 bytes), bufferView 1
/// (12 bytes) or nothing, over a 16-byte binary chunk.
fn indexed_sparse_glb(positions: usize, indices: Option<usize>) -> Vec<u8> {
    let (primitive_indices, index_accessor) = match indices {
        Some(view) => (
            r#","indices":1"#.to_owned(),
            format!(r#",{{"bufferView":{view},"componentType":5125,"count":3,"type":"SCALAR"}}"#),
        ),
        None => (String::new(), String::new()),
    };
    let mut json = format!(
        r#"{{"asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":[0]}}],"nodes":[{{"mesh":0}}],"meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}}{primitive_indices}}}]}}],"buffers":[{{"byteLength":16}}],"bufferViews":[{{"buffer":0,"byteLength":4}},{{"buffer":0,"byteOffset":4,"byteLength":12}}],"accessors":[{{"componentType":5126,"count":{positions},"type":"VEC3","min":[0,0,0],"max":[1,1,1],"sparse":{{"count":1,"indices":{{"bufferView":0,"componentType":5125}},"values":{{"bufferView":1}}}}}}{index_accessor}]}}"#
    )
    .into_bytes();
    while !json.len().is_multiple_of(4) {
        json.push(b' ');
    }
    let total = 12 + 8 + json.len() + 8 + 16;
    let mut glb = b"glTF".to_vec();
    for n in [2, total, json.len()] {
        glb.extend_from_slice(&(n as u32).to_le_bytes());
    }
    glb.extend_from_slice(b"JSON");
    glb.extend_from_slice(&json);
    glb.extend_from_slice(&16u32.to_le_bytes());
    glb.extend_from_slice(b"BIN\0");
    glb.extend_from_slice(&[0; 16]);
    glb
}

#[test]
fn an_unreadable_index_accessor_is_refused_not_made_implicit() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let script = "return ui.scene {height=200, draw=function(s) s:model {asset='m.glb'} end}";
    let publish = |positions: usize, indices: Option<usize>| {
        let assets = zeron_visual::Assets::new(
            [(
                "m.glb".to_owned(),
                indexed_sparse_glb(positions, indices).into(),
            )]
            .into_iter()
            .collect(),
        );
        ALLOCATED.store(0, Ordering::Relaxed);
        TRACK.store(true, Ordering::Relaxed);
        let result = zeron_visual::check_with_assets(script, 1, assets);
        TRACK.store(false, Ordering::Relaxed);
        (result, ALLOCATED.load(Ordering::Relaxed))
    };
    // Three explicit u32 indices need 12 bytes; bufferView 0 holds 4, so the
    // reader yields nothing. Admitted as 48 MB of positions plus 3 indices,
    // that used to fall back to 4M implicit indices: 48 + 32 MB collected.
    let (result, allocated) = publish(4_000_000, Some(0));
    assert!(
        result
            .as_ref()
            .is_err_and(|e| e.message.contains("index accessor"))
            && allocated < 8 * 1024 * 1024,
        "{result:?}; native allocated {allocated} bytes"
    );
    // Controls: readable explicit indices and absent ones (implicit, still
    // charged: 4M positions + 4M implicit indices are refused up front).
    publish(3, Some(1)).0.unwrap();
    publish(3, None).0.unwrap();
    let (result, allocated) = publish(4_000_000, None);
    assert!(
        result
            .as_ref()
            .is_err_and(|e| e.message.contains("expanded accessor byte budget"))
            && allocated < 8 * 1024 * 1024,
        "{result:?}; native allocated {allocated} bytes"
    );
}
