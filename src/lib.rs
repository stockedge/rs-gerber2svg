//! 
//! 
//! 
//! 
//! use gerber2svg::Gerber2SVG;
//! 
//! 

use std::fs::File;
use std::io::BufReader;

use gerber_parser::GerberDoc;
use gerber_parser::parse;
use gerber_types::{Aperture, Command, Coordinates, ExtendedCode, GCode, InterpolationMode};
use gerber_types::{CoordinateOffset, FunctionCode};

use svg::node::element::{path, Circle, Group, Path, Polygon, Rectangle};

mod point;
use crate::point::Point;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Polarity {
    Dark,
    Clear,
}

#[derive(Clone, Debug)]
enum DrawingState {
    Normal,
    InRegion { path_data: path::Data },
}


pub struct Gerber2SVG {
    gerber_doc: GerberDoc,
    scale: f32,

    draw_state: InterpolationMode,
    drawing_state: DrawingState,
    position: Point,
    selected_aperture: Option<Aperture>,

    svg_document: svg::Document,
    current_path_data: path::Data,

    polarity: Polarity,
    mirror_x: bool,
    mirror_y: bool,
    rotation: f64,
    scaling: f64,

    step_repeat_active: bool,
    step_repeat_x: u32,
    step_repeat_y: u32,
    step_repeat_offset_x: f64,
    step_repeat_offset_y: f64,
    step_repeat_commands: Vec<Command>,


    min_x: f32,
    max_x: f32,
    min_y: f32,
    max_y: f32,
}

impl Gerber2SVG {
    /// Creates a new `Gerber2SVG` instance from a Gerber file.
    /// 
    /// 
    /// 
    #[allow(clippy::missing_errors_doc)]
    pub fn from_file(filename: &str) -> Result<Self, std::io::Error> {
        let file = File::open(filename)?;
        let reader = BufReader::new(file);
        let gerber_doc = parse(reader).map_err(|(_, e)| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, format!("Parse error: {e:?}"))
        })?;

        Ok(Self::from_gerber_doc(gerber_doc))
    }

    /// Creates a new `Gerber2SVG` instance from a parsed `GerberDoc`.
    /// 
    /// * `gerber_doc` - A parsed `GerberDoc` structure containing Gerber commands
    /// 
    /// 
    /// 
    #[must_use]
    pub fn from_gerber_doc(gerber_doc: GerberDoc) -> Self {
        Self {
            gerber_doc,
            scale: 1.0,
            draw_state: InterpolationMode::Linear,
            drawing_state: DrawingState::Normal,
            position: Point::new(0.0, 0.0),
            selected_aperture: None,
            svg_document: svg::Document::new(), //.set("viewbox", (0, 0, 80, 80)),
            current_path_data: path::Data::new(),
            polarity: Polarity::Dark,
            mirror_x: false,
            mirror_y: false,
            rotation: 0.0,
            scaling: 1.0,
            step_repeat_active: false,
            step_repeat_x: 1,
            step_repeat_y: 1,
            step_repeat_offset_x: 0.0,
            step_repeat_offset_y: 0.0,
            step_repeat_commands: Vec::new(),
            min_x: f32::MAX,
            max_x: f32::MIN,
            min_y: f32::MAX,
            max_y: f32::MIN,
        }
    }

    /// 
    /// 
    /// 
    /// use gerber2svg::Gerber2SVG;
    ///     .set_scale(2.0);
    #[must_use]
    pub fn set_scale(mut self, scale: f32) -> Self {
        if scale > 0.0 {
            self.scale = scale;
        }
        self
    }

    /// 
    /// 
    /// 
    /// use gerber2svg::Gerber2SVG;
    #[allow(clippy::missing_errors_doc)]
    pub fn save_svg(&self, filename: &str) -> Result<(), std::io::Error> {
        svg::save(filename, &self.svg_document)
    }

    /// 
    /// 
    /// use gerber2svg::Gerber2SVG;
    /// let svg_content = gerber.to_string();
    #[must_use]
    #[allow(clippy::inherent_to_string)]
    pub fn to_string(&self) -> String {
        self.svg_document.to_string()
    }

    /// 
    /// 
    /// 
    /// use gerber2svg::Gerber2SVG;
    ///     .set_scale(1.5)
    #[must_use]
    pub fn build(mut self) -> Self {
        log::debug!("Start building...");

        let commands: Vec<_> = self.gerber_doc.commands.iter()
            .filter_map(|c| c.as_ref().ok().cloned())
            .collect();
        
        for command in commands {
            match command {
                    Command::FunctionCode(f) => match f {
                        FunctionCode::DCode(d) => {
                        log::debug!("DCode: {d:?}");
                        match d {
                            gerber_types::DCode::Operation(op) => match op {
                                gerber_types::Operation::Interpolate(coord, offset) => {
                                    if let Some(offset) = offset {
                                        if let Some(c) = coord {
                                            self.add_arc_segment(&c, &offset);
                                        }
                                    } else if let Some(c) = coord {
                                        self.add_draw_segment(&c);
                                    }
                                }
                                gerber_types::Operation::Move(coord) => {
                                    if let Some(c) = coord {
                                        self.move_position(&c);
                                    }
                                }
                                gerber_types::Operation::Flash(coord) => {
                                    if let Some(c) = coord {
                                        self.move_position(&c);
                                    }
                                    self.place_aperture();
                                }
                            },
                            gerber_types::DCode::SelectAperture(a) => {
                                log::debug!("Select aperture: {a:?}");
                                self.selected_aperture = self.gerber_doc.apertures.get(&a).cloned();
                            }
                        }
                    }
                        FunctionCode::GCode(g) => match g {
                        GCode::InterpolationMode(im) => self.draw_state = im,
                        GCode::Comment(c) => log::info!("[COMMENT] \"{c:?}\""),
                        GCode::RegionMode(true) => self.handle_region_start(),
                        GCode::RegionMode(false) => self.handle_region_end(),
                        _ => log::error!("Unsupported GCode:\r\n{g:#?}"),
                    },
                        FunctionCode::MCode(_) => (),
                    },
                    Command::ExtendedCode(e) => {
                        self.handle_extended_code(&e);
                    }
            }
        }

        self.create_path_from_data();
        self.set_bbox();
        self
    }

    #[allow(
        clippy::too_many_lines,
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss
    )]
    fn place_aperture(&mut self) {
        let target = (self.position.x, self.position.y);
        let transformed_target = self.apply_transformations(target.0, target.1);
        let mut doc = std::mem::replace(&mut self.svg_document, svg::Document::new());

        let fill_color = match self.polarity {
            Polarity::Dark => "white",
            Polarity::Clear => "black",
        };

        match self
            .selected_aperture
            .as_ref()
            .expect("No aperture selected")
        {
            Aperture::Circle(c) => {
                let radius = (c.diameter / 2.0) * f64::from(self.scale) * self.scaling;
                let circle = Circle::new()
                    .set("cx", transformed_target.0)
                    .set("cy", transformed_target.1)
                    .set("r", radius)
                    .set("fill", fill_color);
                doc = doc.add(circle);
                #[allow(clippy::cast_possible_truncation)]
                self.check_bbox(
                    transformed_target.0,
                    transformed_target.1,
                    radius as f32,
                    radius as f32,
                );
            }
            Aperture::Rectangle(r) => {
                let width = r.x * f64::from(self.scale) * self.scaling;
                let height = r.y * f64::from(self.scale) * self.scaling;
                #[allow(clippy::cast_possible_truncation)]
                let x = transformed_target.0 - (width / 2.0) as f32;
                #[allow(clippy::cast_possible_truncation)]
                let y = transformed_target.1 - (height / 2.0) as f32;

                let rect = Rectangle::new()
                    .set("x", x)
                    .set("y", y)
                    .set("width", width)
                    .set("height", height)
                    .set("fill", fill_color);
                doc = doc.add(rect);
                #[allow(clippy::cast_possible_truncation)]
                self.check_bbox(
                    transformed_target.0,
                    transformed_target.1,
                    (width / 2.0) as f32,
                    (height / 2.0) as f32,
                );
            }
            Aperture::Obround(o) => {
                let width = o.x * f64::from(self.scale) * self.scaling;
                let height = o.y * f64::from(self.scale) * self.scaling;
                let radius = (width.min(height) / 2.0) as f32;

                let mut path_data = path::Data::new();
                if width >= height {
                    let rect_width = width - height;
                    path_data = path_data
                        .move_to((
                            transformed_target.0 - rect_width as f32 / 2.0,
                            transformed_target.1 - radius,
                        ))
                        .line_to((
                            transformed_target.0 + rect_width as f32 / 2.0,
                            transformed_target.1 - radius,
                        ))
                        .elliptical_arc_to((
                            transformed_target.0 + rect_width as f32 / 2.0,
                            transformed_target.1 + radius,
                            radius,
                            radius,
                            0.0,
                        ))
                        .line_to((
                            transformed_target.0 - rect_width as f32 / 2.0,
                            transformed_target.1 + radius,
                        ))
                        .elliptical_arc_to((
                            transformed_target.0 - rect_width as f32 / 2.0,
                            transformed_target.1 - radius,
                            radius,
                            radius,
                            0.0,
                        ))
                        .close();
                } else {
                    let rect_height = height - width;
                    path_data = path_data
                        .move_to((
                            transformed_target.0 - radius,
                            transformed_target.1 - rect_height as f32 / 2.0,
                        ))
                        .line_to((
                            transformed_target.0 - radius,
                            transformed_target.1 + rect_height as f32 / 2.0,
                        ))
                        .elliptical_arc_to((
                            transformed_target.0 + radius,
                            transformed_target.1 + rect_height as f32 / 2.0,
                            radius,
                            radius,
                            0.0,
                        ))
                        .line_to((
                            transformed_target.0 + radius,
                            transformed_target.1 - rect_height as f32 / 2.0,
                        ))
                        .elliptical_arc_to((
                            transformed_target.0 - radius,
                            transformed_target.1 - rect_height as f32 / 2.0,
                            radius,
                            radius,
                            0.0,
                        ))
                        .close();
                }

                let path = Path::new().set("fill", fill_color).set("d", path_data);
                doc = doc.add(path);
                #[allow(clippy::cast_possible_truncation)]
                self.check_bbox(
                    transformed_target.0,
                    transformed_target.1,
                    (width / 2.0) as f32,
                    (height / 2.0) as f32,
                );
            }
            Aperture::Polygon(p) => {
                let radius = (p.diameter / 2.0) * f64::from(self.scale) * self.scaling;
                let vertices = p.vertices as usize;
                let rotation_offset = p.rotation.unwrap_or(0.0) + self.rotation;

                let mut points = Vec::new();
                for i in 0..vertices {
                    let angle = (i as f64 * 2.0 * std::f64::consts::PI / vertices as f64)
                        + rotation_offset.to_radians();
                    let x = transformed_target.0 + (radius * angle.cos()) as f32;
                    let y = transformed_target.1 + (radius * angle.sin()) as f32;
                    points.push(format!("{x},{y}"));
                }

                let polygon = Polygon::new()
                    .set("points", points.join(" "))
                    .set("fill", fill_color);

                doc = doc.add(polygon);
                #[allow(clippy::cast_possible_truncation)]
                self.check_bbox(
                    transformed_target.0,
                    transformed_target.1,
                    radius as f32,
                    radius as f32,
                );
            }
            Aperture::Macro(name, params) => {
                log::warn!("Aperture macro not yet supported: {name} with params {params:?}");
            }
        }

        self.svg_document = doc;
    }

    fn add_draw_segment(&mut self, coord: &Coordinates) {
        let target = Self::coordinate_to_float(coord);
        let transformed_target = self.apply_transformations(target.0, target.1);

        match self.draw_state {
            InterpolationMode::Linear => match &mut self.drawing_state {
                DrawingState::Normal => {
                    self.current_path_data = self
                        .current_path_data
                        .clone()
                        .line_to((transformed_target.0, transformed_target.1));
                }
                DrawingState::InRegion { path_data } => {
                    *path_data = path_data
                        .clone()
                        .line_to((transformed_target.0, transformed_target.1));
                }
            },
            InterpolationMode::ClockwiseCircular => {
                log::warn!("Clockwise circular interpolation not yet supported");
            }
            InterpolationMode::CounterclockwiseCircular => {
                log::warn!("Counterclockwise circular interpolation not yet supported");
            }
        }

        self.position.x = target.0;
        self.position.y = target.1;
        self.check_bbox(transformed_target.0, transformed_target.1, 0.0, 0.0);
    }

    fn add_arc_segment(&self, coord: &Coordinates, offset: &CoordinateOffset) {
        log::debug!(
            "Draw arc from {:?} to {:?} with offset {:?}",
            self.position,
            Self::coordinate_to_float(coord),
            Self::coordinate_offset_to_float(offset)
        );
    }

    fn move_position(&mut self, coord: &Coordinates) {
        let target = Self::coordinate_to_float(coord);
        let transformed_target = self.apply_transformations(target.0, target.1);

        match &mut self.drawing_state {
            DrawingState::Normal => {
                self.current_path_data = self
                    .current_path_data
                    .clone()
                    .move_to((transformed_target.0, transformed_target.1));
            }
            DrawingState::InRegion { path_data } => {
                *path_data = path_data
                    .clone()
                    .move_to((transformed_target.0, transformed_target.1));
            }
        }

        self.position.x = target.0;
        self.position.y = target.1;
    }

    fn create_path_from_data(&mut self) {
        if !format!("{:?}", self.current_path_data).is_empty() {
            let mut doc = std::mem::replace(&mut self.svg_document, svg::Document::new());

            let stroke_color = self.get_path_stroke();
            let path = Path::new()
                .set("fill", "none")
                .set("stroke", stroke_color)
                .set("stroke-width", 0.1)
                .set("d", self.current_path_data.clone());

            doc = doc.add(path);
            self.svg_document = doc;
            self.current_path_data = path::Data::new();
        }
    }

    const fn get_path_stroke(&self) -> &str {
        match self.polarity {
            Polarity::Dark => "white",
            Polarity::Clear => "black",
        }
    }

    #[allow(clippy::cast_possible_truncation)]
    fn coordinate_to_float(coord: &Coordinates) -> (f32, f32) {
        let x = coord.x.map_or(0.0, |x| Into::<f64>::into(x) as f32);
        let y = coord.y.map_or(0.0, |y| Into::<f64>::into(y) as f32);
        (x, y)
    }

    #[allow(clippy::cast_possible_truncation)]
    fn coordinate_offset_to_float(offset: &CoordinateOffset) -> (f32, f32) {
        let x = offset.x.map_or(0.0, |x| Into::<f64>::into(x) as f32);
        let y = offset.y.map_or(0.0, |y| Into::<f64>::into(y) as f32);
        (x, y)
    }

    fn check_bbox(&mut self, x: f32, y: f32, width: f32, height: f32) {
        self.min_x = self.min_x.min(x - width);
        self.max_x = self.max_x.max(x + width);
        self.min_y = self.min_y.min(y - height);
        self.max_y = self.max_y.max(y + height);
    }

    fn set_bbox(&mut self) {
        if self.min_x != f32::MAX {
            let width = self.max_x - self.min_x;
            let height = self.max_y - self.min_y;
            let viewbox = format!("{} {} {} {}", self.min_x, self.min_y, width, height);
            self.svg_document = std::mem::replace(&mut self.svg_document, svg::Document::new())
                .set("viewBox", viewbox);
        }
    }

    fn handle_region_start(&mut self) {
        log::debug!("Starting region (G36)");
        self.create_path_from_data();
        self.drawing_state = DrawingState::InRegion {
            path_data: path::Data::new(),
        };
    }

    fn handle_region_end(&mut self) {
        log::debug!("Ending region (G37)");
        if let DrawingState::InRegion { path_data } = &self.drawing_state {
            let mut doc = std::mem::replace(&mut self.svg_document, svg::Document::new());

            let fill_color = match self.polarity {
                Polarity::Dark => "white",
                Polarity::Clear => "black",
            };

            let path = Path::new()
                .set("fill", fill_color)
                .set("stroke", "none")
                .set("fill-rule", "evenodd")
                .set("d", path_data.clone());

            doc = doc.add(path);
            self.svg_document = doc;
            self.drawing_state = DrawingState::Normal;
        }
    }

    fn handle_extended_code(&mut self, extended_code: &ExtendedCode) {
        match extended_code {
            ExtendedCode::LoadPolarity(p) => {
                self.set_polarity(match p {
                    gerber_types::Polarity::Dark => Polarity::Dark,
                    gerber_types::Polarity::Clear => Polarity::Clear,
                });
            }
            ExtendedCode::StepAndRepeat(sr) => {
                self.handle_step_and_repeat(sr);
            }
            _ => {
                log::debug!("Unsupported extended code: {extended_code:#?}");
            }
        }
    }

    fn set_polarity(&mut self, polarity: Polarity) {
        if self.polarity != polarity {
            self.create_path_from_data();
            self.polarity = polarity;
        }
    }

    #[allow(clippy::cast_possible_truncation, clippy::suboptimal_flops)]
    fn apply_transformations(&self, x: f32, y: f32) -> (f32, f32) {
        let mut tx = x;
        let mut ty = y;

        if self.mirror_x {
            tx = -tx;
        }
        if self.mirror_y {
            ty = -ty;
        }

        if self.rotation != 0.0 {
            let cos_r = self.rotation.to_radians().cos() as f32;
            let sin_r = self.rotation.to_radians().sin() as f32;
            let new_x = tx * cos_r - ty * sin_r;
            let new_y = tx * sin_r + ty * cos_r;
            tx = new_x;
            ty = new_y;
        }

        tx *= self.scaling as f32;
        ty *= self.scaling as f32;

        (tx, ty)
    }

    fn finalize_step_and_repeat(&mut self) {
        if self.step_repeat_active && (!self.step_repeat_commands.is_empty()) {
            let mut doc = std::mem::replace(&mut self.svg_document, svg::Document::new());

            for x in 0..self.step_repeat_x {
                for y in 0..self.step_repeat_y {
                    if x == 0 && y == 0 {
                        continue;
                    }

                    let offset_x = f64::from(x) * self.step_repeat_offset_x;
                    let offset_y = f64::from(y) * self.step_repeat_offset_y;

                    let repeat_group =
                        Group::new().set("transform", format!("translate({offset_x}, {offset_y})"));

                    doc = doc.add(repeat_group);
                }
            }

            self.svg_document = doc;
            self.step_repeat_active = false;
            self.step_repeat_commands.clear();
        }
    }




    fn handle_step_and_repeat(&mut self, sr: &gerber_types::StepAndRepeat) {
        match sr {
            gerber_types::StepAndRepeat::Open {
                repeat_x,
                repeat_y,
                distance_x,
                distance_y,
            } => {
                self.step_repeat_active = true;
                self.step_repeat_x = *repeat_x;
                self.step_repeat_y = *repeat_y;
                self.step_repeat_offset_x = *distance_x;
                self.step_repeat_offset_y = *distance_y;
                self.step_repeat_commands.clear();
                log::debug!(
                    "Starting step and repeat: {repeat_x}x{repeat_y} with offset ({distance_x}, {distance_y})"
                );
            }
            gerber_types::StepAndRepeat::Close => {
                self.finalize_step_and_repeat();
                log::debug!("Ending step and repeat");
            }
        }
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn create_test_gerber_file() -> String {
        let filename = format!("test_{}.gbr", std::process::id());
        let content = r#"G04 Test Gerber file*
%FSLAX36Y36*%
%MOMM*%
%ADD10C,0.1*%
G01*
X0Y0D02*
X1000000Y0D01*
M02*"#;
        fs::write(&filename, content).unwrap();

        assert!(
            std::path::Path::new(&filename).exists(),
            "Test file was not created successfully"
        );
        filename
    }

    #[test]
    fn test_from_file_success() {
        let filename = create_test_gerber_file();
        let result = Gerber2SVG::from_file(&filename);
        assert!(result.is_ok());
        let _ = fs::remove_file(&filename);
    }

    #[test]
    fn test_from_file_nonexistent() {
        let result = Gerber2SVG::from_file("nonexistent.gbr");
        assert!(result.is_err());
    }

    #[test]
    fn test_set_scale_positive() {
        let filename = create_test_gerber_file();
        let gerber = Gerber2SVG::from_file(&filename).unwrap().set_scale(2.0);
        assert_eq!(gerber.scale, 2.0);
        let _ = fs::remove_file(&filename);
    }

    #[test]
    fn test_set_scale_zero_or_negative() {
        let filename = create_test_gerber_file();
        let gerber = Gerber2SVG::from_file(&filename).unwrap().set_scale(-1.0);
        assert_eq!(gerber.scale, 1.0);
        let _ = fs::remove_file(&filename);
    }

    #[test]
    fn test_build_and_to_string() {
        let filename = create_test_gerber_file();
        let gerber = Gerber2SVG::from_file(&filename).unwrap().build();
        let content = gerber.to_string();
        assert!(content.contains("<svg"));
        let _ = fs::remove_file(&filename);
    }

    #[test]
    fn test_save_svg() {
        let filename = create_test_gerber_file();
        let output_filename = format!("test_output_{}.svg", std::process::id());
        let gerber = Gerber2SVG::from_file(&filename).unwrap().build();
        let result = gerber.save_svg(&output_filename);
        assert!(result.is_ok());

        let content = fs::read_to_string(&output_filename).unwrap();
        assert!(content.contains("<svg"));

        let _ = fs::remove_file(&filename);
        let _ = fs::remove_file(&output_filename);
    }

    #[test]
    fn test_basic_rs274x_features() {
        let filename = create_test_gerber_file();
        let gerber = Gerber2SVG::from_file(&filename).unwrap().build();
        let content = gerber.to_string();
        assert!(content.contains("<svg"));
        let _ = fs::remove_file(&filename);
    }
}
