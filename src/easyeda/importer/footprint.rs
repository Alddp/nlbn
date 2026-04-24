use crate::easyeda::models::*;
use crate::error::{EasyedaError, Result};

pub struct FootprintImporter;

impl FootprintImporter {
    pub fn parse(shape_data: &[String]) -> Result<EeFootprint> {
        let mut footprint = EeFootprint {
            name: String::new(),
            pads: Vec::new(),
            tracks: Vec::new(),
            circles: Vec::new(),
            arcs: Vec::new(),
            rectangles: Vec::new(),
            texts: Vec::new(),
            holes: Vec::new(),
            vias: Vec::new(),
            svg_nodes: Vec::new(),
        };

        for shape in shape_data {
            let fields: Vec<&str> = shape.split('~').collect();
            if fields.is_empty() {
                continue;
            }

            match fields[0] {
                "PAD" => {
                    if let Ok(pad) = Self::parse_pad(&fields) {
                        footprint.pads.push(pad);
                    }
                }
                "TRACK" => {
                    if let Ok(track) = Self::parse_track(&fields) {
                        footprint.tracks.push(track);
                    }
                }
                "CIRCLE" => {
                    if let Ok(circle) = Self::parse_circle(&fields) {
                        footprint.circles.push(circle);
                    }
                }
                "ARC" => {
                    if let Ok(arc) = Self::parse_arc(&fields) {
                        footprint.arcs.push(arc);
                    }
                }
                "RECT" => {
                    if let Ok(rect) = Self::parse_rectangle(&fields) {
                        footprint.rectangles.push(rect);
                    }
                }
                "TEXT" => {
                    if let Ok(text) = Self::parse_text(&fields) {
                        footprint.texts.push(text);
                    }
                }
                "HOLE" => {
                    if let Ok(hole) = Self::parse_hole(&fields) {
                        footprint.holes.push(hole);
                    }
                }
                "VIA" => {
                    if let Ok(via) = Self::parse_via(&fields) {
                        footprint.vias.push(via);
                    }
                }
                "SVGNODE" => {
                    if let Ok(svg_node) = Self::parse_svg_node(&fields) {
                        footprint.svg_nodes.push(svg_node);
                    }
                }
                _ => {}
            }
        }

        Ok(footprint)
    }

    fn parse_pad(fields: &[&str]) -> Result<EePad> {
        if fields.len() < 9 {
            return Err(EasyedaError::InvalidData("Invalid pad data".to_string()).into());
        }

        let shape = fields[1].to_string();
        let x = fields[2]
            .parse::<f64>()
            .map_err(|_| EasyedaError::InvalidData("Invalid pad X".to_string()))?;
        let y = fields[3]
            .parse::<f64>()
            .map_err(|_| EasyedaError::InvalidData("Invalid pad Y".to_string()))?;
        let width = fields[4]
            .parse::<f64>()
            .map_err(|_| EasyedaError::InvalidData("Invalid pad width".to_string()))?;
        let height = fields[5]
            .parse::<f64>()
            .map_err(|_| EasyedaError::InvalidData("Invalid pad height".to_string()))?;
        let layer_id = fields[6]
            .parse::<i32>()
            .map_err(|_| EasyedaError::InvalidData("Invalid pad layer_id".to_string()))?;
        let number = if fields.len() > 8 {
            fields[8].to_string()
        } else {
            String::new()
        };
        let hole_radius = if fields.len() > 9 {
            let val = fields[9].parse::<f64>().unwrap_or(0.0);
            if val > 0.0 { Some(val) } else { None }
        } else {
            None
        };
        let points = if fields.len() > 10 {
            fields[10].to_string()
        } else {
            String::new()
        };
        let rotation = if fields.len() > 11 {
            fields[11].parse::<f64>().unwrap_or(0.0)
        } else {
            0.0
        };
        let hole_length = if fields.len() > 13 {
            let val = fields[13].parse::<f64>().unwrap_or(0.0);
            if val > 0.0 { Some(val) } else { None }
        } else {
            None
        };

        Ok(EePad {
            number,
            shape,
            x,
            y,
            width,
            height,
            rotation,
            hole_radius,
            hole_length,
            points,
            layer_id,
        })
    }

    fn parse_track(fields: &[&str]) -> Result<EeTrack> {
        if fields.len() < 5 {
            return Err(EasyedaError::InvalidData("Invalid track data".to_string()).into());
        }

        Ok(EeTrack {
            stroke_width: fields[1]
                .parse::<f64>()
                .map_err(|_| EasyedaError::InvalidData("Invalid track width".to_string()))?,
            layer_id: fields[2]
                .parse::<i32>()
                .map_err(|_| EasyedaError::InvalidData("Invalid track layer_id".to_string()))?,
            net: fields[3].to_string(),
            points: fields[4].to_string(),
        })
    }

    fn parse_circle(fields: &[&str]) -> Result<EeCircle> {
        if fields.len() < 5 {
            return Err(EasyedaError::InvalidData("Invalid circle data".to_string()).into());
        }

        let cx = fields[1]
            .parse::<f64>()
            .map_err(|_| EasyedaError::InvalidData("Invalid circle CX".to_string()))?;
        let cy = fields[2]
            .parse::<f64>()
            .map_err(|_| EasyedaError::InvalidData("Invalid circle CY".to_string()))?;
        let radius = fields[3]
            .parse::<f64>()
            .map_err(|_| EasyedaError::InvalidData("Invalid circle radius".to_string()))?;
        let stroke_width = if fields.len() > 4 {
            fields[4].parse::<f64>().unwrap_or(1.0)
        } else {
            1.0
        };
        let layer_id = if fields.len() > 5 {
            fields[5].parse::<i32>().unwrap_or(3)
        } else {
            3
        };

        Ok(EeCircle {
            cx,
            cy,
            radius,
            stroke_width,
            fill: false,
            layer_id,
        })
    }

    fn parse_arc(fields: &[&str]) -> Result<EeFootprintArc> {
        if fields.len() < 5 {
            return Err(EasyedaError::InvalidData("Invalid arc data".to_string()).into());
        }

        Ok(EeFootprintArc {
            stroke_width: fields[1].parse::<f64>().unwrap_or(1.0),
            layer_id: fields[2].parse::<i32>().unwrap_or(3),
            path: fields[4].to_string(),
        })
    }

    fn parse_rectangle(fields: &[&str]) -> Result<EeRectangle> {
        if fields.len() < 6 {
            return Err(EasyedaError::InvalidData("Invalid rectangle data".to_string()).into());
        }

        let x = fields[1]
            .parse::<f64>()
            .map_err(|_| EasyedaError::InvalidData("Invalid rectangle X".to_string()))?;
        let y = fields[2]
            .parse::<f64>()
            .map_err(|_| EasyedaError::InvalidData("Invalid rectangle Y".to_string()))?;
        let width = fields[3]
            .parse::<f64>()
            .map_err(|_| EasyedaError::InvalidData("Invalid rectangle width".to_string()))?;
        let height = fields[4]
            .parse::<f64>()
            .map_err(|_| EasyedaError::InvalidData("Invalid rectangle height".to_string()))?;
        let stroke_width = if fields.len() > 5 {
            fields[5].parse::<f64>().unwrap_or(1.0)
        } else {
            1.0
        };
        let layer_id = if fields.len() > 7 {
            fields[7].parse::<i32>().unwrap_or(3)
        } else {
            3
        };

        Ok(EeRectangle {
            x,
            y,
            width,
            height,
            stroke_width,
            fill: false,
            layer_id,
        })
    }

    fn parse_text(fields: &[&str]) -> Result<EeText> {
        if fields.len() < 11 {
            return Err(EasyedaError::InvalidData("Invalid text data".to_string()).into());
        }

        Ok(EeText {
            text: fields[10].to_string(),
            x: fields[2]
                .parse::<f64>()
                .map_err(|_| EasyedaError::InvalidData("Invalid text X".to_string()))?,
            y: fields[3]
                .parse::<f64>()
                .map_err(|_| EasyedaError::InvalidData("Invalid text Y".to_string()))?,
            rotation: fields[5].parse::<i32>().unwrap_or(0),
            font_size: fields[9].parse::<f64>().unwrap_or(12.0),
            stroke_width: fields[4].parse::<f64>().unwrap_or(0.0),
            layer_id: fields[7].parse::<i32>().unwrap_or(3),
        })
    }

    fn parse_hole(fields: &[&str]) -> Result<EeHole> {
        if fields.len() < 4 {
            return Err(EasyedaError::InvalidData("Invalid hole data".to_string()).into());
        }

        Ok(EeHole {
            x: fields[1]
                .parse::<f64>()
                .map_err(|_| EasyedaError::InvalidData("Invalid hole X".to_string()))?,
            y: fields[2]
                .parse::<f64>()
                .map_err(|_| EasyedaError::InvalidData("Invalid hole Y".to_string()))?,
            radius: fields[3]
                .parse::<f64>()
                .map_err(|_| EasyedaError::InvalidData("Invalid hole radius".to_string()))?,
        })
    }

    fn parse_via(fields: &[&str]) -> Result<EeVia> {
        if fields.len() < 6 {
            return Err(EasyedaError::InvalidData("Invalid via data".to_string()).into());
        }

        Ok(EeVia {
            x: fields[1]
                .parse::<f64>()
                .map_err(|_| EasyedaError::InvalidData("Invalid via X".to_string()))?,
            y: fields[2]
                .parse::<f64>()
                .map_err(|_| EasyedaError::InvalidData("Invalid via Y".to_string()))?,
            diameter: fields[3]
                .parse::<f64>()
                .map_err(|_| EasyedaError::InvalidData("Invalid via diameter".to_string()))?,
            net: fields[4].to_string(),
            radius: fields[5]
                .parse::<f64>()
                .map_err(|_| EasyedaError::InvalidData("Invalid via radius".to_string()))?,
        })
    }

    fn parse_svg_node(fields: &[&str]) -> Result<EeSvgNode> {
        if fields.len() < 3 {
            return Err(EasyedaError::InvalidData("Invalid SVG node data".to_string()).into());
        }

        Ok(EeSvgNode {
            path: fields[2].to_string(),
            stroke_width: 1.0,
            layer: fields[1].to_string(),
        })
    }
}
