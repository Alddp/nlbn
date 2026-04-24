/// KiCad layer mapping for EasyEDA footprints
/// Map EasyEDA layer ID to KiCad layer name

const FRONT_SMD_PAD_LAYERS: [&str; 3] = ["F.Cu", "F.Paste", "F.Mask"];
const BACK_SMD_PAD_LAYERS: [&str; 3] = ["B.Cu", "B.Paste", "B.Mask"];
const ALL_SMD_PAD_LAYERS: [&str; 3] = ["*.Cu", "*.Paste", "*.Mask"];
const FRONT_THT_PAD_LAYERS: [&str; 2] = ["F.Cu", "F.Mask"];
const BACK_THT_PAD_LAYERS: [&str; 2] = ["B.Cu", "B.Mask"];
const ALL_THT_PAD_LAYERS: [&str; 2] = ["*.Cu", "*.Mask"];

fn to_owned_layers(layers: &[&str]) -> Vec<String> {
    layers.iter().map(|layer| (*layer).to_string()).collect()
}

fn map_graphics_layer_name(layer_id: i32) -> &'static str {
    match layer_id {
        1 => "F.Cu",            // Front copper
        2 => "B.Cu",            // Back copper
        3 => "F.SilkS",         // Front silk screen
        4 => "B.SilkS",         // Back silk screen
        5 => "F.Paste",         // Front paste
        6 => "B.Paste",         // Back paste
        7 => "F.Mask",          // Front mask
        8 => "B.Mask",          // Back mask
        10 | 11 => "Edge.Cuts", // Board edge
        12 => "Cmts.User",      // User comments
        13 | 101 => "F.Fab",    // Front fabrication
        14 => "B.Fab",          // Back fabrication
        15 => "Dwgs.User",      // User drawings
        _ => "F.SilkS",         // Default to front silk screen
    }
}

/// Map EasyEDA layer ID to KiCad layer name for general graphics
pub fn map_layer(layer_id: i32) -> String {
    map_graphics_layer_name(layer_id).to_string()
}

/// Map EasyEDA layer ID to KiCad pad layers for SMD pads
pub fn map_pad_layers_smd(layer_id: i32) -> Vec<String> {
    match layer_id {
        1 => to_owned_layers(&FRONT_SMD_PAD_LAYERS),
        2 => to_owned_layers(&BACK_SMD_PAD_LAYERS),
        3 => to_owned_layers(&["F.SilkS"]),
        11 => to_owned_layers(&ALL_SMD_PAD_LAYERS),
        13 => to_owned_layers(&["F.Fab"]),
        15 => to_owned_layers(&["Dwgs.User"]),
        _ => to_owned_layers(&FRONT_SMD_PAD_LAYERS),
    }
}

/// Map EasyEDA layer ID to KiCad pad layers for through-hole pads
/// Note: Through-hole pads don't have paste layers
pub fn map_pad_layers_tht(layer_id: i32) -> Vec<String> {
    match layer_id {
        1 => to_owned_layers(&FRONT_THT_PAD_LAYERS),
        2 => to_owned_layers(&BACK_THT_PAD_LAYERS),
        3 => to_owned_layers(&["F.SilkS"]),
        11 => to_owned_layers(&ALL_THT_PAD_LAYERS),
        13 => to_owned_layers(&["F.Fab"]),
        15 => to_owned_layers(&["Dwgs.User"]),
        _ => to_owned_layers(&ALL_THT_PAD_LAYERS),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_map_layer() {
        assert_eq!(map_layer(1), "F.Cu");
        assert_eq!(map_layer(2), "B.Cu");
        assert_eq!(map_layer(3), "F.SilkS");
        assert_eq!(map_layer(13), "F.Fab");
    }

    #[test]
    fn test_map_pad_layers_smd() {
        let layers = map_pad_layers_smd(1);
        assert_eq!(layers, vec!["F.Cu", "F.Paste", "F.Mask"]);

        let layers = map_pad_layers_smd(2);
        assert_eq!(layers, vec!["B.Cu", "B.Paste", "B.Mask"]);
    }

    #[test]
    fn test_map_pad_layers_tht() {
        let layers = map_pad_layers_tht(1);
        assert_eq!(layers, vec!["F.Cu", "F.Mask"]);

        let layers = map_pad_layers_tht(11);
        assert_eq!(layers, vec!["*.Cu", "*.Mask"]);
    }
}
