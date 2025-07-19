use std::fs;
use std::io::Write;
use std::process::Command;
use tempfile::NamedTempFile;

fn create_test_gerber_file() -> NamedTempFile {
    let mut file = NamedTempFile::new().unwrap();
    writeln!(file, "G04 Test Gerber File*").unwrap();
    writeln!(file, "%FSLAX23Y23*%").unwrap();
    writeln!(file, "%MOMM*%").unwrap();
    writeln!(file, "%ADD10C,0.1*%").unwrap();
    writeln!(file, "D10*").unwrap();
    writeln!(file, "X0Y0D02*").unwrap();
    writeln!(file, "X1000Y1000D01*").unwrap();
    writeln!(file, "M02*").unwrap();
    file.flush().unwrap();
    file
}

#[test]
fn test_cli_help() {
    let output = Command::new("cargo")
        .args(&["run", "--", "--help"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("gerber2svg"));
}

#[test]
fn test_cli_conversion() {
    let input_file = create_test_gerber_file();
    let output_file = NamedTempFile::new().unwrap();

    let output = Command::new("cargo")
        .args(&[
            "run",
            "--",
            "-i",
            input_file.path().to_str().unwrap(),
            "-o",
            output_file.path().to_str().unwrap(),
        ])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());

    let content = fs::read_to_string(output_file.path()).unwrap();
    assert!(content.contains("<svg"));
}
