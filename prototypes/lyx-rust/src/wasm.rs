// SPDX-License-Identifier: GPL-2.0-or-later
// Minimal JSON bridge. Editing executes synchronously in this Rust WASM instance.
use crate::{Action, Editor};
use std::cell::RefCell;
thread_local! {
    static EDITOR: RefCell<Editor> = RefCell::new(Editor::default());
    static OUTPUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}
#[unsafe(no_mangle)]
pub extern "C" fn alloc(size: usize) -> *mut u8 {
    let buffer = vec![0u8; size].into_boxed_slice();
    Box::into_raw(buffer) as *mut u8
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dispatch(ptr: *mut u8, len: usize) -> *const u8 {
    // JS allocates exactly len bytes using alloc and transfers ownership once.
    let bytes = unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(ptr, len)) };
    let output = EDITOR.with(|editor| {
        let mut editor = editor.borrow_mut();
        match serde_json::from_slice::<Action>(&bytes).map_err(|e| e.to_string()).and_then(|a| editor.apply(a)) {
            Ok(()) => serde_json::to_vec(&editor.response()).unwrap(),
            Err(error) => serde_json::to_vec(&serde_json::json!({"error":error})).unwrap(),
        }
    });
    OUTPUT.with(|buffer| { *buffer.borrow_mut() = output; buffer.borrow().as_ptr() })
}
#[unsafe(no_mangle)]
pub extern "C" fn output_len() -> usize { OUTPUT.with(|buffer| buffer.borrow().len()) }
