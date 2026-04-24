use nlbn::converter::sanitize_name;
use nlbn::easyeda::{ComponentData, Model3dInfo};
use nlbn::footprint_converter::convert_footprint;
use nlbn::{Cli, LibraryManager, RunRequest};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

struct TestWorkspace {
    root: PathBuf,
}

impl TestWorkspace {
    fn new(test_name: &str) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join("nlbn-tests").join(format!(
            "{}-{}-{}",
            test_name,
            std::process::id(),
            unique
        ));
        fs::create_dir_all(&root).expect("should create test workspace");
        Self { root }
    }

    fn library_dir(&self) -> PathBuf {
        self.root.join("demo-lib")
    }
}

impl Drop for TestWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn test_cli(overwrite: bool) -> Cli {
    Cli {
        lcsc_id: Some("C123456".to_string()),
        batch: None,
        symbol: false,
        footprint: true,
        model_3d: false,
        full: false,
        output: PathBuf::new(),
        overwrite,
        project_relative: false,
        symbol_fill_color: None,
        debug: false,
        continue_on_error: false,
        parallel: 1,
    }
}

fn test_component_data() -> ComponentData {
    ComponentData {
        lcsc_id: "C123456".to_string(),
        title: "Test Component".to_string(),
        description: "fixture".to_string(),
        data_str: Vec::new(),
        bbox_x: 0.0,
        bbox_y: 0.0,
        package_detail: vec!["PAD~RECT~10~20~5~6~1~NET~1~0~~0".to_string()],
        package_bbox_x: 0.0,
        package_bbox_y: 0.0,
        model_3d: Some(Model3dInfo {
            uuid: "fixture-uuid".to_string(),
            title: "Test Model".to_string(),
        }),
        manufacturer: "fixture".to_string(),
        datasheet: String::new(),
        jlc_id: String::new(),
    }
}

fn invalid_component_data() -> ComponentData {
    let mut data = test_component_data();
    data.package_detail = vec!["not-a-valid-footprint-record".to_string()];
    data
}

fn expected_names(component_data: &ComponentData, lcsc_id: &str) -> (String, String) {
    (
        format!("{}_{}", sanitize_name(&component_data.title), lcsc_id),
        format!(
            "{}_{}",
            sanitize_name(
                &component_data
                    .model_3d
                    .as_ref()
                    .expect("fixture should include 3D metadata")
                    .title
            ),
            lcsc_id
        ),
    )
}

fn footprint_path(output_dir: &Path, footprint_name: &str) -> PathBuf {
    LibraryManager::new(output_dir).get_footprint_path(footprint_name)
}

#[test]
fn existing_footprint_is_preserved_without_overwrite() {
    let workspace = TestWorkspace::new("preserve-footprint");
    let output_dir = workspace.library_dir();
    let lib_manager = LibraryManager::with_overwrite(&output_dir, false);
    lib_manager
        .create_directories()
        .expect("should create library directories");

    let args = test_cli(false);
    let component_data = test_component_data();
    let (footprint_name, _) = expected_names(&component_data, "C123456");
    let path = footprint_path(&output_dir, &footprint_name);

    fs::write(&path, "existing footprint").expect("should seed existing footprint");

    convert_footprint(&args, &component_data, &lib_manager, "C123456")
        .expect("footprint conversion should succeed");

    assert_eq!(
        fs::read_to_string(path).expect("should read preserved footprint"),
        "existing footprint"
    );
}

#[test]
fn existing_footprint_is_replaced_with_overwrite() {
    let workspace = TestWorkspace::new("replace-footprint");
    let output_dir = workspace.library_dir();
    let lib_manager = LibraryManager::with_overwrite(&output_dir, true);
    lib_manager
        .create_directories()
        .expect("should create library directories");

    let args = test_cli(true);
    let component_data = test_component_data();
    let (footprint_name, _) = expected_names(&component_data, "C123456");
    let path = footprint_path(&output_dir, &footprint_name);

    fs::write(&path, "existing footprint").expect("should seed existing footprint");

    convert_footprint(&args, &component_data, &lib_manager, "C123456")
        .expect("footprint conversion should succeed");

    let content = fs::read_to_string(path).expect("should read replaced footprint");
    assert_ne!(content, "existing footprint");
    assert!(content.contains("(footprint \"Test_Component_C123456\""));
}

#[test]
fn existing_footprint_short_circuits_without_overwrite() {
    let workspace = TestWorkspace::new("short-circuit-footprint");
    let output_dir = workspace.library_dir();
    let lib_manager = LibraryManager::with_overwrite(&output_dir, false);
    lib_manager
        .create_directories()
        .expect("should create library directories");

    let args = test_cli(false);
    let component_data = invalid_component_data();
    let (footprint_name, _) = expected_names(&component_data, "C123456");
    let path = footprint_path(&output_dir, &footprint_name);

    fs::write(&path, "existing footprint").expect("should seed existing footprint");

    convert_footprint(&args, &component_data, &lib_manager, "C123456")
        .expect("existing footprint should be kept without reparsing");

    assert_eq!(
        fs::read_to_string(path).expect("should read preserved footprint"),
        "existing footprint"
    );
}

#[test]
fn existing_3d_outputs_are_preserved_without_overwrite() {
    let workspace = TestWorkspace::new("preserve-3d");
    let output_dir = workspace.library_dir();
    let lib_manager = LibraryManager::with_overwrite(&output_dir, false);
    lib_manager
        .create_directories()
        .expect("should create library directories");

    let component_data = test_component_data();
    let (_, model_name) = expected_names(&component_data, "C123456");
    let wrl_path = lib_manager.get_wrl_path(&model_name);
    let step_path = lib_manager.get_step_path(&model_name);

    fs::write(&wrl_path, "existing wrl").expect("should seed existing wrl");
    fs::write(&step_path, b"existing step").expect("should seed existing step");

    let wrl_outcome = lib_manager
        .write_wrl_model_with_status(&model_name, "replacement wrl")
        .expect("should handle existing wrl");
    let step_outcome = lib_manager
        .write_step_model_with_status(&model_name, b"replacement step")
        .expect("should handle existing step");

    assert!(!wrl_outcome.was_written());
    assert!(!step_outcome.was_written());
    assert_eq!(
        fs::read_to_string(wrl_path).expect("should read preserved wrl"),
        "existing wrl"
    );
    assert_eq!(
        fs::read(step_path).expect("should read preserved step"),
        b"existing step"
    );
}

#[test]
fn existing_3d_outputs_are_replaced_with_overwrite() {
    let workspace = TestWorkspace::new("replace-3d");
    let output_dir = workspace.library_dir();
    let lib_manager = LibraryManager::with_overwrite(&output_dir, true);
    lib_manager
        .create_directories()
        .expect("should create library directories");

    let component_data = test_component_data();
    let (_, model_name) = expected_names(&component_data, "C123456");
    let wrl_path = lib_manager.get_wrl_path(&model_name);
    let step_path = lib_manager.get_step_path(&model_name);

    fs::write(&wrl_path, "existing wrl").expect("should seed existing wrl");
    fs::write(&step_path, b"existing step").expect("should seed existing step");

    let wrl_outcome = lib_manager
        .write_wrl_model_with_status(&model_name, "replacement wrl")
        .expect("should replace wrl");
    let step_outcome = lib_manager
        .write_step_model_with_status(&model_name, b"replacement step")
        .expect("should replace step");

    assert!(wrl_outcome.was_written());
    assert!(step_outcome.was_written());
    assert_eq!(
        fs::read_to_string(wrl_path).expect("should read replaced wrl"),
        "replacement wrl"
    );
    assert_eq!(
        fs::read(step_path).expect("should read replaced step"),
        b"replacement step"
    );
}

#[test]
fn run_request_building_does_not_change_library_manager_defaults() {
    let workspace = TestWorkspace::new("default-overwrite");
    let output_dir = workspace.library_dir();
    let args = Cli {
        output: output_dir.clone(),
        ..test_cli(true)
    };

    RunRequest::try_from(args).expect("run request should build");

    let lib_manager = LibraryManager::new(&output_dir);

    assert!(!lib_manager.overwrite_enabled());
}

#[test]
fn explicit_library_manager_overwrite_is_preserved() {
    let workspace = TestWorkspace::new("explicit-overwrite");
    let output_dir = workspace.library_dir();

    let lib_manager = LibraryManager::with_overwrite(&output_dir, true);

    assert!(lib_manager.overwrite_enabled());
}
