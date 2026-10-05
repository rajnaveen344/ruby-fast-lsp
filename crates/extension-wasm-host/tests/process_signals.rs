//! The host process receives ordinary signals (for example `SIGCHLD` when a
//! child process exits); an extension engine must not turn one into an abort.
#![cfg(target_os = "macos")]
// `libc` deprecates its Mach bindings in favour of a crate this test does not need.
#![allow(deprecated)]

use ruby_fast_lsp_extension_wasm_host::{WasmExtension, WasmExtensionCompiler};
use std::{mem, ptr, thread, time::Duration};

const EXTENSION: &str = r#"
    (module
      (memory (export "memory") 1)
      (data (i32.const 1024) "[]")
      (func (export "alloc") (param $len i32) (result i32) i32.const 4096)
      (func (export "dealloc") (param $ptr i32) (param $len i32))
      (func (export "abi_version") (result i32) i32.const 1)
      (func (export "indexed_call_names") (result i64)
        (i64.or
          (i64.shl (i64.extend_i32_u (i32.const 1024)) (i64.const 32))
          (i64.extend_i32_u (i32.const 2))))
      (func (export "index_call") (param $ptr i32) (param $len i32) (result i64)
        (i64.const 0))
    )
"#;

extern "C" {
    fn mach_port_deallocate(
        task: libc::mach_port_t,
        name: libc::mach_port_t,
    ) -> libc::kern_return_t;
}

extern "C" fn ignore_signal(_: libc::c_int) {}

/// Delivers `signal` to every thread of this process except the caller, as
/// the kernel may when it picks a thread for a process-directed signal.
fn signal_every_other_thread(signal: libc::c_int) {
    unsafe {
        let task = libc::mach_task_self();
        let mut threads: libc::thread_act_array_t = ptr::null_mut();
        let mut count: libc::mach_msg_type_number_t = 0;
        assert_eq!(
            libc::task_threads(task, &mut threads, &mut count),
            libc::KERN_SUCCESS
        );
        let me = libc::pthread_self();
        for index in 0..count as usize {
            let port = *threads.add(index);
            let thread = libc::pthread_from_mach_thread_np(port);
            if thread != 0 as libc::pthread_t && libc::pthread_equal(thread, me) == 0 {
                libc::pthread_kill(thread, signal);
            }
            mach_port_deallocate(task, port);
        }
        libc::vm_deallocate(
            task,
            threads as libc::vm_address_t,
            count as libc::vm_size_t * mem::size_of::<libc::thread_act_t>() as libc::vm_size_t,
        );
    }
}

#[test]
fn handled_process_signals_do_not_abort_the_extension_host() {
    WasmExtensionCompiler::new().expect("an extension engine must start");
    unsafe {
        let mut action: libc::sigaction = mem::zeroed();
        action.sa_sigaction = ignore_signal as extern "C" fn(libc::c_int) as libc::sighandler_t;
        action.sa_flags = libc::SA_RESTART;
        libc::sigemptyset(&mut action.sa_mask);
        assert_eq!(libc::sigaction(libc::SIGUSR1, &action, ptr::null_mut()), 0);
    }

    for _ in 0..20 {
        signal_every_other_thread(libc::SIGUSR1);
        thread::sleep(Duration::from_millis(5));
    }

    let wasm = wat::parse_str(EXTENSION).expect("the test extension must assemble");
    let mut extension =
        WasmExtension::from_bytes("signals", &wasm).expect("the test extension must load");
    assert_eq!(extension.abi_version().expect("the guest must answer"), 1);
}
