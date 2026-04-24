use crate::converter::sanitize_name;
use crate::easyeda::{ComponentData, EasyedaApi};
use crate::error::Result;
use crate::kicad::ModelExporter;
use crate::library::LibraryManager;

pub async fn convert_3d_model(
    api: &EasyedaApi,
    component_data: &ComponentData,
    lib_manager: &LibraryManager,
    lcsc_id: &str,
) -> Result<()> {
    if let Some(model_info) = &component_data.model_3d {
        log::info!("Converting 3D model...");

        // Use LCSC ID as unique identifier to prevent name collisions
        let model_name = format!("{}_{}", sanitize_name(&model_info.title), lcsc_id);

        let mut has_wrl = false;
        let mut has_step = false;
        let mut wrote_wrl = false;
        let mut wrote_step = false;

        let wrl_path = lib_manager.get_wrl_path(&model_name);
        let step_path = lib_manager.get_step_path(&model_name);
        let should_write_wrl = lib_manager.should_write_file(&wrl_path);
        let should_write_step = lib_manager.should_write_file(&step_path);

        if !should_write_wrl && !should_write_step {
            println!("\u{2713} 3D model kept: {} (WRL + STEP)", model_name);
            return Ok(());
        }

        let exporter = ModelExporter::new();

        // Download OBJ and STEP in parallel when they still need to be written.
        let obj_future = async {
            if should_write_wrl {
                Some(api.download_3d_obj(&model_info.uuid).await)
            } else {
                None
            }
        };
        let step_future = async {
            if should_write_step {
                Some(api.download_3d_step(&model_info.uuid).await)
            } else {
                None
            }
        };

        let (obj_result, step_result) = tokio::join!(obj_future, step_future);

        // Process OBJ -> WRL
        match obj_result {
            Some(Ok(obj_data)) => match exporter.obj_to_wrl(&obj_data) {
                Ok(wrl_data) => {
                    match lib_manager.write_wrl_model_with_status(&model_name, &wrl_data) {
                        Ok(write_outcome) => {
                            has_wrl = true;
                            wrote_wrl = write_outcome.was_written();
                            if wrote_wrl {
                                log::info!("\u{2713} WRL model converted: {}", model_name);
                            }
                        }
                        Err(e) => log::warn!("Failed to write WRL model: {}", e),
                    }
                }
                Err(e) => log::warn!("Failed to convert OBJ to WRL: {}", e),
            },
            Some(Err(e)) => log::warn!("Failed to download OBJ model: {}", e),
            None => {
                has_wrl = wrl_path.is_file();
            }
        }

        // Process STEP result
        match step_result {
            Some(Ok(step_data)) => {
                match lib_manager.write_step_model_with_status(&model_name, &step_data) {
                    Ok(write_outcome) => {
                        has_step = true;
                        wrote_step = write_outcome.was_written();
                        if wrote_step {
                            log::info!("\u{2713} STEP model converted: {}", model_name);
                        }
                    }
                    Err(e) => log::warn!("Failed to write STEP model: {}", e),
                }
            }
            Some(Err(e)) => log::warn!("Failed to download STEP model: {}", e),
            None => {
                has_step = step_path.is_file();
            }
        }

        let action = if wrote_wrl || wrote_step {
            "converted"
        } else {
            "kept"
        };

        match (has_wrl, has_step) {
            (true, true) => println!("\u{2713} 3D model {}: {} (WRL + STEP)", action, model_name),
            (true, false) => println!("\u{2713} 3D model {}: {} (WRL only)", action, model_name),
            (false, true) => println!("\u{2713} 3D model {}: {} (STEP only)", action, model_name),
            (false, false) => println!("\u{26a0} 3D model not available"),
        }
    } else {
        log::warn!("No 3D model metadata available for this component");
    }

    Ok(())
}
