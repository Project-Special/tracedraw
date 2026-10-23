//! Scripts an AI service wrote run in a process of their own on the
//! desktop: `tracedraw --script-sandbox` gets the drawing and the script
//! on standard input and answers with the changes on standard output,
//! both in CBOR ([`encode`]), which keeps every number as it is.
//!
//! The interpreter has loop and call-depth limits but none on memory: a
//! few lines (`s += s` in a loop, `"x".repeat(2 ** 30)`) can ask for
//! gigabytes. In the sandbox process that ends the process, not the
//! window with the user's drawings. The sandbox limits its own memory
//! before it runs the script (a job object on Windows, the data limit on
//! Linux); on macOS, which enforces neither, the window watches its
//! footprint. The window also stops it after a time limit. When no
//! process can be started the script runs in the window as before.

use super::{vg, Evaluated, HostInner, Trust};
use serde::{Deserialize, Serialize};
use tracedraw_core::{Command, Document, LayerId, PageId, ShapeId};

/// The argument that starts the sandbox instead of the window.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub const ARG: &str = "--script-sandbox";

/// `value` in CBOR. Unlike JSON it keeps non-finite numbers (a NaN in a
/// drawing), so the sandbox works on exactly the drawing it was sent.
fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    ciborium::into_writer(value, &mut out).map_err(|e| e.to_string())?;
    Ok(out)
}

/// The value CBOR `bytes` hold.
fn decode<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, String> {
    ciborium::from_reader(bytes).map_err(|e| e.to_string())
}

/// What a script left: the outcome `run_host` applies.
#[derive(Debug, Serialize, Deserialize)]
pub(super) struct Ran {
    pub commands: Vec<Command>,
    pub selection: Vec<ShapeId>,
    pub page: PageId,
    pub output: Vec<String>,
    pub status: Option<String>,
    /// The script's error, if it stopped with one.
    pub result: Result<(), String>,
    /// The completion value as shown, when there was one.
    pub shown: String,
}

impl From<Evaluated> for Ran {
    fn from(e: Evaluated) -> Ran {
        Ran {
            commands: e.inner.commands,
            selection: e.inner.selection,
            page: e.inner.page,
            output: e.inner.output,
            status: e.inner.status,
            result: e.result.map(|_| ()),
            shown: e.shown,
        }
    }
}

/// What the sandbox gets, as the window sends it.
#[cfg_attr(any(test, target_arch = "wasm32"), allow(dead_code))]
#[derive(Serialize)]
struct RequestOut<'a> {
    doc: &'a Document,
    page: PageId,
    layer: Option<LayerId>,
    selection: &'a [ShapeId],
    next_id: u64,
    file: Option<&'a str>,
    view: &'a vg::AppView,
    src: &'a str,
}

/// What the sandbox gets, as it reads it.
#[derive(Deserialize)]
struct Request {
    doc: Document,
    page: PageId,
    layer: Option<LayerId>,
    selection: Vec<ShapeId>,
    next_id: u64,
    file: Option<String>,
    view: vg::AppView,
    src: String,
}

/// The sandbox's answer: what the script left, or why it could not run.
type Reply = Result<Ran, String>;

/// Run the request `input` holds, as the sandbox process does.
fn answer(input: &[u8]) -> Reply {
    let r: Request = decode(input)?;
    if !r.doc.fonts.is_empty() {
        tracedraw_text::fonts().add_document_fonts(&r.doc.fonts);
    }
    let host = HostInner {
        doc: r.doc,
        page: r.page,
        layer: r.layer,
        selection: r.selection,
        commands: Vec::new(),
        output: Vec::new(),
        next_id: r.next_id,
        trust: Trust::Untrusted,
        file: r.file,
        vg: false,
        view: r.view,
        status: None,
    };
    super::evaluate(host, &r.src, None, None).map(Ran::from)
}

/// `tracedraw --script-sandbox`: read the request from standard input,
/// run it and write the reply to standard output.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub fn main() {
    use std::io::{Read, Write};
    let mut input = Vec::new();
    let reply = match std::io::stdin().read_to_end(&mut input) {
        Ok(_) => {
            #[cfg(not(target_arch = "wasm32"))]
            limits::limit_self(memory_limit(input.len()));
            answer(&input)
        }
        Err(e) => Err(e.to_string()),
    };
    let out = encode(&reply)
        .or_else(|e| encode(&Reply::Err(e)))
        .unwrap_or_default();
    let mut stdout = std::io::stdout().lock();
    let _ = stdout.write_all(&out);
    let _ = stdout.flush();
}

/// Run `src` for `inner` in a sandbox process: `None` when no process
/// could be started (the caller then runs it in the window).
#[cfg(not(any(test, target_arch = "wasm32")))]
pub(super) fn run(inner: &HostInner, src: &str) -> Option<Reply> {
    let exe = std::env::current_exe().ok()?;
    run_with(&exe, inner, src)
}

/// Longest the sandbox may take before it is stopped.
#[cfg(not(target_arch = "wasm32"))]
const TIME_LIMIT: std::time::Duration = std::time::Duration::from_secs(60);
/// Memory the sandbox may use: this, plus four times the request it was
/// sent (its copy of the drawing and the changes made to it).
#[cfg(not(target_arch = "wasm32"))]
const MEMORY_BASE: u64 = 2 << 30;

/// The memory limit of a sandbox sent `request_len` bytes. The tests set
/// a smaller base with `TRACEDRAW_SANDBOX_MB`.
#[cfg(not(target_arch = "wasm32"))]
fn memory_limit(request_len: usize) -> u64 {
    let base = std::env::var("TRACEDRAW_SANDBOX_MB")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .map_or(MEMORY_BASE, |mb| mb << 20);
    base.saturating_add(4 * request_len as u64)
}
/// Longest reply read.
#[cfg(not(target_arch = "wasm32"))]
const MAX_REPLY: u64 = 1 << 30;

/// [`run`] with the program `exe` as the sandbox.
#[cfg(not(target_arch = "wasm32"))]
#[cfg_attr(test, allow(dead_code))]
fn run_with(exe: &std::path::Path, inner: &HostInner, src: &str) -> Option<Reply> {
    use std::io::{Read, Write};
    use std::process::Stdio;
    let body = encode(&RequestOut {
        doc: &inner.doc,
        page: inner.page,
        layer: inner.layer,
        selection: &inner.selection,
        next_id: inner.next_id,
        file: inner.file.as_deref(),
        view: &inner.view,
        src,
    })
    .ok()?;
    let limit = memory_limit(body.len());
    let mut cmd = std::process::Command::new(exe);
    cmd.arg(ARG)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            log::warn!("script sandbox not started ({e}); the script runs in the window");
            return None;
        }
    };
    let (Some(mut stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
        let _ = child.kill();
        let _ = child.wait();
        return None;
    };
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(&body);
    });
    let reader = std::thread::spawn(move || {
        let mut out = Vec::new();
        let _ = stdout.take(MAX_REPLY).read_to_end(&mut out);
        out
    });
    let start = std::time::Instant::now();
    let stopped = |child: &mut std::process::Child, why: String| {
        let _ = child.kill();
        let _ = child.wait();
        Some(Err(why))
    };
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(e) => return stopped(&mut child, e.to_string()),
        }
        if start.elapsed() > TIME_LIMIT {
            return stopped(
                &mut child,
                format!("the script took longer than {} s", TIME_LIMIT.as_secs()),
            );
        }
        #[cfg(target_os = "macos")]
        if limits::footprint(child.id()).is_some_and(|used| used > limit) {
            return stopped(&mut child, too_much_memory(limit));
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    };
    let _ = writer.join();
    let out = reader.join().unwrap_or_default();
    if !status.success() {
        // Out of memory ends the process; so does any other failure.
        return Some(Err(too_much_memory(limit)));
    }
    Some(decode::<Reply>(&out).unwrap_or_else(Err))
}

#[cfg(not(target_arch = "wasm32"))]
fn too_much_memory(limit: u64) -> String {
    format!(
        "the script was stopped: it needed more than {} MB of memory",
        limit >> 20
    )
}

/// The operating system's limits on the sandbox process.
#[cfg(not(target_arch = "wasm32"))]
mod limits {
    /// Limit this process to `bytes` of memory, where the system enforces
    /// it: an allocation past it fails and ends the process.
    #[cfg(target_os = "linux")]
    pub fn limit_self(bytes: u64) {
        use std::ffi::{c_int, c_ulong};
        #[repr(C)]
        struct Rlimit {
            cur: c_ulong,
            max: c_ulong,
        }
        extern "C" {
            fn setrlimit(resource: c_int, limit: *const Rlimit) -> c_int;
        }
        // The data limit: heap and other private memory, not file
        // mappings such as the fonts.
        const RLIMIT_DATA: c_int = 2;
        let bytes = c_ulong::try_from(bytes).unwrap_or(c_ulong::MAX);
        let l = Rlimit {
            cur: bytes,
            max: bytes,
        };
        // SAFETY: a plain system call on a value that lives past it.
        if unsafe { setrlimit(RLIMIT_DATA, &l) } != 0 {
            log::warn!("script sandbox: memory limit not set");
        }
    }

    /// macOS enforces no memory limit on a process: the window watches
    /// the sandbox instead ([`footprint`]).
    #[cfg(not(any(target_os = "linux", windows)))]
    pub fn limit_self(_bytes: u64) {}

    /// The memory the process `pid` uses (its physical footprint).
    #[cfg(target_os = "macos")]
    pub fn footprint(pid: u32) -> Option<u64> {
        use std::ffi::{c_int, c_void};
        extern "C" {
            fn proc_pid_rusage(pid: c_int, flavor: c_int, buffer: *mut c_void) -> c_int;
        }
        const RUSAGE_INFO_V0: c_int = 0;
        // rusage_info_v0: a 16-byte uuid, then u64 fields; the physical
        // footprint is the eighth of them.
        let mut info = [0u64; 32];
        // SAFETY: the buffer is larger than rusage_info_v0.
        let r = unsafe {
            proc_pid_rusage(
                c_int::try_from(pid).ok()?,
                RUSAGE_INFO_V0,
                info.as_mut_ptr().cast(),
            )
        };
        (r == 0).then_some(info[9])
    }

    /// JOBOBJECT_BASIC_LIMIT_INFORMATION.
    #[cfg(windows)]
    #[repr(C)]
    #[derive(Default)]
    struct BasicLimits {
        per_process_user_time: i64,
        per_job_user_time: i64,
        flags: u32,
        minimum_working_set: usize,
        maximum_working_set: usize,
        active_process_limit: u32,
        affinity: usize,
        priority_class: u32,
        scheduling_class: u32,
    }

    /// JOBOBJECT_EXTENDED_LIMIT_INFORMATION.
    #[cfg(windows)]
    #[repr(C)]
    #[derive(Default)]
    struct ExtendedLimits {
        basic: BasicLimits,
        io: [u64; 6],
        process_memory_limit: usize,
        job_memory_limit: usize,
        peak_process_memory: usize,
        peak_job_memory: usize,
    }

    /// Put this process in a job object of its own with a memory limit
    /// (the job lives as long as the process).
    #[cfg(windows)]
    pub fn limit_self(bytes: u64) {
        use std::ffi::c_void;
        use std::ptr::{null, null_mut};
        #[link(name = "kernel32")]
        extern "system" {
            fn CreateJobObjectW(attributes: *mut c_void, name: *const u16) -> *mut c_void;
            fn SetInformationJobObject(
                job: *mut c_void,
                class: i32,
                info: *const c_void,
                len: u32,
            ) -> i32;
            fn AssignProcessToJobObject(job: *mut c_void, process: *mut c_void) -> i32;
            fn GetCurrentProcess() -> *mut c_void;
        }
        const JOB_OBJECT_EXTENDED_LIMIT_INFORMATION: i32 = 9;
        const JOB_OBJECT_LIMIT_PROCESS_MEMORY: u32 = 0x100;
        const JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION: u32 = 0x400;
        let info = ExtendedLimits {
            basic: BasicLimits {
                flags: JOB_OBJECT_LIMIT_PROCESS_MEMORY
                    | JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION,
                ..Default::default()
            },
            process_memory_limit: usize::try_from(bytes).unwrap_or(usize::MAX),
            ..Default::default()
        };
        // SAFETY: plain Win32 calls; `info` is laid out as
        // JOBOBJECT_EXTENDED_LIMIT_INFORMATION and lives past them. The
        // job handle is kept open for the life of the process.
        let ok = unsafe {
            let job = CreateJobObjectW(null_mut(), null());
            !job.is_null()
                && SetInformationJobObject(
                    job,
                    JOB_OBJECT_EXTENDED_LIMIT_INFORMATION,
                    (&info as *const ExtendedLimits).cast(),
                    std::mem::size_of::<ExtendedLimits>() as u32,
                ) != 0
                && AssignProcessToJobObject(job, GetCurrentProcess()) != 0
        };
        if !ok {
            log::warn!("script sandbox: memory limit not set");
        }
    }

    #[cfg(all(windows, target_pointer_width = "64", test))]
    #[test]
    fn the_job_limits_have_the_system_layout() {
        // The sizes of the Win32 structures on 64-bit Windows.
        assert_eq!(std::mem::size_of::<BasicLimits>(), 64);
        assert_eq!(std::mem::size_of::<ExtendedLimits>(), 144);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(doc: &Document, src: &str) -> Vec<u8> {
        let page = doc.pages[0].id;
        encode(&RequestOut {
            doc,
            page,
            layer: doc.pages[0].layers.first().map(|l| l.id),
            selection: &[],
            next_id: doc.ids().peek_next(),
            file: None,
            view: &vg::AppView::default(),
            src,
        })
        .expect("request")
    }

    /// The sandbox runs the script on the drawing it was sent, untrusted,
    /// and answers with the changes and the printed lines.
    #[test]
    fn the_sandbox_answers_with_the_changes() {
        let doc = Document::new("t", tracedraw_core::geometry::Size::new(210.0, 297.0));
        let ran = answer(&request(
            &doc,
            "ActiveLayer.CreateRectangle(10, 10, 50, 40); print('done');",
        ))
        .expect("ran");
        assert_eq!(ran.result, Ok(()));
        assert_eq!(ran.output, vec!["done".to_string()]);
        assert!(!ran.commands.is_empty());
        let bytes = encode(&Reply::Ok(ran)).expect("reply");
        let back: Reply = decode(&bytes).expect("parses");
        assert!(back.is_ok());
    }

    /// The window's side: the drawing goes to the program, the changes
    /// come back. Uses the built program next to the test (there when
    /// the integration tests were built with it).
    #[test]
    fn the_window_gets_the_changes_from_the_sandbox() {
        let Some(exe) = std::env::current_exe().ok().and_then(|t| {
            let dir = t.parent()?.parent()?;
            let exe = dir.join(format!("tracedraw{}", std::env::consts::EXE_SUFFIX));
            exe.is_file().then_some(exe)
        }) else {
            return;
        };
        let app = crate::app::App::headless();
        let host = super::super::host_for(&app, Trust::Untrusted);
        let ran = run_with(&exe, &host, "ActiveLayer.CreateEllipse(10, 10, 60, 40);")
            .expect("started")
            .expect("ran");
        assert_eq!(ran.result, Ok(()));
        assert!(!ran.commands.is_empty());
        let failed = run_with(&exe, &host, "throw new Error('no');")
            .expect("started")
            .expect("ran");
        assert!(failed.result.is_err());
    }

    /// Untrusted there too: no file access.
    #[test]
    fn the_sandbox_keeps_scripts_from_files() {
        let doc = Document::new("t", tracedraw_core::geometry::Size::new(210.0, 297.0));
        let ran = answer(&request(&doc, "ActiveDocument.Export('x.svg');")).expect("ran");
        assert!(ran.result.is_err());
        assert!(answer(b"not cbor").is_err());
    }
}
