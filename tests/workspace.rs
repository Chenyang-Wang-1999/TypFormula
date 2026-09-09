use visual_typst_core::workspace::{file,resolve};
use serde_json::json;
#[test]
fn project_paths_and_optimistic_save() {
    let root=std::env::temp_dir().join(format!("visual-typst-files-{}",std::process::id()));std::fs::create_dir_all(&root).unwrap();
    assert!(resolve(&root,"../outside.typ").is_err());assert!(resolve(&root,"/outside.typ").is_err());
    file(&root,json!({"path":"main.typ","source":"中文😀","base":null})).unwrap();
    assert!(file(&root,json!({"path":"main.typ","source":"lost","base":null})).is_err());
    assert!(file(&root,json!({"path":"main.typ","source":"new","base":"中文😀"})).is_ok());
    assert_eq!(file(&root,json!({"path":"main.typ"})).unwrap()["source"],"new");
    std::fs::remove_file(root.join("main.typ")).unwrap();std::fs::remove_dir(root).unwrap();
}

