use visual_typst_core::workspace::resolve;

#[test]
fn project_paths_stay_inside_the_document_directory() {
    let root=std::env::temp_dir().join(format!("visual-typst-paths-{}",std::process::id()));
    std::fs::create_dir_all(root.join("sub")).unwrap();
    std::fs::write(root.join("inside.typ"),"inline $x$").unwrap();
    // The document directory itself, an existing file and a nested path are fine.
    assert_eq!(resolve(&root,"inside.typ").unwrap(),root.canonicalize().unwrap().join("inside.typ"));
    assert_eq!(resolve(&root,"sub/new.typ").unwrap(),root.canonicalize().unwrap().join("sub/new.typ"));
    assert_eq!(resolve(&root,"new.typ").unwrap(),root.canonicalize().unwrap().join("new.typ"));
    // An empty name, an absolute path and any parent step are refused.
    assert!(resolve(&root,"").is_err());
    assert!(resolve(&root,"/outside.typ").is_err());
    assert!(resolve(&root,"../outside.typ").is_err());
    assert!(resolve(&root,"sub/../../outside.typ").is_err());
    std::fs::remove_dir_all(&root).unwrap();
}
