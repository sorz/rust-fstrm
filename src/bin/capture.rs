use clap::Parser;
use log::*;
use std::{
    error::Error,
    fmt::Debug,
    fs::{remove_file, File},
    io,
    net::{Incoming as TcpIncoming, TcpListener, TcpStream},
    os::unix::net::{Incoming as UnixIncoming, UnixListener, UnixStream},
    path::Path,
    sync::mpsc,
    thread,
};

use fstrm::control::ContentType;
use fstrm::reader::{bi_directional::Build, Builder, RefContentType};
use fstrm::*;

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// increment debugging level
    #[arg(short, long, default_value_t = false)]
    #[cfg(feature = "logger")]
    debug: bool,

    /// Frame Streams content type
    #[arg(short, long)]
    r#type: String,

    /// Unix socket path to read from
    #[arg(
        value_name = "FILENAME",
        short,
        long,
        required = false,
        required_unless_present = "tcp"
    )]
    unix: Option<String>,

    /// TCP socket address to read from
    #[arg(
        value_name = "ADDRESS",
        short = 'a',
        long,
        required = false,
        requires = "port"
    )]
    tcp: Option<String>,

    /// TCP socket port to read from
    #[arg(short, long, required = false)]
    port: Option<u16>,

    /// read buffer size, in bytes
    // #[arg(value_name = "SIZE", short, long, default_value_t = 262144)]
    // buffersize: u32,

    // /// maximum concurrent connections allowed
    // #[arg(value_name = "COUNT", short = 'c', long, required = false)]
    // maxconns: Option<u32>,

    /// file path to write Frame Streams data to
    #[arg(value_name = "FILENAME", short, long)]
    write: String,
    // /// seconds before rotating output file
    // #[arg(value_name = "SECONDS", short, long, required = false)]
    // split: Option<u32>,

    // /// filter -w path with strftime (local time)
    // #[arg(long, required = false)]
    // localtime: Option<String>,

    // /// filter -w path with strftime (UTC)
    // #[arg(long, required = false)]
    // gmtime: Option<String>,
}

enum EnumStream {
    Unix(UnixStream),
    Tcp(TcpStream),
}

impl std::io::Read for EnumStream {
    #[inline]
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Self::Unix(unix) => unix.read(buf),
            Self::Tcp(tcp) => tcp.read(buf),
        }
    }
}

impl std::io::Write for EnumStream {
    #[inline]
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            Self::Unix(unix) => unix.write(buf),
            Self::Tcp(tcp) => tcp.write(buf),
        }
    }

    #[inline]
    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Self::Unix(unix) => unix.flush(),
            Self::Tcp(unix) => unix.flush(),
        }
    }
}

impl From<UnixStream> for EnumStream {
    #[inline]
    fn from(unix: UnixStream) -> EnumStream {
        EnumStream::Unix(unix)
    }
}

impl From<TcpStream> for EnumStream {
    #[inline]
    fn from(tcp: TcpStream) -> EnumStream {
        EnumStream::Tcp(tcp)
    }
}

enum EnumIncoming<'a> {
    Unix(UnixIncoming<'a>),
    Tcp(TcpIncoming<'a>),
}

#[inline]
fn into_result<T, U: Into<T>, E: std::error::Error>(
    res: std::result::Result<U, E>,
) -> std::result::Result<T, E> {
    Ok(res?.into())
}

impl<'a> Iterator for EnumIncoming<'a> {
    type Item = io::Result<EnumStream>;

    #[inline]
    fn next(&mut self) -> Option<io::Result<EnumStream>> {
        Some(match self {
            Self::Tcp(tcp) => into_result(tcp.next()?),
            Self::Unix(unix) => into_result(unix.next()?),
        })
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (usize::MAX, None)
    }
}

enum EnumListener {
    Unix(UnixListener),
    Tcp(TcpListener),
}

impl EnumListener {
    #[inline]
    fn incoming(&self) -> EnumIncoming<'_> {
        match self {
            Self::Tcp(tcp) => EnumIncoming::Tcp(tcp.incoming()),
            Self::Unix(unix) => EnumIncoming::Unix(unix.incoming()),
        }
    }
}

enum EnumWriter {
    File(std::fs::File),
    Stdout(std::io::Stdout),
}

impl std::io::Write for EnumWriter {
    #[inline]
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            Self::File(file) => file.write(buf),
            Self::Stdout(stdout) => stdout.write(buf),
        }
    }

    #[inline]
    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Self::File(file) => file.flush(),
            Self::Stdout(stdout) => stdout.flush(),
        }
    }
}

impl Write for EnumWriter {}

fn main() -> std::result::Result<(), Box<dyn Error>> {
    let args: Args = Args::parse();

    #[cfg(feature = "logger")]
    env_logger::Builder::new()
        .filter_level(if args.debug {
            log::LevelFilter::Debug
        } else {
            log::LevelFilter::Info
        })
        .format_timestamp(None)
        .parse_default_env()
        .init();

    let content_types: Vec<ContentType> = vec![args.r#type.into()];

    let (tx, rx) = mpsc::channel::<fstrm::Frame>();
    let mut handles = vec![];

    let handle = thread::spawn(move || {
        let mut writer = if &args.write == "-" {
            EnumWriter::Stdout(std::io::stdout())
        } else {
            let path = Path::new(&args.write);

            debug!("opened output file {:?}", path);
            EnumWriter::File(File::options().write(true).create(true).open(path).unwrap())
        };

        for frame in rx {
            if frame.encode(&mut writer).is_err() {
                break;
            }
        }
    });

    handles.push(handle);

    for r in if let Some(unix) = args.unix {
        let unix_path = Path::new(&unix);

        match remove_file(unix_path) {
            Err(err) if err.kind() != io::ErrorKind::NotFound => Err(err),
            _ => Ok(()),
        }?;

        debug!("opening Unix socket path {:?}", unix_path);
        EnumListener::Unix(UnixListener::bind(unix_path)?)
    } else {
        let tcp = args.tcp.unwrap();
        let port = args.port.unwrap();

        debug!("opening TCP socket [{}]:{}", tcp, port);
        EnumListener::Tcp(TcpListener::bind(format!("{}:{}", tcp, port))?)
    }
    .incoming()
    {
        let stream = r?;
        let mut builder = Builder::new(stream, &content_types);
        let reader = builder.build()?;

        tx.send(Frame::Control(Control::Start(
            match reader.content_type_ref() {
                Some(content_type) => {
                    ControlData::Field(ControlField::ContentType(content_type.clone()))
                }
                None => ControlData::Empty,
            },
        )))?;

        for r in reader {
            let data = r?;

            tx.send(Frame::Data(data))?;
        }

        tx.send(Frame::Control(Control::Stop))?;
    }

    drop(tx);

    for handle in handles {
        handle.join().unwrap();
    }

    Ok(())
}
