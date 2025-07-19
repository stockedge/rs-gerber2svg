use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;

use gerber_parser::gerber_doc::GerberDoc;
use gerber_parser::parser::parse_gerber;
use gerber_types::{Aperture, Command, Coordinates, ExtendedCode, GCode, InterpolationMode};
use gerber_types::{CoordinateOffset, FunctionCode};

use svg;
use svg::node::element::{path, Circle, Group, Path, Rectangle, Polygon};

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

#[derive(Debug, Clone)]
pub enum MacroPrimitive {
    Comment(String),
    Circle {
        exposure: bool,
        diameter: f64,
        center_x: f64,
        center_y: f64,
        rotation: Option<f64>,
    },
    VectorLine {
        exposure: bool,
        width: f64,
        start_x: f64,
        start_y: f64,
        end_x: f64,
        end_y: f64,
        rotation: Option<f64>,
    },
    CenterLine {
        exposure: bool,
        width: f64,
        height: f64,
        center_x: f64,
        center_y: f64,
        rotation: Option<f64>,
    },
    Outline {
        exposure: bool,
        points: Vec<(f64, f64)>,
        rotation: Option<f64>,
    },
    Polygon {
        exposure: bool,
        vertices: u32,
        center_x: f64,
        center_y: f64,
        diameter: f64,
        rotation: Option<f64>,
    },
    Thermal {
        center_x: f64,
        center_y: f64,
        outer_diameter: f64,
        inner_diameter: f64,
        gap: f64,
        rotation: Option<f64>,
    },
}

#[derive(Debug, Clone)]
pub struct ApertureMacro {
    pub name: String,
    pub primitives: Vec<MacroPrimitive>,
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

    aperture_macros: HashMap<String, ApertureMacro>,
    block_apertures: HashMap<String, Vec<Command>>,
    attributes: HashMap<String, String>,

    min_x: f32,
    max_x: f32,
    min_y: f32,
    max_y: f32,
}

impl Gerber2SVG {
    /// Create instance from a Gerber file 
    /// * filename: `&str` path to the gerber file
    pub fn from_file(filename: &str) -> Result<Self, std::io::Error> {
        let file = File::open(filename)?;
        let reader = BufReader::new(file);
        let gerber_doc: GerberDoc = parse_gerber(reader);

        Ok(Self::from_gerber_doc(gerber_doc))
    }

    /// Create Instance form GerberDoc struct
    /// * gerber_doc: `GerberDoc` struct
    pub fn from_gerber_doc(gerber_doc: GerberDoc) -> Self {
        let s = Self {
            gerber_doc: gerber_doc,
            scale: 1.0,
            draw_state: InterpolationMode::Linear,
            drawing_state: DrawingState::Normal,
            position: Point::new(0.0, 0.0),
            selected_aperture: None,
            svg_document: svg::Document::new(),
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
            aperture_macros: HashMap::new(),
            block_apertures: HashMap::new(),
            attributes: HashMap::new(),
            min_x: f32::INFINITY,
            max_x: f32::NEG_INFINITY,
            min_y: f32::INFINITY,
            max_y: f32::NEG_INFINITY,
        };

        return s;
    }

    
    /// Set the scale of the path and aperture. (Must be called **before** the build function)
    /// * scale : `f32` the scale value (> 0.0)
    pub fn set_scale(mut self, scale: f32) -> Self {
        
        if scale > 0.0 {
            self.scale = scale;
        }
        else{
            log::warn!("Scale value need to be greater than 0.0. Skip scale setting");
        }

        return self;
    }

    /// Save the gerber as SVG file
    /// * filename: `&str` path to save the SVG file
    /// * crop: `bool` trim unused space
    pub fn save_svg(&mut self, filename: &str, crop: bool) -> std::io::Result<()> {
        self.set_bbox(crop);
        svg::save(filename, &self.svg_document)
    }

    /// Get SVG as String
    /// * crop: `bool` trim unused space
    pub fn to_string(&mut self, crop: bool) -> String {
        self.set_bbox(crop);
        self.svg_document.to_string()
    }

    /// Build the SVG
    pub fn build(mut self) -> Self {
        log::debug!("Start building...");
        for c in &self.gerber_doc.commands.clone() {
            match c {
                gerber_types::Command::FunctionCode(f) => {
                    match f {
                        FunctionCode::DCode(d) => match d {
                            gerber_types::DCode::Operation(o) => match o {
                                gerber_types::Operation::Interpolate(coord, offset) => {
                                    if self.draw_state == InterpolationMode::Linear {
                                        self.add_draw_segment(coord);
                                    } else {
                                        self.add_arc_segment(coord, offset.as_ref().expect(format!("No offset coord with 'Circular' state\r\n{:#?}", c).as_str()))
                                    }
                                    self.move_position(coord);
                                }
                                gerber_types::Operation::Move(m) => {
                                    log::debug!("Move to {:?}, create path.", &m);
                                    self.create_path_from_data();
                                    self.move_position(m);
                                }
                                gerber_types::Operation::Flash(f) => {
                                    self.create_path_from_data();
                                    self.place_aperture(f);
                                    self.move_position(f);
                                }
                            },
                            gerber_types::DCode::SelectAperture(i) => {
                                self.create_path_from_data();
                                self.selected_aperture = Some(
                                    self.gerber_doc
                                        .apertures
                                        .get(&i)
                                        .expect(format!("Unknown aperture id '{}'", i).as_str())
                                        .clone(),
                                )
                            }
                        },
                        FunctionCode::GCode(g) => match g {
                            GCode::InterpolationMode(im) => self.draw_state = *im,
                            GCode::Comment(c) => log::info!("[COMMENT] \"{}\"", c),
                            GCode::RegionMode(true) => self.handle_region_start(),
                            GCode::RegionMode(false) => self.handle_region_end(),
                            _ => log::debug!("Unsupported GCode: {:?}", g),
                        },
                        FunctionCode::MCode(_) => (),
                    }
                }
                Command::ExtendedCode(e) => {
                    self.handle_extended_code(e);
                }
            };
        }

        return self;
    }

    fn place_aperture(&mut self, coord: &Coordinates) -> () {
        let target = Self::coordinate_to_float(coord);
        let target = (
            target.0.unwrap_or(self.position.x),
            target.1.unwrap_or(self.position.y),
        );
        let transformed_target = self.apply_transformations(target.0, target.1);

        let mut doc = std::mem::replace(&mut self.svg_document, svg::Document::new());

        log::debug!(
            "Place aperture {:?} to {:?}",
            self.selected_aperture,
            &transformed_target
        );

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
                let radius = (c.diameter / 2.0) * self.scale as f64 * self.scaling;
                let circle = Circle::new()
                    .set("cx", transformed_target.0)
                    .set("cy", transformed_target.1)
                    .set("r", radius)
                    .set("fill", fill_color);
                doc = doc.add(circle);
                self.check_bbox(transformed_target.0, transformed_target.1, radius as f32, radius as f32);
            }
            Aperture::Rectangle(r) => {
                let width = r.x * self.scale as f64 * self.scaling;
                let height = r.y * self.scale as f64 * self.scaling;
                let x = transformed_target.0 - (width / 2.0) as f32;
                let y = transformed_target.1 - (height / 2.0) as f32;

                let rect = Rectangle::new()
                    .set("x", x)
                    .set("y", y)
                    .set("width", width)
                    .set("height", height)
                    .set("fill", fill_color);
                doc = doc.add(rect);
                self.check_bbox(transformed_target.0, transformed_target.1, (width / 2.0) as f32, (height / 2.0) as f32);
            }
            Aperture::Obround(o) => {
                let width = o.x * self.scale as f64 * self.scaling;
                let height = o.y * self.scale as f64 * self.scaling;
                let radius = (width.min(height) / 2.0) as f32;
                
                let mut path_data = path::Data::new();
                if width > height {
                    let rect_width = width - height;
                    path_data = path_data
                        .move_to((transformed_target.0 - rect_width as f32 / 2.0, transformed_target.1 - radius))
                        .line_to((transformed_target.0 + rect_width as f32 / 2.0, transformed_target.1 - radius))
                        .elliptical_arc_to((transformed_target.0 + rect_width as f32 / 2.0, transformed_target.1 + radius, radius, radius, 0.0))
                        .line_to((transformed_target.0 - rect_width as f32 / 2.0, transformed_target.1 + radius))
                        .elliptical_arc_to((transformed_target.0 - rect_width as f32 / 2.0, transformed_target.1 - radius, radius, radius, 0.0))
                        .close();
                } else {
                    let rect_height = height - width;
                    path_data = path_data
                        .move_to((transformed_target.0 - radius, transformed_target.1 - rect_height as f32 / 2.0))
                        .line_to((transformed_target.0 - radius, transformed_target.1 + rect_height as f32 / 2.0))
                        .elliptical_arc_to((transformed_target.0 + radius, transformed_target.1 + rect_height as f32 / 2.0, radius, radius, 0.0))
                        .line_to((transformed_target.0 + radius, transformed_target.1 - rect_height as f32 / 2.0))
                        .elliptical_arc_to((transformed_target.0 - radius, transformed_target.1 - rect_height as f32 / 2.0, radius, radius, 0.0))
                        .close();
                }
                
                let path = Path::new()
                    .set("fill", fill_color)
                    .set("d", path_data);
                doc = doc.add(path);
                self.check_bbox(transformed_target.0, transformed_target.1, (width / 2.0) as f32, (height / 2.0) as f32);
            }
            Aperture::Polygon(p) => {
                let radius = (p.diameter / 2.0) * self.scale as f64 * self.scaling;
                let vertices = p.vertices as usize;
                let rotation_offset = p.rotation.unwrap_or(0.0) + self.rotation;
                
                let mut points = Vec::new();
                for i in 0..vertices {
                    let angle = (i as f64 * 2.0 * std::f64::consts::PI / vertices as f64) + rotation_offset.to_radians();
                    let x = transformed_target.0 + (radius * angle.cos()) as f32;
                    let y = transformed_target.1 + (radius * angle.sin()) as f32;
                    points.push(format!("{},{}", x, y));
                }
                
                let polygon = Polygon::new()
                    .set("points", points.join(" "))
                    .set("fill", fill_color);
                doc = doc.add(polygon);
                self.check_bbox(transformed_target.0, transformed_target.1, radius as f32, radius as f32);
            }
            Aperture::Other(o) => {
                log::warn!("Other aperture type not yet supported: {:?}", o);
            }
        }

        self.svg_document = doc;
    }

    fn add_draw_segment(&mut self, coord: &Coordinates) -> () {
        let target = Self::coordinate_to_float(coord);
        let target = (
            target.0.unwrap_or(self.position.x),
            target.1.unwrap_or(self.position.y),
        );
        let transformed_target = self.apply_transformations(target.0, target.1);

        log::debug!("Draw segment from {:?} to {:?}", self.position, &transformed_target);

        match &mut self.drawing_state {
            DrawingState::InRegion { path_data } => {
                if path_data.is_empty() {
                    *path_data = std::mem::take(path_data).move_to((self.position.x, self.position.y));
                }
                *path_data = std::mem::take(path_data).line_to((transformed_target.0, transformed_target.1));
            }
            DrawingState::Normal => {
                let mut path = std::mem::take(&mut self.current_path_data);
                if path.is_empty() {
                    path = path.move_to((self.position.x, self.position.y));
                }
                self.current_path_data = path.line_to((transformed_target.0, transformed_target.1));
                let stroke = self.get_path_stroke();
                self.check_bbox(transformed_target.0, transformed_target.1, stroke / 2.0, stroke / 2.0);
            }
        }
    }

    fn add_arc_segment(&mut self, coord: &Coordinates, offset: &CoordinateOffset) -> () {
        let target = Self::coordinate_to_float(coord);
        let target = (
            target.0.unwrap_or(self.position.x),
            target.1.unwrap_or(self.position.y),
        );
        let transformed_target = self.apply_transformations(target.0, target.1);
        
        let offset_coords = Self::coordinate_offset_to_float(offset);
        let center_x = self.position.x + offset_coords.0.unwrap_or(0.0);
        let center_y = self.position.y + offset_coords.1.unwrap_or(0.0);
        
        let radius = ((center_x - self.position.x).powi(2) + (center_y - self.position.y).powi(2)).sqrt();
        
        log::debug!(
            "Draw arc from {:?} to {:?} with center ({}, {}) radius {}",
            self.position,
            transformed_target,
            center_x,
            center_y,
            radius
        );

        match &mut self.drawing_state {
            DrawingState::InRegion { path_data } => {
                if path_data.is_empty() {
                    *path_data = std::mem::take(path_data).move_to((self.position.x, self.position.y));
                }
                *path_data = std::mem::take(path_data).elliptical_arc_to((
                    transformed_target.0,
                    transformed_target.1,
                    radius,
                    radius,
                    0.0,
                ));
            }
            DrawingState::Normal => {
                let mut path = std::mem::take(&mut self.current_path_data);
                if path.is_empty() {
                    path = path.move_to((self.position.x, self.position.y));
                }
                self.current_path_data = path.elliptical_arc_to((
                    transformed_target.0,
                    transformed_target.1,
                    radius,
                    radius,
                    0.0,
                ));
                let stroke = self.get_path_stroke();
                self.check_bbox(transformed_target.0, transformed_target.1, stroke / 2.0, stroke / 2.0);
            }
        }
    }

    fn move_position(&mut self, coord: &Coordinates) -> () {
        let pos = Self::coordinate_to_float(coord);

        self.position.x = pos.0.unwrap_or(self.position.x);
        self.position.y = pos.1.unwrap_or(self.position.y);
    }

    fn create_path_from_data(&mut self) {
        if self.current_path_data.is_empty() {
            return;
        }

        let mut stroke = self.get_path_stroke(); // * (self.scale * 2.0);

        if self.scale > 1.0 {
            stroke *= 2.0;
        }
        else if self.scale < 1.0 {
            stroke /= 2.0;
        }

        let data = std::mem::replace(&mut self.current_path_data, path::Data::new());
        let svg = std::mem::replace(&mut self.svg_document, svg::Document::new());

        let path = Path::new()
            .set("fill", "none")
            .set("stroke", "white")
            .set("stroke-width", stroke)
            .set("d", data);

        self.svg_document = svg.add(path);
    }

    fn get_path_stroke(&self) -> f32 {
        return match self
            .selected_aperture
            .as_ref()
            .expect("No selected aperture for storke")
        {
            Aperture::Circle(c) => c.diameter as f32,
            _ => {
                log::warn!(
                    "Unsupported stroke aperture other than Circle.\r\n{:#?}",
                    self.selected_aperture
                );
                0_f32
            }
        };
    }

    fn coordinate_to_float(coord: &Coordinates) -> (Option<f32>, Option<f32>) {
        let mut result: (Option<f32>, Option<f32>) = (None, None);

        if coord.x.is_some() {
            result.0 = Some(
                coord
                    .x
                    .unwrap()
                    .gerber(&coord.format)
                    .unwrap()
                    .parse::<f32>()
                    .unwrap()
                    / 10_f32.powi(coord.format.decimal as i32),
            );
        }

        if coord.y.is_some() {
            result.1 = Some(
                coord
                    .y
                    .unwrap()
                    .gerber(&coord.format)
                    .unwrap()
                    .parse::<f32>()
                    .unwrap()
                    / 10_f32.powi(coord.format.decimal as i32),
            )
        }

        return result;
    }

    fn coordinate_offset_to_float(coord: &CoordinateOffset) -> (Option<f32>, Option<f32>) {
        let mut result: (Option<f32>, Option<f32>) = (None, None);

        if coord.x.is_some() {
            result.0 = Some(
                coord
                    .x
                    .unwrap()
                    .gerber(&coord.format)
                    .unwrap()
                    .parse::<f32>()
                    .unwrap()
                    / 10_f32.powi(coord.format.decimal as i32),
            );
        }

        if coord.y.is_some() {
            result.1 = Some(
                coord
                    .y
                    .unwrap()
                    .gerber(&coord.format)
                    .unwrap()
                    .parse::<f32>()
                    .unwrap()
                    / 10_f32.powi(coord.format.decimal as i32),
            )
        }

        return result;
    }

    fn check_bbox(&mut self, pos_x: f32, pos_y: f32, stroke_x: f32, stroke_y: f32){
        self.min_x = f32::min(pos_x - stroke_x, self.min_x);
        self.max_x = f32::max(pos_x + stroke_x, self.max_x);
        self.min_y = f32::min(pos_y - stroke_y, self.min_y);
        self.max_y = f32::max(pos_y + stroke_y, self.max_y);
    }

    fn set_bbox(&mut self, crop: bool){
        let mut doc = std::mem::replace(&mut self.svg_document, svg::Document::new());

        if crop{
            log::debug!("Crop enable");
            doc = doc.set("viewbox", (self.min_x, self.min_y, self.max_x - self.min_x, self.max_y - self.min_y));
        }
        else{
            log::debug!("Crop disable");
            doc = doc.set("viewbox", (0, 0, self.max_x, self.max_y));
        }

        self.svg_document = doc;
    }

    fn handle_region_start(&mut self) {
        log::debug!("Starting region (G36)");
        self.create_path_from_data();
        self.drawing_state = DrawingState::InRegion { 
            path_data: path::Data::new() 
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
        }
        self.drawing_state = DrawingState::Normal;
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
                match sr {
                    gerber_types::StepAndRepeat::Open { repeat_x, repeat_y, distance_x, distance_y } => {
                        self.step_repeat_active = true;
                        self.step_repeat_x = *repeat_x;
                        self.step_repeat_y = *repeat_y;
                        self.step_repeat_offset_x = *distance_x;
                        self.step_repeat_offset_y = *distance_y;
                        self.step_repeat_commands.clear();
                        log::debug!("Started step and repeat: {}x{} with offset ({}, {})", 
                                   repeat_x, repeat_y, distance_x, distance_y);
                    }
                    gerber_types::StepAndRepeat::Close => {
                        self.finalize_step_and_repeat();
                    }
                }
            }
            ExtendedCode::ApertureMacro(macro_def) => {
                let macro_name = macro_def.name.clone();
                log::debug!("Defined aperture macro: {}", macro_name);
            }
            ExtendedCode::DeleteAttribute(name) => {
                self.attributes.remove(name);
                log::debug!("Deleted attribute: {}", name);
            }
            _ => {
                log::debug!("Unsupported extended code: {:?}", extended_code);
            }
        }
    }

    fn set_polarity(&mut self, polarity: Polarity) {
        if self.polarity != polarity {
            self.create_path_from_data();
            self.polarity = polarity;
            log::debug!("Set polarity: {:?}", polarity);
        }
    }

    fn apply_transformations(&self, x: f32, y: f32) -> (f32, f32) {
        let mut tx = x * self.scaling as f32;
        let mut ty = y * self.scaling as f32;

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

        (tx, ty)
    }

    fn finalize_step_and_repeat(&mut self) {
        if !self.step_repeat_active {
            return;
        }

        log::debug!("Finalizing step and repeat with {} commands", self.step_repeat_commands.len());
        
        let mut doc = std::mem::replace(&mut self.svg_document, svg::Document::new());
        let mut group = Group::new();

        for repeat_x in 0..self.step_repeat_x {
            for repeat_y in 0..self.step_repeat_y {
                let offset_x = repeat_x as f64 * self.step_repeat_offset_x;
                let offset_y = repeat_y as f64 * self.step_repeat_offset_y;
                
                let mut repeat_group = Group::new()
                    .set("transform", format!("translate({}, {})", offset_x, offset_y));

                for command in &self.step_repeat_commands {
                    let element = self.command_to_svg_element(command, offset_x as f32, offset_y as f32);
                    if let Some(elem) = element {
                        repeat_group = repeat_group.add(elem);
                    }
                }
                
                group = group.add(repeat_group);
            }
        }

        doc = doc.add(group);
        self.svg_document = doc;
        
        self.step_repeat_active = false;
        self.step_repeat_commands.clear();
    }

    fn command_to_svg_element(&self, _command: &Command, _offset_x: f32, _offset_y: f32) -> Option<svg::node::element::Group> {
        None
    }

    fn parse_aperture_macro(&self, name: &str, _content: &str) -> ApertureMacro {
        let primitives = Vec::new();
        
        log::warn!("Aperture macro parsing not yet implemented for: {}", name);

        ApertureMacro {
            name: name.to_string(),
            primitives,
        }
    }

    fn instantiate_aperture_macro(&self, aperture_macro: &ApertureMacro, _params: &[f64], x: f32, y: f32) -> Group {
        let mut group = Group::new()
            .set("transform", format!("translate({}, {})", x, y));
        
        let fill_color = match self.polarity {
            Polarity::Dark => "white",
            Polarity::Clear => "black",
        };
        
        for primitive in &aperture_macro.primitives {
            match primitive {
                MacroPrimitive::Circle { exposure, diameter, center_x, center_y, rotation: _ } => {
                    let circle = Circle::new()
                        .set("cx", *center_x)
                        .set("cy", *center_y)
                        .set("r", diameter / 2.0)
                        .set("fill", if *exposure { fill_color } else { "none" });
                    
                    group = group.add(circle);
                }
                MacroPrimitive::VectorLine { exposure, width, start_x, start_y, end_x, end_y, rotation: _ } => {
                    if *exposure {
                        let path_data = path::Data::new()
                            .move_to((*start_x, *start_y))
                            .line_to((*end_x, *end_y));
                        
                        let path = Path::new()
                            .set("fill", "none")
                            .set("stroke", fill_color)
                            .set("stroke-width", *width)
                            .set("stroke-linecap", "round")
                            .set("d", path_data);
                        
                        group = group.add(path);
                    }
                }
                MacroPrimitive::CenterLine { exposure, width, height, center_x, center_y, rotation: _ } => {
                    if *exposure {
                        let rect = Rectangle::new()
                            .set("x", center_x - width / 2.0)
                            .set("y", center_y - height / 2.0)
                            .set("width", *width)
                            .set("height", *height)
                            .set("fill", fill_color);
                        
                        group = group.add(rect);
                    }
                }
                MacroPrimitive::Polygon { exposure, vertices, center_x, center_y, diameter, rotation } => {
                    if *exposure {
                        let radius = diameter / 2.0;
                        let rotation_offset = rotation.unwrap_or(0.0);
                        
                        let mut points = Vec::new();
                        for i in 0..*vertices {
                            let angle = (i as f64 * 2.0 * std::f64::consts::PI / *vertices as f64) + rotation_offset.to_radians();
                            let px = center_x + radius * angle.cos();
                            let py = center_y + radius * angle.sin();
                            points.push(format!("{},{}", px, py));
                        }
                        
                        let polygon = Polygon::new()
                            .set("points", points.join(" "))
                            .set("fill", fill_color);
                        
                        group = group.add(polygon);
                    }
                }
                MacroPrimitive::Thermal { center_x, center_y, outer_diameter, inner_diameter, gap, rotation: _ } => {
                    let outer_radius = outer_diameter / 2.0;
                    let inner_radius = inner_diameter / 2.0;
                    let gap_half = gap / 2.0;
                    
                    let outer_circle = Circle::new()
                        .set("cx", *center_x)
                        .set("cy", *center_y)
                        .set("r", outer_radius)
                        .set("fill", fill_color);
                    
                    let inner_circle = Circle::new()
                        .set("cx", *center_x)
                        .set("cy", *center_y)
                        .set("r", inner_radius)
                        .set("fill", "black");
                    
                    let gap_rect_h = Rectangle::new()
                        .set("x", center_x - outer_radius)
                        .set("y", center_y - gap_half)
                        .set("width", *outer_diameter)
                        .set("height", *gap)
                        .set("fill", "black");
                    
                    let gap_rect_v = Rectangle::new()
                        .set("x", center_x - gap_half)
                        .set("y", center_y - outer_radius)
                        .set("width", *gap)
                        .set("height", *outer_diameter)
                        .set("fill", "black");
                    
                    group = group.add(outer_circle);
                    group = group.add(inner_circle);
                    group = group.add(gap_rect_h);
                    group = group.add(gap_rect_v);
                }
                MacroPrimitive::Outline { exposure, points, rotation: _ } => {
                    if *exposure && !points.is_empty() {
                        let mut path_data = path::Data::new();
                        if let Some(first_point) = points.first() {
                            path_data = path_data.move_to((first_point.0, first_point.1));
                        }
                        for point in points.iter().skip(1) {
                            path_data = path_data.line_to((point.0, point.1));
                        }
                        path_data = path_data.close();
                        
                        let path = Path::new()
                            .set("fill", fill_color)
                            .set("d", path_data);
                        
                        group = group.add(path);
                    }
                }
                MacroPrimitive::Comment(_) => {
                }
            }
        }
        
        group
    }
}
