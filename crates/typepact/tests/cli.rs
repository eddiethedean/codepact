use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
};

fn fixture_root() -> PathBuf {
    std::env::temp_dir().join(format!("typepact-cli-{}", std::process::id()))
}

fn run(root: &PathBuf, paths: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_typepact"))
        .current_dir(root)
        .arg("inspect-contracts")
        .arg("--root")
        .arg(root)
        .args(paths)
        .output()
        .expect("CLI starts")
}

#[test]
fn logical_module_identity_is_stable_for_relative_and_absolute_paths() {
    let root = fixture_root();
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("pkg")).unwrap();
    fs::write(
        root.join("pkg/models.py"),
        "from typepact import resource\n@resource(closed_by='close')\nclass Connection: pass\n",
    )
    .unwrap();
    fs::write(
        root.join("pkg/client.py"),
        "from pkg.models import Connection\nfrom typepact.rust import Borrowed\ndef use(value: Borrowed[Connection]): pass\n",
    )
    .unwrap();

    let relative = run(&root, &["pkg/client.py", "pkg/models.py"]);
    let absolute = run(
        &root,
        &[
            root.join("pkg/models.py").to_str().unwrap(),
            root.join("pkg/client.py").to_str().unwrap(),
        ],
    );
    assert!(
        relative.status.success(),
        "{}",
        String::from_utf8_lossy(&relative.stderr)
    );
    assert!(
        absolute.status.success(),
        "{}",
        String::from_utf8_lossy(&absolute.stderr)
    );
    assert_eq!(relative.stdout, absolute.stdout);
    let inspection: serde_json::Value = serde_json::from_slice(&relative.stdout).unwrap();
    assert!(inspection["contracts"]
        .as_array()
        .unwrap()
        .iter()
        .any(|contract| {
            contract["base_symbol"] == "pkg.models.Connection"
                && contract["display_name"] == "use.value"
        }));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn unicode_module_file_names_are_valid_python_identifiers() {
    let root = fixture_root().with_extension("unicode");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("模型.py"), "class C: pass\n").unwrap();

    let output = run(&root, &["模型.py"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    fs::remove_dir_all(root).unwrap();
}
