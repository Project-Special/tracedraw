//! `tracedraw --script-sandbox`: the process AI-written scripts run in
//! (see src/scripting/sandbox.rs). It answers a script with its changes,
//! and a script that asks for too much memory ends the sandbox, not the
//! window.

use std::io::Write;
use std::process::{Command, Stdio};
use tracedraw_core::{geometry::Size, Document};

fn cbor(v: &serde_json::Value) -> Vec<u8> {
    let mut out = Vec::new();
    ciborium::into_writer(v, &mut out).expect("cbor");
    out
}

/// Run the sandbox on `src` with a memory base of `mb` megabytes; the
/// exit success and what it wrote.
fn sandbox(src: &str, mb: u64) -> (bool, String) {
    let doc = Document::new("t", Size::new(210.0, 297.0));
    let page = &doc.pages[0];
    let request = serde_json::json!({
        "doc": doc,
        "page": page.id,
        "layer": page.layers.first().map(|l| l.id),
        "selection": [],
        "next_id": doc.ids().peek_next(),
        "file": null,
        "view": {"units": "Millimeters", "center": [0.0, 0.0], "nodes": [], "profiles": ["", "", ""]},
        "src": src,
    });
    let mut child = Command::new(env!("CARGO_BIN_EXE_tracedraw"))
        .arg("--script-sandbox")
        .env("TRACEDRAW_SANDBOX_MB", mb.to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("sandbox starts");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(&cbor(&request))
        .expect("request sent");
    let out = child.wait_with_output().expect("sandbox ends");
    // The reply (CBOR) as JSON text, to look into.
    let reply = ciborium::from_reader::<serde_json::Value, _>(&out.stdout[..])
        .map(|v| v.to_string())
        .unwrap_or_default();
    (out.status.success(), reply)
}

#[test]
fn the_sandbox_answers_with_the_changes() {
    let (ok, out) = sandbox(
        "ActiveLayer.CreateRectangle(10, 10, 50, 40); print('done');",
        512,
    );
    assert!(ok, "{out}");
    let reply: serde_json::Value = serde_json::from_str(&out).expect("json reply");
    let ran = &reply["Ok"];
    assert_eq!(ran["output"][0], "done");
    assert!(!ran["commands"].as_array().expect("commands").is_empty());
}

/// Doubling a string gets past any memory in a few dozen steps: the
/// sandbox's limit ends it, with no reply.
#[cfg(any(windows, target_os = "linux"))]
#[test]
fn a_script_asking_for_too_much_memory_ends_the_sandbox() {
    // The same script within the limit runs.
    let (ok, out) = sandbox(
        "let s = 'xy'; for (let i = 0; i < 16; i++) { s = s + s; } print(s.length);",
        256,
    );
    assert!(ok && out.contains("131072"), "{out}");
    let started = std::time::Instant::now();
    let (ok, out) = sandbox(
        "let s = 'xy'; for (let i = 0; i < 40; i++) { s = s + s; } print(s.length);",
        256,
    );
    assert!(!ok || !out.contains("\"Ok\""), "{out}");
    assert!(started.elapsed() < std::time::Duration::from_secs(60));
}
