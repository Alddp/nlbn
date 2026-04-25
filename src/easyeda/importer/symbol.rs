use crate::easyeda::models::*;
use crate::error::{EasyedaError, Result};

pub struct SymbolImporter;

impl SymbolImporter {
    pub fn parse(data_str: &[String]) -> Result<EeSymbol> {
        log::debug!("Parsing symbol with {} shapes", data_str.len());

        let mut symbol = EeSymbol {
            name: String::new(),
            prefix: String::new(),
            pins: Vec::new(),
            rectangles: Vec::new(),
            circles: Vec::new(),
            ellipses: Vec::new(),
            arcs: Vec::new(),
            polylines: Vec::new(),
            polygons: Vec::new(),
            paths: Vec::new(),
            texts: Vec::new(),
        };

        for shape in data_str {
            if shape.is_empty() {
                continue;
            }

            let fields: Vec<&str> = shape.split('~').collect();
            if fields.is_empty() {
                continue;
            }

            let designator = fields[0];

            log::debug!(
                "Shape designator: '{}', fields: {}",
                designator,
                fields.len()
            );

            match designator {
                "P" => {
                    log::debug!("Parsing pin: {}", shape);
                    if let Ok(pin) = Self::parse_pin(shape) {
                        log::debug!(
                            "Successfully parsed pin: {} at ({}, {})",
                            pin.number,
                            pin.x,
                            pin.y
                        );
                        symbol.pins.push(pin);
                    } else {
                        log::warn!("Failed to parse pin from: {}", shape);
                    }
                }
                "R" => {
                    if let Ok(rect) = Self::parse_rectangle(&fields) {
                        symbol.rectangles.push(rect);
                    }
                }
                "C" => {
                    log::debug!("Parsing circle with {} fields: {:?}", fields.len(), fields);
                    if let Ok(circle) = Self::parse_circle(&fields) {
                        log::debug!(
                            "Successfully parsed circle at ({}, {}), radius {}, fill {}",
                            circle.cx,
                            circle.cy,
                            circle.radius,
                            circle.fill
                        );
                        symbol.circles.push(circle);
                    } else {
                        log::warn!("Failed to parse circle from: {}", shape);
                    }
                }
                "E" => {
                    log::debug!("Parsing ellipse with {} fields: {:?}", fields.len(), fields);
                    if let Ok(ellipse) = Self::parse_ellipse(&fields) {
                        log::debug!(
                            "Successfully parsed ellipse at ({}, {}), rx {}, ry {}, fill {}",
                            ellipse.cx,
                            ellipse.cy,
                            ellipse.rx,
                            ellipse.ry,
                            ellipse.fill
                        );
                        symbol.ellipses.push(ellipse);
                    } else {
                        log::warn!("Failed to parse ellipse from: {}", shape);
                    }
                }
                "A" => {
                    log::debug!("Parsing arc with {} fields", fields.len());

                    if fields.len() > 1 && fields[1].trim().starts_with("M") {
                        log::debug!("Detected SVG path arc: {}", fields[1]);
                        if let Ok(path_arcs) = Self::parse_svg_arc(&fields) {
                            symbol.arcs.extend(path_arcs);
                        } else {
                            log::warn!("Failed to parse SVG arc from: {}", shape);
                        }
                    } else if let Ok(arc) = Self::parse_arc(&fields) {
                        symbol.arcs.push(arc);
                    } else {
                        log::warn!("Failed to parse traditional arc from: {}", shape);
                    }
                }
                "PL" => {
                    if let Ok(polyline) = Self::parse_polyline(&fields) {
                        symbol.polylines.push(polyline);
                    }
                }
                "PG" => {
                    if let Ok(polygon) = Self::parse_polygon(&fields) {
                        symbol.polygons.push(polygon);
                    }
                }
                "PT" => {
                    log::debug!("Parsing PT path with {} fields", fields.len());
                    if let Ok(polygon) = Self::parse_pt_path(&fields) {
                        symbol.polygons.push(polygon);
                    } else {
                        log::warn!("Failed to parse PT path from: {}", shape);
                    }
                }
                "T" => {
                    if let Ok(text) = Self::parse_text(&fields) {
                        symbol.texts.push(text);
                    }
                }
                "PATH" => {
                    if let Ok(path) = Self::parse_path(&fields) {
                        symbol.paths.push(path);
                    }
                }
                "LIB" if fields.len() > 3 => {
                    symbol.name = fields[3].to_string();
                }
                _ => {}
            }
        }

        if symbol.prefix.is_empty() {
            symbol.prefix = "U".to_string();
        }

        log::info!(
            "Parsed symbol: {} pins, {} rectangles, {} circles, {} ellipses, {} polylines",
            symbol.pins.len(),
            symbol.rectangles.len(),
            symbol.circles.len(),
            symbol.ellipses.len(),
            symbol.polylines.len()
        );

        Ok(symbol)
    }

    fn parse_pin(pin_data: &str) -> Result<EePin> {
        let segments: Vec<&str> = pin_data.split("^^").collect();
        if segments.is_empty() {
            return Err(EasyedaError::InvalidData("Empty pin data".to_string()).into());
        }

        let fields: Vec<&str> = segments[0].split('~').collect();
        if fields.len() < 7 {
            return Err(EasyedaError::InvalidData(format!(
                "Invalid pin data, only {} fields in segment 0",
                fields.len()
            ))
            .into());
        }

        let x = fields[4]
            .parse::<f64>()
            .map_err(|_| EasyedaError::InvalidData("Invalid pin X coordinate".to_string()))?;
        let y = fields[5]
            .parse::<f64>()
            .map_err(|_| EasyedaError::InvalidData("Invalid pin Y coordinate".to_string()))?;
        let rotation = fields[6].parse::<i32>().unwrap_or(0);
        let electric_type = fields[2].to_string();

        let name = if segments.len() > 3 {
            let name_fields: Vec<&str> = segments[3].split('~').collect();
            if name_fields.len() > 4 {
                name_fields[4].to_string()
            } else {
                "PIN".to_string()
            }
        } else {
            "PIN".to_string()
        };

        let number = if segments.len() > 4 {
            let num_fields: Vec<&str> = segments[4].split('~').collect();
            if num_fields.len() > 4 {
                num_fields[4].to_string()
            } else {
                fields[3].to_string()
            }
        } else {
            fields[3].to_string()
        };

        let length = if segments.len() > 2 {
            let path_fields: Vec<&str> = segments[2].split('~').collect();
            if !path_fields.is_empty() {
                let path = path_fields[0];

                if let Some(h_pos) = path.rfind('h') {
                    let num_str = &path[h_pos + 1..].trim();
                    let parsed_length = num_str.parse::<f64>().unwrap_or(100.0).abs();
                    log::debug!(
                        "Pin {} ({}): path='{}', extracted length={}",
                        number,
                        name,
                        path,
                        parsed_length
                    );
                    parsed_length
                } else if let Some(v_pos) = path.rfind('v') {
                    let num_str = &path[v_pos + 1..].trim();
                    let parsed_length = num_str.parse::<f64>().unwrap_or(100.0).abs();
                    log::debug!(
                        "Pin {} ({}): path='{}', extracted length={}",
                        number,
                        name,
                        path,
                        parsed_length
                    );
                    parsed_length
                } else {
                    log::debug!(
                        "Pin {} ({}): path='{}' has no 'h' or 'v', using default length=100",
                        number,
                        name,
                        path
                    );
                    100.0
                }
            } else {
                100.0
            }
        } else {
            100.0
        };

        Ok(EePin {
            number,
            name,
            x,
            y,
            rotation,
            length,
            name_visible: true,
            number_visible: true,
            electric_type,
            dot: false,
            clock: false,
        })
    }

    fn parse_rectangle(fields: &[&str]) -> Result<EeRectangle> {
        if fields.len() < 7 {
            return Err(EasyedaError::InvalidData("Invalid rectangle data".to_string()).into());
        }

        log::debug!(
            "Parsing rectangle with {} fields: {:?}",
            fields.len(),
            fields
        );

        let x = fields[1]
            .parse::<f64>()
            .map_err(|_| EasyedaError::InvalidData("Invalid rectangle X".to_string()))?;
        let y = fields[2]
            .parse::<f64>()
            .map_err(|_| EasyedaError::InvalidData("Invalid rectangle Y".to_string()))?;
        let width = fields[5]
            .parse::<f64>()
            .map_err(|_| EasyedaError::InvalidData("Invalid rectangle width".to_string()))?;
        let height = fields[6]
            .parse::<f64>()
            .map_err(|_| EasyedaError::InvalidData("Invalid rectangle height".to_string()))?;
        let fill = fields.len() > 10 && !fields[10].is_empty() && fields[10] != "none";

        log::debug!(
            "Rectangle fill_color field[10] = '{}', fill = {}",
            if fields.len() > 10 { fields[10] } else { "N/A" },
            fill
        );

        Ok(EeRectangle {
            x,
            y,
            width,
            height,
            stroke_width: 1.0,
            fill,
            layer_id: 0,
        })
    }

    fn parse_circle(fields: &[&str]) -> Result<EeCircle> {
        if fields.len() < 4 {
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
        let fill = fields.len() > 7 && !fields[7].is_empty() && fields[7] != "none";

        log::debug!(
            "Circle fill_color field[7] = '{}', fill = {}",
            if fields.len() > 7 { fields[7] } else { "N/A" },
            fill
        );

        Ok(EeCircle {
            cx,
            cy,
            radius,
            stroke_width: 1.0,
            fill,
            layer_id: 0,
        })
    }

    fn parse_ellipse(fields: &[&str]) -> Result<EeEllipse> {
        if fields.len() < 5 {
            return Err(EasyedaError::InvalidData("Invalid ellipse data".to_string()).into());
        }

        let cx = fields[1]
            .parse::<f64>()
            .map_err(|_| EasyedaError::InvalidData("Invalid ellipse CX".to_string()))?;
        let cy = fields[2]
            .parse::<f64>()
            .map_err(|_| EasyedaError::InvalidData("Invalid ellipse CY".to_string()))?;
        let rx = fields[3]
            .parse::<f64>()
            .map_err(|_| EasyedaError::InvalidData("Invalid ellipse RX".to_string()))?;
        let ry = fields[4]
            .parse::<f64>()
            .map_err(|_| EasyedaError::InvalidData("Invalid ellipse RY".to_string()))?;
        let fill = fields.len() > 8 && !fields[8].is_empty() && fields[8] != "none";

        log::debug!(
            "Ellipse fill_color field[8] = '{}', fill = {}",
            if fields.len() > 8 { fields[8] } else { "N/A" },
            fill
        );

        Ok(EeEllipse {
            cx,
            cy,
            rx,
            ry,
            stroke_width: 1.0,
            fill,
        })
    }

    fn parse_arc(fields: &[&str]) -> Result<EeArc> {
        if fields.len() < 6 {
            return Err(EasyedaError::InvalidData("Invalid arc data".to_string()).into());
        }

        let x = fields[1]
            .parse::<f64>()
            .map_err(|_| EasyedaError::InvalidData("Invalid arc X".to_string()))?;
        let y = fields[2]
            .parse::<f64>()
            .map_err(|_| EasyedaError::InvalidData("Invalid arc Y".to_string()))?;
        let radius = fields[3]
            .parse::<f64>()
            .map_err(|_| EasyedaError::InvalidData("Invalid arc radius".to_string()))?;
        let start_angle = fields[4]
            .parse::<f64>()
            .map_err(|_| EasyedaError::InvalidData("Invalid arc start angle".to_string()))?;
        let end_angle = fields[5]
            .parse::<f64>()
            .map_err(|_| EasyedaError::InvalidData("Invalid arc end angle".to_string()))?;

        Ok(EeArc {
            x,
            y,
            radius,
            start_angle,
            end_angle,
            stroke_width: 1.0,
        })
    }

    fn parse_svg_arc(fields: &[&str]) -> Result<Vec<EeArc>> {
        use crate::converter::Converter;
        use crate::easyeda::svg_parser::{SvgCommand, parse_svg_path};

        if fields.len() < 2 {
            return Err(EasyedaError::InvalidData("Invalid SVG arc data".to_string()).into());
        }

        let svg_path = fields[1];
        let commands = parse_svg_path(svg_path)?;
        let conv = Converter::new();

        let mut arcs = Vec::new();
        let mut current_pos = (0.0, 0.0);

        for cmd in commands {
            match cmd {
                SvgCommand::MoveTo { x, y } => {
                    current_pos = (x, y);
                }
                SvgCommand::Arc {
                    rx,
                    ry,
                    angle,
                    large_arc,
                    sweep,
                    x,
                    y,
                } => {
                    if let Ok((cx, cy, start_angle, end_angle)) = conv.compute_arc_center(
                        current_pos,
                        (x, y),
                        (rx, ry),
                        angle,
                        large_arc,
                        sweep,
                    ) {
                        let radius = (rx + ry) / 2.0;
                        arcs.push(EeArc {
                            x: cx,
                            y: cy,
                            radius,
                            start_angle,
                            end_angle,
                            stroke_width: 1.0,
                        });
                    }

                    current_pos = (x, y);
                }
                SvgCommand::LineTo { x, y } => {
                    current_pos = (x, y);
                }
                SvgCommand::ClosePath => {}
            }
        }

        Ok(arcs)
    }

    fn parse_polyline(fields: &[&str]) -> Result<EePolyline> {
        if fields.len() < 2 {
            return Err(EasyedaError::InvalidData("Invalid polyline data".to_string()).into());
        }

        let points = Self::parse_points(fields[1])?;
        let stroke_width = if fields.len() > 3 {
            fields[3].parse::<f64>().unwrap_or(1.0)
        } else {
            1.0
        };

        Ok(EePolyline {
            points,
            stroke_width,
        })
    }

    fn parse_polygon(fields: &[&str]) -> Result<EePolygon> {
        if fields.len() < 2 {
            return Err(EasyedaError::InvalidData("Invalid polygon data".to_string()).into());
        }

        let points = Self::parse_points(fields[1])?;
        let stroke_width = if fields.len() > 3 {
            fields[3].parse::<f64>().unwrap_or(1.0)
        } else {
            1.0
        };
        let fill = fields.len() > 5 && !fields[5].is_empty() && fields[5] != "none";

        Ok(EePolygon {
            points,
            stroke_width,
            fill,
        })
    }

    fn parse_pt_path(fields: &[&str]) -> Result<EePolygon> {
        use crate::easyeda::svg_parser::{SvgCommand, parse_svg_path};

        if fields.len() < 2 {
            return Err(EasyedaError::InvalidData("Invalid path data".to_string()).into());
        }

        let commands = parse_svg_path(fields[1])?;
        let mut points = Vec::new();
        let mut has_close_path = false;

        for cmd in commands {
            match cmd {
                SvgCommand::MoveTo { x, y } => {
                    points.push((x, y));
                }
                SvgCommand::LineTo { x, y } => {
                    points.push((x, y));
                }
                SvgCommand::Arc { x, y, .. } => {
                    points.push((x, y));
                }
                SvgCommand::ClosePath => {
                    has_close_path = true;
                }
            }
        }

        if has_close_path && !points.is_empty() {
            points.push(points[0]);
        }

        let stroke_width = if fields.len() > 3 {
            fields[3].parse::<f64>().unwrap_or(1.0)
        } else {
            1.0
        };

        Ok(EePolygon {
            points,
            stroke_width,
            fill: has_close_path,
        })
    }

    fn parse_text(fields: &[&str]) -> Result<EeText> {
        if fields.len() < 13 {
            return Err(EasyedaError::InvalidData("Invalid text data".to_string()).into());
        }

        let x = fields[2]
            .parse::<f64>()
            .map_err(|_| EasyedaError::InvalidData("Invalid text X".to_string()))?;
        let y = fields[3]
            .parse::<f64>()
            .map_err(|_| EasyedaError::InvalidData("Invalid text Y".to_string()))?;
        let rotation = fields[4].parse::<i32>().unwrap_or(0);
        let text = fields[12].to_string();
        let font_size = if fields.len() > 7 {
            fields[7].parse::<f64>().unwrap_or(12.0)
        } else {
            12.0
        };

        Ok(EeText {
            text,
            x,
            y,
            rotation,
            font_size,
            stroke_width: 0.0,
            layer_id: 0,
        })
    }

    fn parse_path(fields: &[&str]) -> Result<EePath> {
        if fields.len() < 4 {
            return Err(EasyedaError::InvalidData("Invalid path data".to_string()).into());
        }

        Ok(EePath {
            path_data: fields[3].to_string(),
            stroke_width: fields[1].parse::<f64>().unwrap_or(1.0),
            fill: false,
        })
    }

    fn parse_points(points_str: &str) -> Result<Vec<(f64, f64)>> {
        let coords: Vec<&str> = points_str.split_whitespace().collect();
        let mut points = Vec::new();

        for i in (0..coords.len()).step_by(2) {
            if i + 1 < coords.len() {
                let x = coords[i]
                    .parse::<f64>()
                    .map_err(|_| EasyedaError::InvalidData("Invalid point X".to_string()))?;
                let y = coords[i + 1]
                    .parse::<f64>()
                    .map_err(|_| EasyedaError::InvalidData("Invalid point Y".to_string()))?;
                points.push((x, y));
            }
        }

        Ok(points)
    }
}
