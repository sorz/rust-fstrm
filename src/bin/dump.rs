use clap::Parser;

use fstrm::control::ContentType;
use fstrm::reader::{uni_directional::Build, Builder, RefContentType};
use fstrm::*;

#[derive(Parser, Debug)]
#[command(author, version, about="Dumps a Frame Streams formatted input file.", long_about = None)]
struct Args {
    /// input file
    #[arg(value_name = "INPUT FILE")]
    input: String,

    /// output file
    #[arg(value_name = "OUTPUT FILE", required = false)]
    output: Option<String>,
}

#[inline]
fn isprint(c: u8) -> bool {
    c >> 7 == 0 && c >> 5 != 0 && c != 0x7f
}

fn to_string(bytes: &Vec<u8>) -> String {
    let mut s = String::new();

    for c in bytes.iter() {
        if !isprint(*c) {
            s.push_str(&format!("\\x{:02x}", *c));
        } else if *c == b'\"' {
            s.push_str("\\\"");
        } else {
            s.push((*c).into())
        }
    }

    s
}

enum EnumReader {
    Stdin(std::io::Stdin),
    File(std::fs::File),
}

impl std::io::Read for EnumReader {
    #[inline]
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Self::Stdin(stdin) => stdin.read(buf),
            Self::File(file) => file.read(buf),
        }
    }
}

enum EnumWriter {
    File(std::fs::File),
    None,
}

impl std::io::Write for EnumWriter {
    #[inline]
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            Self::File(file) => file.write(buf),
            Self::None => Ok(0),
        }
    }

    #[inline]
    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Self::File(file) => file.flush(),
            Self::None => Ok(()),
        }
    }
}

impl Write for EnumWriter {}

impl EnumWriter {
    #[inline]
    pub const fn is_some(&self) -> bool {
        !self.is_none()
    }

    #[inline]
    pub const fn is_none(&self) -> bool {
        matches!(*self, Self::None)
    }
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let args: Args = Args::parse();

    #[cfg(feature = "logger")]
    env_logger::Builder::new()
        .filter_level(log::LevelFilter::Error)
        .format_timestamp(None)
        .parse_default_env()
        .init();

    let content_types = Vec::<ContentType>::new();

    let mut builder = Builder::new(
        if args.input == "-" {
            EnumReader::Stdin(std::io::stdin())
        } else {
            EnumReader::File(std::fs::File::open(args.input)?)
        },
        &content_types,
    );

    let mut writer = if let Some(output) = args.output {
        EnumWriter::File(std::fs::File::create(output)?)
    } else {
        EnumWriter::None
    };

    let reader = builder.build()?;

    println!("FSTRM_CONTROL_START.");

    if let Some(content_type) = reader.content_type_ref() {
        println!(
            "FSTRM_CONTROL_FIELD_CONTENT_TYPE ({} bytes).",
            content_type.len()
        );
        println!(" \"{}\"", std::str::from_utf8(content_type.ref_body())?);

        if writer.is_some() {
            Frame::Control(Control::Start(ControlData::Field(
                ControlField::ContentType(content_type.clone()),
            )))
            .encode(&mut writer)?;
        }
    }

    for r in reader {
        let data = r?;

        println!("Data frame ({}) bytes.", data.len());
        println!(" \"{}\"", to_string(&data));

        if writer.is_some() {
            Frame::Data(data).encode(&mut writer)?;
        }
    }

    println!("FSTRM_CONTROL_STOP.");
    if writer.is_some() {
        Frame::Control(Control::Stop).encode(&mut writer)?;
    }
    Ok(())
}
