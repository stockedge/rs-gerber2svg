# Gerber2SVG

[![Crates.io](https://img.shields.io/crates/v/gerber2svg.svg)](https://crates.io/crates/gerber2svg)
[![Documentation](https://docs.rs/gerber2svg/badge.svg)](https://docs.rs/gerber2svg)
[![License](https://img.shields.io/crates/l/gerber2svg.svg)](https://github.com/stockedge/rs-gerber2svg#license)

## Introduction
Gerber2SVG is a Rust library and command-line utility for converting Gerber files (RS-274X format) into SVG files. It supports standard Gerber features including apertures, draw commands, regions, and polarity control.

The generated SVG files contain individual geometric elements (paths, rectangles, circles) rather than a single unified path, making them suitable for further processing and analysis.

## Features

### ✅ Supported Features
- **Standard Apertures**: Circle, Rectangle, Obround, Polygon apertures
- **Draw Commands**: Linear and arc interpolation with D01/D02/D03 codes
- **Region Statements**: G36/G37 filled regions with proper polarity handling
- **Polarity Control**: LPD (Dark) and LPC (Clear) polarity switching
- **Aperture Macros (AM)**: Circle, VectorLine, CenterLine, Outline, and Polygon primitives
- **Step and Repeat (SR)**: Complete panelization functionality
- **Block Apertures (AB)**: Complex shape definition and reuse
- **Coordinate Systems**: Support for various coordinate formats and units
- **Scaling**: Configurable scaling of output SVG
- **File I/O**: Read Gerber files and save/output SVG files

### ⚠️ Current Limitations
- **Macro Expressions**: Arithmetic expressions in aperture macros (e.g., "$1+$2") are not yet evaluated
- **Advanced Macro Primitives**: Moire and Thermal primitives have basic support
- **Aperture Transformations**: LM/LR/LS commands are parsed but transformation application is limited

## Installation

### As a Library
Add to your `Cargo.toml`:
```toml
[dependencies]
gerber2svg = "0.2"
```

### As a Command-Line Tool
```bash
cargo install gerber2svg
```

## Usage

### Command Line
```bash
# Convert Gerber file to SVG
gerber2svg -i input.gbr -o output.svg

# Scale the output by 2x
gerber2svg -i input.gbr -o output.svg --scale 2.0

# Print SVG to stdout
gerber2svg -i input.gbr

# Enable verbose logging
gerber2svg -i input.gbr -o output.svg --verbose

# Show help
gerber2svg --help
```

### Library Usage
```rust
use gerber2svg::Gerber2SVG;

// Convert from file
let gerber = Gerber2SVG::from_file("input.gbr")?
    .set_scale(2.0)
    .build();

// Save to file
gerber.save_svg("output.svg")?;

// Get SVG as string
let svg_content = gerber.to_string();
```

## Examples

### Basic Conversion
```rust
use gerber2svg::Gerber2SVG;

fn main() -> Result<(), std::io::Error> {
    let gerber = Gerber2SVG::from_file("example.gbr")?
        .set_scale(1.0)
        .build();
    
    gerber.save_svg("example.svg")?;
    println!("Conversion complete!");
    Ok(())
}
```

## Contributing

Contributions are welcome! Please feel free to submit issues and pull requests.

### Development
```bash
# Clone the repository
git clone https://github.com/stockedge/rs-gerber2svg.git
cd rs-gerber2svg

# Run tests
cargo test

# Run with example
cargo run -- -i examples/example.gbr -o output.svg
```

## License

This project is licensed under either of
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT License ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.
