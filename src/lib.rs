//! # gerber2svg
//!
//! A library for converting Gerber (RS-274X) files to SVG format.
//!
//! This crate provides functionality to parse Gerber files and generate
//! corresponding SVG representations for visualization and further processing.

use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;

use gerber_parser::parse;
use gerber_parser::GerberDoc;
use gerber_types::{Aperture, Command, Coordinates, ExtendedCode, GCode, InterpolationMode};
use gerber_types::{CoordinateOffset, FunctionCode};

use svg::node::element::{path, Circle, Group, Path, Polygon, Rectangle};

mod point;
use crate::point::Point;

#[derive(Debug, Clone)]
enum Token {
    Number(f64),
    Variable(u32),
    Operator(char),
    LeftParen,
    RightParen,
}

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

#[allow(clippy::struct_excessive_bools)]
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
    aperture_rotation: f64,
    aperture_scaling: f64,
    aperture_mirror_x: bool,
    aperture_mirror_y: bool,

    step_repeat_active: bool,
    step_repeat_x: u32,
    step_repeat_y: u32,
    step_repeat_offset_x: f64,
    step_repeat_offset_y: f64,
    step_repeat_commands: Vec<Command>,

    aperture_macros: HashMap<String, gerber_types::ApertureMacro>,
    block_apertures: HashMap<i32, Vec<Command>>,
    current_block_commands: Vec<Command>,
    block_definition_active: bool,
    current_block_code: Option<i32>,

    min_x: f32,
    max_x: f32,
    min_y: f32,
    max_y: f32,
}

impl Gerber2SVG {
    /// Creates a new `Gerber2SVG` instance from a Gerber file.
    ///
    /// # Arguments
    /// * `filename` - Path to the Gerber file to be converted
    ///
    /// # Errors
    /// Returns an error if the file cannot be opened or parsed
    #[allow(clippy::missing_errors_doc)]
    pub fn from_file(filename: &str) -> Result<Self, std::io::Error> {
        let file = File::open(filename)?;
        let reader = BufReader::new(file);
        let gerber_doc = parse(reader).map_err(|(_, e)| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Parse error: {e:?}"),
            )
        })?;

        Ok(Self::from_gerber_doc(gerber_doc))
    }

    /// Creates a new `Gerber2SVG` instance from a parsed `GerberDoc`.
    ///
    /// # Arguments
    /// * `gerber_doc` - A parsed `GerberDoc` structure containing Gerber commands
    ///
    /// # Returns
    /// A new `Gerber2SVG` instance ready for configuration and building
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
            aperture_rotation: 0.0,
            aperture_scaling: 1.0,
            aperture_mirror_x: false,
            aperture_mirror_y: false,
            step_repeat_active: false,
            step_repeat_x: 1,
            step_repeat_y: 1,
            step_repeat_offset_x: 0.0,
            step_repeat_offset_y: 0.0,
            step_repeat_commands: Vec::new(),
            aperture_macros: HashMap::new(),
            block_apertures: HashMap::new(),
            current_block_commands: Vec::new(),
            block_definition_active: false,
            current_block_code: None,
            min_x: f32::MAX,
            max_x: f32::MIN,
            min_y: f32::MAX,
            max_y: f32::MIN,
        }
    }

    /// Sets the scaling factor for the SVG output.
    ///
    ///
    /// # Arguments
    ///
    /// # Returns
    /// Self for method chaining
    ///
    /// # Example
    /// ```no_run
    /// use gerber2svg::Gerber2SVG;
    /// let gerber = Gerber2SVG::from_file("example.gbr")?
    ///     .set_scale(2.0);
    /// # Ok::<(), std::io::Error>(())
    /// ```
    #[must_use]
    pub fn set_scale(mut self, scale: f32) -> Self {
        if scale > 0.0 {
            self.scale = scale;
        }
        self
    }

    /// Saves the generated SVG to a file.
    ///
    /// # Arguments
    /// * `filename` - Path where the SVG file will be saved
    ///
    /// # Errors
    /// Returns an error if the file cannot be written
    #[allow(clippy::missing_errors_doc)]
    pub fn save_svg(&self, filename: &str) -> Result<(), std::io::Error> {
        svg::save(filename, &self.svg_document)
    }

    /// Returns the SVG content as a string.
    ///
    /// This method converts the internal SVG document to its string representation,
    ///
    /// # Returns
    ///
    /// # Example
    /// ```no_run
    /// use gerber2svg::Gerber2SVG;
    /// let gerber = Gerber2SVG::from_file("example.gbr")?.build();
    /// let svg_content = gerber.to_string();
    /// # Ok::<(), std::io::Error>(())
    /// ```
    #[must_use]
    #[allow(clippy::inherent_to_string)]
    pub fn to_string(&self) -> String {
        self.svg_document.to_string()
    }

    /// Processes the Gerber commands and builds the SVG document.
    ///
    /// This method must be called after all configuration methods
    /// to generate the final SVG output.
    ///
    /// # Returns
    /// Self for method chaining
    #[must_use]
    pub fn build(mut self) -> Self {
        log::debug!("Start building...");

        let commands: Vec<_> = self
            .gerber_doc
            .commands
            .iter()
            .filter_map(|c| c.as_ref().ok().cloned())
            .collect();

        for command in commands {
            if self.step_repeat_active {
                self.step_repeat_commands.push(command.clone());
            }

            match command {
                Command::FunctionCode(f) => match f {
                    FunctionCode::DCode(d) => {
                        log::debug!("DCode: {d:?}");
                        match d {
                            gerber_types::DCode::Operation(op) => match op {
                                gerber_types::Operation::Interpolate(coord, offset) => {
                                    if let Some(c) = coord {
                                        self.add_draw_segment(&c, offset.as_ref());
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
                let macro_group = self.render_aperture_macro(name, params.as_ref());
                let positioned_group = Group::new()
                    .set(
                        "transform",
                        format!(
                            "translate({}, {})",
                            self.position.x * self.scale,
                            self.position.y * self.scale
                        ),
                    )
                    .add(macro_group);

                doc = doc.add(positioned_group);
            }
        }

        self.svg_document = doc;
    }

    fn add_draw_segment(&mut self, coord: &Coordinates, offset: Option<&CoordinateOffset>) {
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
            InterpolationMode::ClockwiseCircular | InterpolationMode::CounterclockwiseCircular => {
                if let Some(offset) = offset {
                    self.add_arc_segment(coord, offset);
                } else {
                    log::warn!("Arc interpolation requires offset coordinates");
                }
            }
        }

        self.position.x = target.0;
        self.position.y = target.1;
        self.check_bbox(transformed_target.0, transformed_target.1, 0.0, 0.0);
    }

    #[allow(clippy::cast_possible_truncation)]
    fn add_arc_segment(&mut self, coord: &Coordinates, offset: &CoordinateOffset) {
        let end_point = Self::coordinate_to_float(coord);
        let (i, j) = Self::coordinate_offset_to_float(offset);

        let radius = f64::from(i.mul_add(i, j * j)).sqrt();

        let sweep_flag = match self.draw_state {
            InterpolationMode::ClockwiseCircular => 1,
            InterpolationMode::CounterclockwiseCircular => 0,
            InterpolationMode::Linear => {
                log::warn!("Arc segment called with non-circular interpolation mode");
                return;
            }
        };

        match &mut self.drawing_state {
            DrawingState::Normal => {
                self.current_path_data = self.current_path_data.clone().elliptical_arc_to((
                    end_point.0,
                    end_point.1,
                    radius as f32,
                    radius as f32,
                    0.0,
                    0,
                    sweep_flag,
                ));
            }
            DrawingState::InRegion { path_data } => {
                *path_data = path_data.clone().elliptical_arc_to((
                    end_point.0,
                    end_point.1,
                    radius as f32,
                    radius as f32,
                    0.0,
                    0,
                    sweep_flag,
                ));
            }
        }

        self.position = Point {
            x: end_point.0,
            y: end_point.1,
        };
        self.check_bbox(end_point.0, end_point.1, 0.0, 0.0);
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
        if !self.current_path_data.is_empty() {
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
            ExtendedCode::ApertureMacro(macro_def) => {
                self.aperture_macros
                    .insert(macro_def.name.clone(), macro_def.clone());
                log::debug!("Registered aperture macro: {}", macro_def.name);
            }
            ExtendedCode::LoadPolarity(p) => {
                self.set_polarity(match p {
                    gerber_types::Polarity::Dark => Polarity::Dark,
                    gerber_types::Polarity::Clear => Polarity::Clear,
                });
            }
            ExtendedCode::StepAndRepeat(sr) => {
                self.handle_step_and_repeat(sr);
            }
            ExtendedCode::ApertureBlock(block) => {
                self.handle_aperture_block(block);
            }
            ExtendedCode::LoadMirroring(mirroring) => {
                self.aperture_mirror_x = matches!(
                    mirroring,
                    gerber_types::Mirroring::X | gerber_types::Mirroring::XY
                );
                self.aperture_mirror_y = matches!(
                    mirroring,
                    gerber_types::Mirroring::Y | gerber_types::Mirroring::XY
                );
                log::debug!(
                    "Set aperture mirroring: X={}, Y={}",
                    self.aperture_mirror_x,
                    self.aperture_mirror_y
                );
            }
            ExtendedCode::LoadRotation(rotation) => {
                self.aperture_rotation = rotation.rotation;
                log::debug!("Set aperture rotation: {}", self.aperture_rotation);
            }
            ExtendedCode::LoadScaling(scaling) => {
                self.aperture_scaling = scaling.scale;
                log::debug!("Set aperture scaling: {}", self.aperture_scaling);
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

                    let mut repeat_group =
                        Group::new().set("transform", format!("translate({offset_x}, {offset_y})"));

                    let saved_state = self.save_current_state();
                    let commands_to_repeat = self.step_repeat_commands.clone();
                    for cmd in &commands_to_repeat {
                        self.process_command_for_repeat(cmd, &mut repeat_group);
                    }
                    self.restore_state(saved_state);

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

    fn handle_aperture_block(&mut self, block: &gerber_types::ApertureBlock) {
        match block {
            gerber_types::ApertureBlock::Open { code } => {
                self.block_definition_active = true;
                self.current_block_code = Some(*code);
                self.current_block_commands.clear();
                log::debug!("Starting aperture block definition: {code}");
            }
            gerber_types::ApertureBlock::Close => {
                if let Some(code) = self.current_block_code.take() {
                    self.block_apertures
                        .insert(code, self.current_block_commands.clone());
                    log::debug!("Completed aperture block definition: {code}");
                }
                self.block_definition_active = false;
                self.current_block_commands.clear();
            }
        }
    }

    fn render_aperture_macro(
        &self,
        name: &str,
        params: Option<&Vec<gerber_types::MacroDecimal>>,
    ) -> Group {
        let mut group = Group::new();

        if let Some(macro_def) = self.aperture_macros.get(name) {
            for content in &macro_def.content {
                match content {
                    gerber_types::MacroContent::Circle(circle) => {
                        let circle_elem = self.render_macro_circle(circle, params);
                        group = group.add(circle_elem);
                    }
                    gerber_types::MacroContent::VectorLine(line) => {
                        let line_elem = self.render_macro_vector_line(line, params);
                        group = group.add(line_elem);
                    }
                    gerber_types::MacroContent::CenterLine(line) => {
                        let line_elem = self.render_macro_center_line(line, params);
                        group = group.add(line_elem);
                    }
                    gerber_types::MacroContent::Outline(outline) => {
                        let outline_elem = self.render_macro_outline(outline, params);
                        group = group.add(outline_elem);
                    }
                    gerber_types::MacroContent::Polygon(polygon) => {
                        let polygon_elem = self.render_macro_polygon(polygon, params);
                        group = group.add(polygon_elem);
                    }
                    gerber_types::MacroContent::Moire(moire) => {
                        let moire_elem = self.render_macro_moire(moire, params);
                        group = group.add(moire_elem);
                    }
                    gerber_types::MacroContent::Thermal(thermal) => {
                        let thermal_elem = self.render_macro_thermal(thermal, params);
                        group = group.add(thermal_elem);
                    }
                    _ => {
                        log::warn!("Unsupported macro primitive: {content:?}");
                    }
                }
            }
        } else {
            log::warn!("Aperture macro not found: {name}");
        }

        group
    }

    #[allow(clippy::only_used_in_recursion)]
    fn evaluate_macro_decimal(
        &self,
        value: &gerber_types::MacroDecimal,
        params: Option<&Vec<gerber_types::MacroDecimal>>,
    ) -> f64 {
        match value {
            gerber_types::MacroDecimal::Value(v) => *v,
            gerber_types::MacroDecimal::Variable(var_num) => params.map_or_else(
                || {
                    log::warn!("No parameters provided for macro variable ${var_num}");
                    0.0
                },
                |params| {
                    params
                        .get((*var_num as usize).saturating_sub(1))
                        .map_or_else(
                            || {
                                log::warn!("Macro variable ${var_num} not found in parameters");
                                0.0
                            },
                            |param| self.evaluate_macro_decimal(param, None),
                        )
                },
            ),
            gerber_types::MacroDecimal::Expression(expr) => {
                self.evaluate_macro_expression(expr, params)
            }
        }
    }

    fn evaluate_macro_boolean(
        &self,
        value: &gerber_types::MacroBoolean,
        params: Option<&Vec<gerber_types::MacroDecimal>>,
    ) -> bool {
        match value {
            gerber_types::MacroBoolean::Value(v) => *v,
            gerber_types::MacroBoolean::Variable(var_num) => params.map_or_else(
                || {
                    log::warn!("No parameters provided for macro variable ${var_num}");
                    false
                },
                |params| {
                    params
                        .get((*var_num as usize).saturating_sub(1))
                        .map_or_else(
                            || {
                                log::warn!("Macro variable ${var_num} not found in parameters");
                                false
                            },
                            |param| self.evaluate_macro_decimal(param, None) != 0.0,
                        )
                },
            ),
            gerber_types::MacroBoolean::Expression(expr) => {
                self.evaluate_macro_expression(expr, params) != 0.0
            }
        }
    }

    fn render_macro_circle(
        &self,
        circle: &gerber_types::CirclePrimitive,
        params: Option<&Vec<gerber_types::MacroDecimal>>,
    ) -> Circle {
        let diameter = self.evaluate_macro_decimal(&circle.diameter, params);
        let center_x = self.evaluate_macro_decimal(&circle.center.0, params);
        let center_y = self.evaluate_macro_decimal(&circle.center.1, params);
        let exposure = self.evaluate_macro_boolean(&circle.exposure, params);

        Circle::new()
            .set("cx", center_x * f64::from(self.scale))
            .set("cy", center_y * f64::from(self.scale))
            .set("r", (diameter / 2.0) * f64::from(self.scale))
            .set("fill", if exposure { "white" } else { "black" })
            .set("stroke", "none")
    }

    fn render_macro_vector_line(
        &self,
        line: &gerber_types::VectorLinePrimitive,
        params: Option<&Vec<gerber_types::MacroDecimal>>,
    ) -> Path {
        let width = self.evaluate_macro_decimal(&line.width, params);
        let start_x = self.evaluate_macro_decimal(&line.start.0, params);
        let start_y = self.evaluate_macro_decimal(&line.start.1, params);
        let end_x = self.evaluate_macro_decimal(&line.end.0, params);
        let end_y = self.evaluate_macro_decimal(&line.end.1, params);
        let exposure = self.evaluate_macro_boolean(&line.exposure, params);

        let path_data = path::Data::new()
            .move_to((
                start_x * f64::from(self.scale),
                start_y * f64::from(self.scale),
            ))
            .line_to((end_x * f64::from(self.scale), end_y * f64::from(self.scale)));

        Path::new()
            .set("d", path_data)
            .set("stroke", if exposure { "white" } else { "black" })
            .set("stroke-width", width * f64::from(self.scale))
            .set("stroke-linecap", "round")
            .set("fill", "none")
    }

    fn render_macro_center_line(
        &self,
        line: &gerber_types::CenterLinePrimitive,
        params: Option<&Vec<gerber_types::MacroDecimal>>,
    ) -> Rectangle {
        let width = self.evaluate_macro_decimal(&line.dimensions.0, params);
        let height = self.evaluate_macro_decimal(&line.dimensions.1, params);
        let center_x = self.evaluate_macro_decimal(&line.center.0, params);
        let center_y = self.evaluate_macro_decimal(&line.center.1, params);
        let exposure = self.evaluate_macro_boolean(&line.exposure, params);

        Rectangle::new()
            .set("x", (center_x - width / 2.0) * f64::from(self.scale))
            .set("y", (center_y - height / 2.0) * f64::from(self.scale))
            .set("width", width * f64::from(self.scale))
            .set("height", height * f64::from(self.scale))
            .set("fill", if exposure { "white" } else { "black" })
            .set("stroke", "none")
    }

    fn render_macro_outline(
        &self,
        outline: &gerber_types::OutlinePrimitive,
        params: Option<&Vec<gerber_types::MacroDecimal>>,
    ) -> Path {
        let exposure = self.evaluate_macro_boolean(&outline.exposure, params);
        let mut path_data = path::Data::new();

        if let Some(first_point) = outline.points.first() {
            let x = self.evaluate_macro_decimal(&first_point.0, params);
            let y = self.evaluate_macro_decimal(&first_point.1, params);
            path_data = path_data.move_to((x * f64::from(self.scale), y * f64::from(self.scale)));

            for point in outline.points.iter().skip(1) {
                let x = self.evaluate_macro_decimal(&point.0, params);
                let y = self.evaluate_macro_decimal(&point.1, params);
                path_data =
                    path_data.line_to((x * f64::from(self.scale), y * f64::from(self.scale)));
            }
        }

        Path::new()
            .set("d", path_data)
            .set("fill", if exposure { "white" } else { "black" })
            .set("stroke", "none")
            .set("fill-rule", "evenodd")
    }

    fn render_macro_polygon(
        &self,
        polygon: &gerber_types::PolygonPrimitive,
        params: Option<&Vec<gerber_types::MacroDecimal>>,
    ) -> Polygon {
        let center_x = self.evaluate_macro_decimal(&polygon.center.0, params);
        let center_y = self.evaluate_macro_decimal(&polygon.center.1, params);
        let diameter = self.evaluate_macro_decimal(&polygon.diameter, params);
        let exposure = self.evaluate_macro_boolean(&polygon.exposure, params);

        let vertices = if let gerber_types::MacroInteger::Value(v) = &polygon.vertices {
            *v
        } else {
            log::warn!("Variable vertices not supported for macro polygons");
            6
        };

        let radius = diameter / 2.0;
        let mut points = Vec::new();

        for i in 0..vertices {
            let angle = 2.0 * std::f64::consts::PI * f64::from(i) / f64::from(vertices);
            let x = radius.mul_add(angle.cos(), center_x);
            let y = radius.mul_add(angle.sin(), center_y);
            points.push(format!(
                "{},{}",
                x * f64::from(self.scale),
                y * f64::from(self.scale)
            ));
        }

        Polygon::new()
            .set("points", points.join(" "))
            .set("fill", if exposure { "white" } else { "black" })
            .set("stroke", "none")
    }

    fn evaluate_macro_expression(
        &self,
        expr: &str,
        params: Option<&Vec<gerber_types::MacroDecimal>>,
    ) -> f64 {
        let tokens = Self::tokenize_expression(expr);
        let resolved = self.resolve_variables(tokens, params);
        let rpn = Self::to_rpn(resolved);
        Self::evaluate_rpn(&rpn)
    }

    fn tokenize_expression(expr: &str) -> Vec<Token> {
        let mut tokens = Vec::new();
        let mut chars = expr.chars().peekable();

        while let Some(ch) = chars.next() {
            match ch {
                ' ' | '\t' => {}
                '(' => tokens.push(Token::LeftParen),
                ')' => tokens.push(Token::RightParen),
                '+' | '-' | '/' => tokens.push(Token::Operator(ch)),
                'x' | 'X' => tokens.push(Token::Operator('*')),
                '$' => {
                    let mut var_num = String::new();
                    while let Some(&digit) = chars.peek() {
                        if digit.is_ascii_digit() {
                            var_num.push(chars.next().unwrap());
                        } else {
                            break;
                        }
                    }
                    if let Ok(num) = var_num.parse::<u32>() {
                        tokens.push(Token::Variable(num));
                    }
                }
                _ if ch.is_ascii_digit() || ch == '.' => {
                    let mut number = String::new();
                    number.push(ch);
                    while let Some(&next_ch) = chars.peek() {
                        if next_ch.is_ascii_digit() || next_ch == '.' {
                            number.push(chars.next().unwrap());
                        } else {
                            break;
                        }
                    }
                    if let Ok(num) = number.parse::<f64>() {
                        tokens.push(Token::Number(num));
                    }
                }
                _ => {}
            }
        }

        tokens
    }

    fn resolve_variables(
        &self,
        tokens: Vec<Token>,
        params: Option<&Vec<gerber_types::MacroDecimal>>,
    ) -> Vec<Token> {
        tokens
            .into_iter()
            .map(|token| match token {
                Token::Variable(var_num) => {
                    let value = params.map_or_else(
                        || {
                            log::warn!("No parameters provided for macro variable ${var_num}");
                            0.0
                        },
                        |params| {
                            params
                                .get((var_num as usize).saturating_sub(1))
                                .map_or_else(
                                    || {
                                        log::warn!(
                                            "Macro variable ${var_num} not found in parameters"
                                        );
                                        0.0
                                    },
                                    |param| self.evaluate_macro_decimal(param, None),
                                )
                        },
                    );
                    Token::Number(value)
                }
                other => other,
            })
            .collect()
    }

    fn to_rpn(tokens: Vec<Token>) -> Vec<Token> {
        let mut output = Vec::new();
        let mut operators = Vec::new();

        for token in tokens {
            match token {
                Token::Number(_) => output.push(token),
                Token::Operator(op) => {
                    while let Some(Token::Operator(top_op)) = operators.last() {
                        let precedence = |o: char| match o {
                            '+' | '-' => 1,
                            '*' | '/' => 2,
                            _ => 0,
                        };

                        if precedence(*top_op) >= precedence(op) {
                            output.push(operators.pop().unwrap());
                        } else {
                            break;
                        }
                    }
                    operators.push(token);
                }
                Token::LeftParen => operators.push(token),
                Token::RightParen => {
                    while let Some(op) = operators.pop() {
                        if matches!(op, Token::LeftParen) {
                            break;
                        }
                        output.push(op);
                    }
                }
                Token::Variable(_) => {}
            }
        }

        while let Some(op) = operators.pop() {
            output.push(op);
        }

        output
    }

    fn evaluate_rpn(rpn: &[Token]) -> f64 {
        let mut stack = Vec::new();

        for token in rpn {
            match token {
                Token::Number(n) => stack.push(*n),
                Token::Operator(op) => {
                    if stack.len() >= 2 {
                        let b = stack.pop().unwrap();
                        let a = stack.pop().unwrap();
                        let result = match op {
                            '+' => a + b,
                            '-' => a - b,
                            '*' => a * b,
                            '/' => {
                                if b == 0.0 {
                                    0.0
                                } else {
                                    a / b
                                }
                            }
                            _ => 0.0,
                        };
                        stack.push(result);
                    }
                }
                Token::Variable(_) | Token::LeftParen | Token::RightParen => {}
            }
        }

        stack.pop().unwrap_or(0.0)
    }

    fn render_macro_moire(
        &self,
        moire: &gerber_types::MoirePrimitive,
        params: Option<&Vec<gerber_types::MacroDecimal>>,
    ) -> Group {
        let mut group = Group::new();

        let center_x = self.evaluate_macro_decimal(&moire.center.0, params);
        let center_y = self.evaluate_macro_decimal(&moire.center.1, params);
        let outer_diameter = self.evaluate_macro_decimal(&moire.diameter, params);
        let ring_thickness = self.evaluate_macro_decimal(&moire.ring_thickness, params);
        let gap = self.evaluate_macro_decimal(&moire.gap, params);
        let max_rings = moire.max_rings;
        let crosshair_thickness = self.evaluate_macro_decimal(&moire.cross_hair_thickness, params);
        let crosshair_length = self.evaluate_macro_decimal(&moire.cross_hair_length, params);
        let rotation = self.evaluate_macro_decimal(&moire.angle, params);

        if rotation != 0.0 {
            group = group.set(
                "transform",
                format!(
                    "rotate({} {} {})",
                    rotation,
                    center_x * f64::from(self.scale),
                    center_y * f64::from(self.scale)
                ),
            );
        }

        let mut current_diameter = outer_diameter;
        for _i in 0..max_rings {
            if current_diameter <= 0.0 {
                break;
            }

            let outer_circle = Circle::new()
                .set("cx", center_x * f64::from(self.scale))
                .set("cy", center_y * f64::from(self.scale))
                .set("r", (current_diameter / 2.0) * f64::from(self.scale))
                .set("fill", "white")
                .set("stroke", "none");

            group = group.add(outer_circle);

            let inner_diameter = ring_thickness.mul_add(-2.0, current_diameter);
            if inner_diameter > 0.0 {
                let inner_circle = Circle::new()
                    .set("cx", center_x * f64::from(self.scale))
                    .set("cy", center_y * f64::from(self.scale))
                    .set("r", (inner_diameter / 2.0) * f64::from(self.scale))
                    .set("fill", "black")
                    .set("stroke", "none");

                group = group.add(inner_circle);
            }

            current_diameter -= (ring_thickness + gap) * 2.0;
        }

        if crosshair_length > 0.0 && crosshair_thickness > 0.0 {
            let h_line = Rectangle::new()
                .set(
                    "x",
                    (center_x - crosshair_length / 2.0) * f64::from(self.scale),
                )
                .set(
                    "y",
                    (center_y - crosshair_thickness / 2.0) * f64::from(self.scale),
                )
                .set("width", crosshair_length * f64::from(self.scale))
                .set("height", crosshair_thickness * f64::from(self.scale))
                .set("fill", "white")
                .set("stroke", "none");

            let v_line = Rectangle::new()
                .set(
                    "x",
                    (center_x - crosshair_thickness / 2.0) * f64::from(self.scale),
                )
                .set(
                    "y",
                    (center_y - crosshair_length / 2.0) * f64::from(self.scale),
                )
                .set("width", crosshair_thickness * f64::from(self.scale))
                .set("height", crosshair_length * f64::from(self.scale))
                .set("fill", "white")
                .set("stroke", "none");

            group = group.add(h_line).add(v_line);
        }

        group
    }

    fn render_macro_thermal(
        &self,
        thermal: &gerber_types::ThermalPrimitive,
        params: Option<&Vec<gerber_types::MacroDecimal>>,
    ) -> Group {
        let mut group = Group::new();

        let center_x = self.evaluate_macro_decimal(&thermal.center.0, params);
        let center_y = self.evaluate_macro_decimal(&thermal.center.1, params);
        let outer_diameter = self.evaluate_macro_decimal(&thermal.outer_diameter, params);
        let inner_diameter = self.evaluate_macro_decimal(&thermal.inner_diameter, params);
        let gap = self.evaluate_macro_decimal(&thermal.gap, params);
        let rotation = self.evaluate_macro_decimal(&thermal.angle, params);

        let outer_circle = Circle::new()
            .set("cx", center_x * f64::from(self.scale))
            .set("cy", center_y * f64::from(self.scale))
            .set("r", (outer_diameter / 2.0) * f64::from(self.scale))
            .set("fill", "white")
            .set("stroke", "none");

        let inner_circle = Circle::new()
            .set("cx", center_x * f64::from(self.scale))
            .set("cy", center_y * f64::from(self.scale))
            .set("r", (inner_diameter / 2.0) * f64::from(self.scale))
            .set("fill", "black")
            .set("stroke", "none");

        group = group.add(outer_circle).add(inner_circle);

        for i in 0..4 {
            let angle = f64::from(i).mul_add(90.0, rotation);
            let gap_rect = Rectangle::new()
                .set("x", (center_x - gap / 2.0) * f64::from(self.scale))
                .set(
                    "y",
                    (center_y - outer_diameter / 2.0) * f64::from(self.scale),
                )
                .set("width", gap * f64::from(self.scale))
                .set("height", outer_diameter * f64::from(self.scale))
                .set("fill", "black")
                .set("stroke", "none")
                .set(
                    "transform",
                    format!(
                        "rotate({} {} {})",
                        angle,
                        center_x * f64::from(self.scale),
                        center_y * f64::from(self.scale)
                    ),
                );

            group = group.add(gap_rect);
        }

        group
    }

    fn save_current_state(&self) -> (svg::node::element::path::Data, f32, f32) {
        (
            self.current_path_data.clone(),
            self.position.x,
            self.position.y,
        )
    }

    fn restore_state(&mut self, state: (svg::node::element::path::Data, f32, f32)) {
        self.current_path_data = state.0;
        self.position.x = state.1;
        self.position.y = state.2;
    }

    fn process_command_for_repeat(&mut self, command: &Command, group: &mut Group) {
        if let Command::FunctionCode(FunctionCode::DCode(gerber_types::DCode::Operation(
            gerber_types::Operation::Flash(coord),
        ))) = command
        {
            if let Some(c) = coord {
                self.move_position(c);
            }
            if let Some(aperture) = &self.selected_aperture {
                let (x, y) = self.apply_transformations(self.position.x, self.position.y);
                if let gerber_types::Aperture::Circle(circle) = aperture {
                    let svg_circle = svg::node::element::Circle::new()
                        .set("cx", f64::from(x))
                        .set("cy", f64::from(y))
                        .set("r", circle.diameter / 2.0 * f64::from(self.scale))
                        .set("fill", "white");
                    *group = std::mem::replace(group, Group::new()).add(svg_circle);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use tempfile;

    fn create_test_gerber_file() -> tempfile::NamedTempFile {
        let content = r#"G04 Test Gerber file*
%FSLAX36Y36*%
%MOMM*%
%ADD10C,0.1*%
G01*
X0Y0D02*
X1000000Y0D01*
M02*"#;

        let mut temp_file = tempfile::NamedTempFile::new().unwrap();
        temp_file.write_all(content.as_bytes()).unwrap();
        temp_file.flush().unwrap();
        temp_file
    }

    #[test]
    fn test_from_file_success() {
        let temp_file = create_test_gerber_file();
        let result = Gerber2SVG::from_file(temp_file.path().to_str().unwrap());
        assert!(result.is_ok());
    }

    #[test]
    fn test_from_file_nonexistent() {
        let result = Gerber2SVG::from_file("nonexistent.gbr");
        assert!(result.is_err());
    }

    #[test]
    fn test_set_scale_positive() {
        let temp_file = create_test_gerber_file();
        let gerber = Gerber2SVG::from_file(temp_file.path().to_str().unwrap())
            .unwrap()
            .set_scale(2.0);
        assert_eq!(gerber.scale, 2.0);
    }

    #[test]
    fn test_set_scale_zero_or_negative() {
        let temp_file = create_test_gerber_file();
        let gerber = Gerber2SVG::from_file(temp_file.path().to_str().unwrap())
            .unwrap()
            .set_scale(-1.0);
        assert_eq!(gerber.scale, 1.0);
    }

    #[test]
    fn test_build_and_to_string() {
        let temp_file = create_test_gerber_file();
        let gerber = Gerber2SVG::from_file(temp_file.path().to_str().unwrap())
            .unwrap()
            .build();
        let content = gerber.to_string();
        assert!(content.contains("<svg"));
    }

    #[test]
    fn test_save_svg() {
        let temp_gerber = create_test_gerber_file();
        let temp_svg = tempfile::NamedTempFile::new().unwrap();

        let gerber = Gerber2SVG::from_file(temp_gerber.path().to_str().unwrap())
            .unwrap()
            .build();
        let result = gerber.save_svg(temp_svg.path().to_str().unwrap());
        assert!(result.is_ok());

        let content = fs::read_to_string(temp_svg.path()).unwrap();
        assert!(content.contains("<svg"));
    }

    #[test]
    fn test_basic_rs274x_features() {
        let gerber_content = r#"G04 Test Gerber file*
%FSLAX36Y36*%
%MOMM*%
%ADD10C,0.1*%
D10*
G01*
X0Y0D02*
X1000000Y0D01*
M02*"#;

        let temp_file = create_test_gerber_file_with_content(gerber_content);
        let gerber = Gerber2SVG::from_file(temp_file.path().to_str().unwrap())
            .unwrap()
            .build();
        let content = gerber.to_string();
        assert!(content.contains("<svg"));
    }

    #[test]
    fn test_aperture_macro_circle() {
        let gerber_content = r#"G04 Test Gerber with aperture macro*
%FSLAX36Y36*%
%MOMM*%
%AMCIRCLE*
1,1,$1,$2,$3*
%
%ADD10CIRCLE,0.5,0,0*%
D10*
X0Y0D03*
M02*"#;

        let temp_file = create_test_gerber_file_with_content(gerber_content);
        let result = Gerber2SVG::from_file(temp_file.path().to_str().unwrap());
        assert!(result.is_ok());

        let gerber = result.unwrap().build();
        let svg_content = gerber.to_string();
        assert!(svg_content.contains("<svg"));
    }

    #[test]
    fn test_aperture_block() {
        let gerber_content = r#"G04 Test Gerber with aperture block*
%FSLAX36Y36*%
%MOMM*%
%ADD10C,0.1*%
%AB102*%
D10*
X0Y0D03*
X1000000Y0D03*
%AB*%
M02*"#;

        let temp_file = create_test_gerber_file_with_content(gerber_content);
        let result = Gerber2SVG::from_file(temp_file.path().to_str().unwrap());
        assert!(result.is_ok());

        let gerber = result.unwrap().build();
        let svg_content = gerber.to_string();
        assert!(svg_content.contains("<svg"));
    }

    fn create_test_gerber_file_with_content(content: &str) -> tempfile::NamedTempFile {
        let mut temp_file = tempfile::NamedTempFile::new().unwrap();
        temp_file.write_all(content.as_bytes()).unwrap();
        temp_file.flush().unwrap();
        temp_file
    }

    #[test]
    fn test_macro_expression_evaluation() {
        let gerber_content = r#"G04 Test macro expressions*
%FSLAX25Y25*%
%MOMM*%
%AMTEST*
1,1,$1+$2,0,0*%
%ADD10TEST,1.0X2.0*%
D10*
X0Y0D03*
M02*"#;

        let temp_file = create_test_gerber_file_with_content(gerber_content);
        let gerber = Gerber2SVG::from_file(temp_file.path().to_str().unwrap())
            .unwrap()
            .build();
        let content = gerber.to_string();
        assert!(content.contains("<svg"));
    }

    #[test]
    fn test_arc_interpolation() {
        let gerber_content = r#"G04 Test arc interpolation*
%FSLAX25Y25*%
%MOMM*%
%ADD10C,0.1*%
D10*
G02*
X1000000Y0I500000J0D01*
G03*
X0Y0I-500000J0D01*
M02*"#;

        let temp_file = create_test_gerber_file_with_content(gerber_content);
        let gerber = Gerber2SVG::from_file(temp_file.path().to_str().unwrap())
            .unwrap()
            .build();
        let content = gerber.to_string();
        assert!(content.contains("<svg"));
    }

    #[test]
    fn test_moire_primitive() {
        let gerber_content = r#"G04 Test Moire primitive*
%FSLAX25Y25*%
%MOMM*%
%AMMOIRE*
6,0,0,5.0,0.5,0.1,3,0.1,1.0,0*%
%ADD10MOIRE*%
D10*
X0Y0D03*
M02*"#;

        let temp_file = create_test_gerber_file_with_content(gerber_content);
        let gerber = Gerber2SVG::from_file(temp_file.path().to_str().unwrap())
            .unwrap()
            .build();
        let content = gerber.to_string();
        assert!(content.contains("<svg"));
    }

    #[test]
    fn test_thermal_primitive() {
        let gerber_content = r#"G04 Test Thermal primitive*
%FSLAX25Y25*%
%MOMM*%
%AMTHERMAL*
7,0,0,2.0,1.0,0.2,0*%
%ADD10THERMAL*%
D10*
X0Y0D03*
M02*"#;

        let temp_file = create_test_gerber_file_with_content(gerber_content);
        let gerber = Gerber2SVG::from_file(temp_file.path().to_str().unwrap())
            .unwrap()
            .build();
        let content = gerber.to_string();
        assert!(content.contains("<svg"));
    }

    #[test]
    fn test_aperture_transformations() {
        let gerber_content = r#"G04 Test aperture transformations*
%FSLAX25Y25*%
%MOMM*%
%ADD10C,1.0*%
%LMX*%
%LR45*%
%LS2.0*%
D10*
X0Y0D03*
M02*"#;

        let temp_file = create_test_gerber_file_with_content(gerber_content);
        let gerber = Gerber2SVG::from_file(temp_file.path().to_str().unwrap())
            .unwrap()
            .build();
        let content = gerber.to_string();
        assert!(content.contains("<svg"));
    }
}
