// SPDX-License-Identifier: GPL-2.0-or-later
// Minimal JSON bridge. Editing executes synchronously in this Rust WASM instance.
use crate::document::Document;
use std::cell::RefCell;
use std::collections::HashMap;
thread_local! {
    static EDITOR: RefCell<Document> = RefCell::new(Document::default());
    static OUTPUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    // Every buffer handed to JavaScript, with the length it was allocated with.
    // A slice pointer is fat, and JavaScript only ever passes the address back.
    static INPUTS: RefCell<HashMap<usize, usize>> = RefCell::new(HashMap::new());
}
fn reply(value: Vec<u8>) -> *const u8 {
    OUTPUT.with(|buffer| { *buffer.borrow_mut() = value; buffer.borrow().as_ptr() })
}
fn fail(message: &str) -> *const u8 {
    reply(serde_json::to_vec(&serde_json::json!({"error":message})).unwrap())
}
#[unsafe(no_mangle)]
pub extern "C" fn alloc(size: usize) -> *mut u8 {
    let mut buffer = vec![0u8; size].into_boxed_slice();
    let ptr = buffer.as_mut_ptr();
    std::mem::forget(buffer);
    INPUTS.with(|inputs| inputs.borrow_mut().insert(ptr as usize, size));
    ptr
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dispatch(ptr: *mut u8, len: usize) -> *const u8 {
    // Ownership is transferred exactly once, with the length this module
    // allocated. A repeated address or a forged length is an error result, not
    // an out-of-bounds read or a second free.
    let recorded = INPUTS.with(|inputs| inputs.borrow_mut().remove(&(ptr as usize)));
    let Some(recorded) = recorded else { return fail("输入缓冲区无效或已释放"); };
    if recorded != len {
        drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(ptr, recorded)) });
        return fail("输入缓冲区长度与分配长度不一致");
    }
    let bytes = unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(ptr, recorded)) };
    let output = EDITOR.with(|editor| {
        let mut editor = editor.borrow_mut();
        match serde_json::from_slice(&bytes).map_err(|e| e.to_string()).and_then(|a| editor.apply(a)) {
            Ok(()) => serde_json::to_vec(&editor.response()).unwrap(),
            Err(error) => serde_json::to_vec(&serde_json::json!({"error":error})).unwrap(),
        }
    });
    reply(output)
}
#[unsafe(no_mangle)]
pub extern "C" fn output_len() -> usize { OUTPUT.with(|buffer| buffer.borrow().len()) }

#[cfg(test)]
mod tests {
    use super::*;
    // Never dereferenced: an address this module did not hand out fails the lookup.
    const STRAY: *mut u8 = 1 as *mut u8;
    fn output() -> serde_json::Value {
        let bytes=OUTPUT.with(|buffer| buffer.borrow().clone());
        serde_json::from_slice(&bytes).unwrap()
    }
    fn send(action: &str) -> serde_json::Value {
        let bytes = action.as_bytes();
        let ptr = alloc(bytes.len());
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len()); }
        unsafe { dispatch(ptr, bytes.len()) };
        assert_eq!(output_len(), OUTPUT.with(|buffer| buffer.borrow().len()));
        output()
    }
    #[test]
    fn one_allocation_is_consumed_exactly_once_with_its_own_length() {
        let state=send(r#"{"action":"state"}"#);
        assert!(state.get("view").is_some(),"{state}");
        // A forged length is refused instead of rebuilding the buffer with it.
        let bytes=b"{}";
        let ptr=alloc(bytes.len());
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len()); }
        let forged=unsafe { dispatch(ptr, 4096) };
        assert!(!forged.is_null());
        assert_eq!(output()["error"],serde_json::json!("输入缓冲区长度与分配长度不一致"));
        // The same address must never be freed a second time.
        let reused=unsafe { dispatch(STRAY, 0) };
        assert!(!reused.is_null());
        assert_eq!(output()["error"],serde_json::json!("输入缓冲区无效或已释放"));
    }
    #[test]
    fn an_unregistered_pointer_reports_an_error() {
        unsafe { dispatch(STRAY, 0) };
        assert_eq!(output()["error"],serde_json::json!("输入缓冲区无效或已释放"));
    }
}
