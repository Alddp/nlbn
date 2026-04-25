use crate::converter::Converter;
use crate::error::{AppError, Result};
use crate::kicad::symbol::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SymbolFillColor {
    red: u8,
    green: u8,
    blue: u8,
    alpha: u8,
}

impl SymbolFillColor {
    pub fn parse(value: &str) -> Result<Self> {
        let trimmed = value.trim();
        let hex = trimmed.strip_prefix('#').unwrap_or(trimmed);

        if !(hex.len() == 6 || hex.len() == 8) || !hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
            return Err(AppError::Other(format!(
                "Invalid symbol fill color '{}'. Expected #RRGGBB or #RRGGBBAA",
                value
            )));
        }

        let red = u8::from_str_radix(&hex[0..2], 16)
            .map_err(|_| AppError::Other(format!("Invalid symbol fill color '{}'", value)))?;
        let green = u8::from_str_radix(&hex[2..4], 16)
            .map_err(|_| AppError::Other(format!("Invalid symbol fill color '{}'", value)))?;
        let blue = u8::from_str_radix(&hex[4..6], 16)
            .map_err(|_| AppError::Other(format!("Invalid symbol fill color '{}'", value)))?;
        let alpha = if hex.len() == 8 {
            u8::from_str_radix(&hex[6..8], 16)
                .map_err(|_| AppError::Other(format!("Invalid symbol fill color '{}'", value)))?
        } else {
            u8::MAX
        };

        Ok(Self {
            red,
            green,
            blue,
            alpha,
        })
    }

    fn to_kicad_fill(self) -> String {
        format!(
            "(fill (type color) (color {} {} {} {}))",
            self.red,
            self.green,
            self.blue,
            self.alpha_to_kicad()
        )
    }

    fn alpha_to_kicad(self) -> String {
        match self.alpha {
            0 => "0".to_string(),
            u8::MAX => "1".to_string(),
            alpha => {
                let formatted = format!("{:.3}", f64::from(alpha) / f64::from(u8::MAX));
                formatted
                    .trim_end_matches('0')
                    .trim_end_matches('.')
                    .to_string()
            }
        }
    }
}

pub struct SymbolExporter {
    converter: Converter,
    rectangle_fill_color: Option<SymbolFillColor>,
}

impl SymbolExporter {
    pub fn new(rectangle_fill_color: Option<SymbolFillColor>) -> Self {
        Self {
            converter: Converter::new(),
            rectangle_fill_color,
        }
    }

    pub fn export(&self, symbol: &KiSymbol) -> Result<String> {
        self.export_v6(symbol)
    }

    fn export_v6(&self, symbol: &KiSymbol) -> Result<String> {
        let mut output = String::new();

        // Calculate y_high and y_low from pin positions
        let (y_high, y_low) = self.calculate_y_bounds(symbol);

        // Start symbol definition - match Python formatting
        output.push_str(&format!("  (symbol \"{}\"\n", symbol.name));
        output.push_str("    (in_bom yes)\n");
        output.push_str("    (on_board yes)\n");

        // Properties with proper formatting
        const FIELD_OFFSET_START: f64 = 5.08;
        const FIELD_OFFSET_INCREMENT: f64 = 2.54;
        let mut field_offset_y = FIELD_OFFSET_START;
        let mut property_id = 0;

        // Reference property
        output.push_str("    (property\n");
        output.push_str("      \"Reference\"\n");
        output.push_str(&format!("      \"{}\"\n", symbol.reference));
        output.push_str(&format!("      (id {})\n", property_id));
        output.push_str(&format!("      (at 0 {:.2} 0)\n", y_high + field_offset_y));
        output.push_str("      (effects (font (size 1.27 1.27) ) )\n");
        output.push_str("    )\n");
        property_id += 1;

        // Value property
        output.push_str("    (property\n");
        output.push_str("      \"Value\"\n");
        output.push_str(&format!("      \"{}\"\n", symbol.value));
        output.push_str(&format!("      (id {})\n", property_id));
        output.push_str(&format!("      (at 0 {:.2} 0)\n", y_low - field_offset_y));
        output.push_str("      (effects (font (size 1.27 1.27) ) )\n");
        output.push_str("    )\n");
        property_id += 1;

        // Footprint property
        if !symbol.footprint.is_empty() {
            field_offset_y += FIELD_OFFSET_INCREMENT;
            output.push_str("    (property\n");
            output.push_str("      \"Footprint\"\n");
            output.push_str(&format!("      \"{}\"\n", symbol.footprint));
            output.push_str(&format!("      (id {})\n", property_id));
            output.push_str(&format!("      (at 0 {:.2} 0)\n", y_low - field_offset_y));
            output.push_str("      (effects (font (size 1.27 1.27) ) hide)\n");
            output.push_str("    )\n");
            property_id += 1;
        }

        // Datasheet property (always emit)
        field_offset_y += FIELD_OFFSET_INCREMENT;
        output.push_str("    (property\n");
        output.push_str("      \"Datasheet\"\n");
        output.push_str(&format!("      \"{}\"\n", symbol.datasheet));
        output.push_str(&format!("      (id {})\n", property_id));
        output.push_str(&format!("      (at 0 {:.2} 0)\n", y_low - field_offset_y));
        output.push_str("      (effects (font (size 1.27 1.27) ) hide)\n");
        output.push_str("    )\n");
        property_id += 1;

        // Description property (always emit)
        field_offset_y += FIELD_OFFSET_INCREMENT;
        output.push_str("    (property\n");
        output.push_str("      \"Description\"\n");
        output.push_str(&format!("      \"{}\"\n", symbol.description));
        output.push_str(&format!("      (id {})\n", property_id));
        output.push_str(&format!("      (at 0 {:.2} 0)\n", y_low - field_offset_y));
        output.push_str("      (effects (font (size 1.27 1.27) ) hide)\n");
        output.push_str("    )\n");
        property_id += 1;

        // Manufacturer property
        if !symbol.manufacturer.is_empty() {
            field_offset_y += FIELD_OFFSET_INCREMENT;
            output.push_str("    (property\n");
            output.push_str("      \"Manufacturer\"\n");
            output.push_str(&format!("      \"{}\"\n", symbol.manufacturer));
            output.push_str(&format!("      (id {})\n", property_id));
            output.push_str(&format!("      (at 0 {:.2} 0)\n", y_low - field_offset_y));
            output.push_str("      (effects (font (size 1.27 1.27) ) hide)\n");
            output.push_str("    )\n");
            property_id += 1;
        }

        // LCSC Part property
        if !symbol.lcsc_id.is_empty() {
            field_offset_y += FIELD_OFFSET_INCREMENT;
            output.push_str("    (property\n");
            output.push_str("      \"LCSC Part\"\n");
            output.push_str(&format!("      \"{}\"\n", symbol.lcsc_id));
            output.push_str(&format!("      (id {})\n", property_id));
            output.push_str(&format!("      (at 0 {:.2} 0)\n", y_low - field_offset_y));
            output.push_str("      (effects (font (size 1.27 1.27) ) hide)\n");
            output.push_str("    )\n");
            property_id += 1;
        }

        // JLC Part property
        if !symbol.jlc_id.is_empty() {
            field_offset_y += FIELD_OFFSET_INCREMENT;
            output.push_str("    (property\n");
            output.push_str("      \"JLC Part\"\n");
            output.push_str(&format!("      \"{}\"\n", symbol.jlc_id));
            output.push_str(&format!("      (id {})\n", property_id));
            output.push_str(&format!("      (at 0 {:.2} 0)\n", y_low - field_offset_y));
            output.push_str("      (effects (font (size 1.27 1.27) ) hide)\n");
            output.push_str("    )\n");
        }

        // Symbol graphics section (unit 0, convert 1) - contains body graphics
        output.push_str(&format!("    (symbol \"{}_0_1\"\n", symbol.name));

        // Rectangles
        for rect in &symbol.rectangles {
            output.push_str(&self.format_rectangle_v6(rect));
        }

        // Circles
        for circle in &symbol.circles {
            output.push_str(&self.format_circle_v6(circle));
        }

        // Arcs
        for arc in &symbol.arcs {
            output.push_str(&self.format_arc_v6(arc));
        }

        // Polylines
        for polyline in &symbol.polylines {
            output.push_str(&self.format_polyline_v6(polyline));
        }

        // Texts
        for text in &symbol.texts {
            output.push_str(&self.format_text_v6(text));
        }

        // Pins - in the same _0_1 section as graphics
        for pin in &symbol.pins {
            output.push_str(&self.format_pin_v6(pin));
        }

        output.push_str("    )\n");
        output.push_str("  )\n");

        Ok(output)
    }

    fn calculate_y_bounds(&self, symbol: &KiSymbol) -> (f64, f64) {
        if symbol.pins.is_empty() {
            return (0.0, 0.0);
        }

        let mut y_high = f64::MIN;
        let mut y_low = f64::MAX;

        for pin in &symbol.pins {
            let y = self.converter.px_to_mm(pin.pos_y);
            if y > y_high {
                y_high = y;
            }
            if y < y_low {
                y_low = y;
            }
        }

        (y_high, y_low)
    }

    fn format_pin_v6(&self, pin: &KiPin) -> String {
        let x = self.converter.px_to_mm(pin.pos_x);
        let y = self.converter.px_to_mm(pin.pos_y);
        let length = self.converter.px_to_mm(pin.length);

        // Convert pin rotation: (180 + orientation) % 360
        let orientation = (180 + pin.rotation) % 360;

        format!(
            "      (pin {} {}\n        (at {:.2} {:.2} {})\n        (length {:.2})\n        (name \"{}\" (effects (font (size 1.27 1.27))))\n        (number \"{}\" (effects (font (size 1.27 1.27))))\n      )\n",
            pin.pin_type.to_kicad(),
            pin.style.to_kicad(),
            x,
            y,
            orientation,
            length,
            pin.name,
            pin.number
        )
    }

    fn format_rectangle_v6(&self, rect: &KiRectangle) -> String {
        let x1 = self.converter.px_to_mm(rect.x1);
        let y1 = self.converter.px_to_mm(rect.y1);
        let x2 = self.converter.px_to_mm(rect.x2);
        let y2 = self.converter.px_to_mm(rect.y2);
        let _width = self.converter.px_to_mm(rect.stroke_width);

        let fill = if rect.fill {
            self.rectangle_fill_color
                .map(SymbolFillColor::to_kicad_fill)
                .unwrap_or_else(|| "(fill (type background))".to_string())
        } else {
            "(fill (type none))".to_string()
        };

        format!(
            "      (rectangle\n        (start {:.2} {:.2})\n        (end {:.2} {:.2})\n        (stroke (width {}) (type default) (color 0 0 0 0))\n        {}\n      )\n",
            x1, y1, x2, y2, 0, fill
        )
    }

    fn format_circle_v6(&self, circle: &KiCircle) -> String {
        let cx = self.converter.px_to_mm(circle.cx);
        let cy = self.converter.px_to_mm(circle.cy);
        let radius = self.converter.px_to_mm(circle.radius);

        // Circles in symbols should always have fill type "none" to match Python output
        let fill = "none";

        format!(
            "      (circle\n        (center {:.2} {:.2})\n        (radius {:.2})\n        (stroke (width {}) (type default) (color 0 0 0 0))\n        (fill (type {}))\n      )\n",
            cx, cy, radius, 0, fill
        )
    }

    fn format_arc_v6(&self, arc: &KiArc) -> String {
        let start_x = self.converter.px_to_mm(arc.start_x);
        let start_y = self.converter.px_to_mm(arc.start_y); // Don't flip, already handled
        let mid_x = self.converter.px_to_mm(arc.mid_x);
        let mid_y = self.converter.px_to_mm(arc.mid_y); // Don't flip, already handled
        let end_x = self.converter.px_to_mm(arc.end_x);
        let end_y = self.converter.px_to_mm(arc.end_y); // Don't flip, already handled
        let width = self.converter.px_to_mm(arc.stroke_width);

        format!(
            "    (arc (start {:.4} {:.4}) (mid {:.4} {:.4}) (end {:.4} {:.4})\n      (stroke (width {:.4}) (type default))\n      (fill (type none))\n    )\n",
            start_x, start_y, mid_x, mid_y, end_x, end_y, width
        )
    }

    fn format_polyline_v6(&self, polyline: &KiPolyline) -> String {
        let mut output = String::from("    (polyline\n      (pts\n");

        for (x, y) in &polyline.points {
            let x = self.converter.px_to_mm(*x);
            let y = self.converter.px_to_mm(*y); // Don't flip, already handled
            output.push_str(&format!("        (xy {:.4} {:.4})\n", x, y));
        }

        let width = self.converter.px_to_mm(polyline.stroke_width);
        let fill = if polyline.fill { "outline" } else { "none" };

        output.push_str("      )\n");
        output.push_str(&format!(
            "      (stroke (width {:.4}) (type default))\n",
            width
        ));
        output.push_str(&format!("      (fill (type {}))\n", fill));
        output.push_str("    )\n");

        output
    }

    fn format_text_v6(&self, text: &super::symbol::KiText) -> String {
        let x = self.converter.px_to_mm(text.x);
        let y = self.converter.px_to_mm(text.y);
        let size = (text.font_size * 0.15).clamp(0.5, 1.27);
        let rotation = text.rotation;

        format!(
            "    (text \"{}\" (at {:.4} {:.4} {})\n      (effects (font (size {:.4} {:.4})))\n    )\n",
            text.text, x, y, rotation as i32, size, size
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{SymbolExporter, SymbolFillColor};
    use crate::kicad::symbol::{KiPin, KiRectangle, KiSymbol, PinStyle, PinType};

    fn test_symbol(fill: bool) -> KiSymbol {
        KiSymbol {
            name: "Fixture".to_string(),
            reference: "U".to_string(),
            value: "Fixture".to_string(),
            description: String::new(),
            footprint: String::new(),
            datasheet: String::new(),
            manufacturer: String::new(),
            lcsc_id: String::new(),
            jlc_id: String::new(),
            pins: vec![KiPin {
                number: "1".to_string(),
                name: "IN".to_string(),
                pin_type: PinType::Input,
                style: PinStyle::Line,
                pos_x: 0.0,
                pos_y: 0.0,
                rotation: 0,
                length: 100.0,
            }],
            rectangles: vec![KiRectangle {
                x1: 0.0,
                y1: 0.0,
                x2: 100.0,
                y2: -100.0,
                stroke_width: 0.0,
                fill,
            }],
            circles: Vec::new(),
            arcs: Vec::new(),
            polylines: Vec::new(),
            texts: Vec::new(),
        }
    }

    #[test]
    fn uses_background_fill_when_no_custom_color_is_configured() {
        let exporter = SymbolExporter::new(None);
        let output = exporter
            .export(&test_symbol(true))
            .expect("symbol export should succeed");

        assert!(output.contains("(fill (type background))"));
        assert!(!output.contains("(fill (type color)"));
    }

    #[test]
    fn uses_custom_fill_color_when_configured() {
        let exporter = SymbolExporter::new(Some(
            SymbolFillColor::parse("#005C8FCC").expect("color should parse"),
        ));
        let output = exporter
            .export(&test_symbol(true))
            .expect("symbol export should succeed");

        assert!(output.contains("(fill (type color) (color 0 92 143 0.8))"));
    }

    #[test]
    fn parses_rgb_without_alpha_as_fully_opaque() {
        let color = SymbolFillColor::parse("#005C8F").expect("color should parse");

        assert_eq!(
            color.to_kicad_fill(),
            "(fill (type color) (color 0 92 143 1))"
        );
    }
}
