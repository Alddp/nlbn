use nlbn::converter::sanitize_name;
use nlbn::easyeda::{ComponentData, EasyedaApi, Model3dInfo};
use nlbn::footprint_converter::convert_footprint;
use nlbn::model_converter::convert_3d_model;
use nlbn::{Cli, LibraryManager};
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

fn test_cli(project_relative: bool) -> Cli {
    Cli {
        lcsc_id: Some("C123456".to_string()),
        batch: None,
        symbol: false,
        footprint: true,
        model_3d: false,
        full: true,
        output: PathBuf::new(),
        overwrite: false,
        overwrite_symbol: false,
        overwrite_footprint: false,
        overwrite_model_3d: false,
        project_relative,
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

fn component_data_without_model() -> ComponentData {
    let mut data = test_component_data();
    data.model_3d = None;
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

fn footprint_content(output_dir: &Path, footprint_name: &str) -> String {
    let lib_name = output_dir
        .file_name()
        .and_then(|name| name.to_str())
        .expect("library directory should have a valid name");
    let footprint_path = output_dir
        .join(format!("{}.pretty", lib_name))
        .join(format!("{}.kicad_mod", footprint_name));
    fs::read_to_string(footprint_path).expect("should read generated footprint")
}

fn extract_model_path(footprint: &str) -> &str {
    let start = footprint
        .find("(model \"")
        .expect("footprint should include a model reference")
        + "(model \"".len();
    let end = footprint[start..]
        .find('"')
        .expect("model reference should terminate")
        + start;
    &footprint[start..end]
}

fn has_model_reference(footprint: &str) -> bool {
    footprint.contains("(model \"")
}

fn expected_model_path(model_name: &str, project_relative: bool, extension: &str) -> String {
    if project_relative {
        format!(
            "${{KIPRJMOD}}/demo-lib.3dshapes/{}.{}",
            model_name, extension
        )
    } else {
        format!("../demo-lib.3dshapes/{}.{}", model_name, extension)
    }
}

#[test]
fn omits_model_reference_when_no_3d_files_exist() {
    let workspace = TestWorkspace::new("default-wrl");
    let output_dir = workspace.library_dir();
    let lib_manager = LibraryManager::new(&output_dir);
    lib_manager
        .create_directories()
        .expect("should create library directories");

    let args = test_cli(false);
    let component_data = test_component_data();
    let (footprint_name, _) = expected_names(&component_data, "C123456");

    convert_footprint(&args, &component_data, &lib_manager, "C123456")
        .expect("footprint conversion should succeed");

    let content = footprint_content(&output_dir, &footprint_name);
    assert!(!has_model_reference(&content));
}

#[test]
fn prefers_wrl_reference_when_wrl_and_step_exist() {
    let workspace = TestWorkspace::new("prefer-wrl");
    let output_dir = workspace.library_dir();
    let lib_manager = LibraryManager::new(&output_dir);
    lib_manager
        .create_directories()
        .expect("should create library directories");

    let args = test_cli(false);
    let component_data = test_component_data();
    let (footprint_name, model_name) = expected_names(&component_data, "C123456");

    fs::write(lib_manager.get_wrl_path(&model_name), "wrl fixture").expect("should write wrl");
    fs::write(lib_manager.get_step_path(&model_name), b"step fixture").expect("should write step");

    convert_footprint(&args, &component_data, &lib_manager, "C123456")
        .expect("footprint conversion should succeed");

    let content = footprint_content(&output_dir, &footprint_name);
    assert_eq!(
        extract_model_path(&content),
        expected_model_path(&model_name, false, "wrl")
    );
}

#[test]
fn falls_back_to_step_reference_when_only_step_exists() {
    let workspace = TestWorkspace::new("fallback-step");
    let output_dir = workspace.library_dir();
    let lib_manager = LibraryManager::new(&output_dir);
    lib_manager
        .create_directories()
        .expect("should create library directories");

    let args = test_cli(false);
    let component_data = test_component_data();
    let (footprint_name, model_name) = expected_names(&component_data, "C123456");

    fs::write(lib_manager.get_step_path(&model_name), b"step fixture").expect("should write step");

    convert_footprint(&args, &component_data, &lib_manager, "C123456")
        .expect("footprint conversion should succeed");

    let content = footprint_content(&output_dir, &footprint_name);
    assert_eq!(
        extract_model_path(&content),
        expected_model_path(&model_name, false, "step")
    );
}

#[test]
fn uses_kiprjmod_model_path_when_project_relative_mode_is_enabled() {
    let workspace = TestWorkspace::new("project-relative");
    let output_dir = workspace.library_dir();
    let lib_manager = LibraryManager::new(&output_dir);
    lib_manager
        .create_directories()
        .expect("should create library directories");

    let args = test_cli(true);
    let component_data = test_component_data();
    let (footprint_name, model_name) = expected_names(&component_data, "C123456");

    fs::write(lib_manager.get_wrl_path(&model_name), "wrl fixture").expect("should write wrl");

    convert_footprint(&args, &component_data, &lib_manager, "C123456")
        .expect("footprint conversion should succeed");

    let content = footprint_content(&output_dir, &footprint_name);
    assert_eq!(
        extract_model_path(&content),
        expected_model_path(&model_name, true, "wrl")
    );
}

#[tokio::test]
async fn convert_3d_model_errors_when_metadata_is_missing() {
    let workspace = TestWorkspace::new("missing-3d-metadata");
    let output_dir = workspace.library_dir();
    let lib_manager = LibraryManager::new(&output_dir);
    lib_manager
        .create_directories()
        .expect("should create library directories");

    let api = EasyedaApi::new();
    let component_data = component_data_without_model();

    let error = convert_3d_model(&api, &component_data, &lib_manager, "C123456")
        .await
        .expect_err("missing metadata should fail explicit 3D export");

    assert!(
        error
            .to_string()
            .contains("No 3D model metadata available for C123456")
    );
}
