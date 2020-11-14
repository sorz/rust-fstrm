use byteorder::{BigEndian, ReadBytesExt};
use log::{info, warn};
use std::{
    cmp::min,
    collections::HashSet,
    convert::TryInto,
    io::{self, ErrorKind, Read, Result, Write},
    iter::FromIterator,
    marker::PhantomData,
    str,
};

const MAX_CONTROL_FRAME_SIZE: usize = 1024 * 1024;

const CONTROL_TYPE_ACCEPT: u32 = 0x01;
const CONTROL_TYPE_START: u32 = 0x02;
const CONTROL_TYPE_STOP: u32 = 0x03;
const CONTROL_TYPE_READY: u32 = 0x04;
const CONTROL_TYPE_FINISH: u32 = 0x05;
const CONTROL_FIELD_CONTENT_TYPE: u32 = 0x01;

pub mod states {
    pub struct Ready;
    pub struct Accepted;
    pub struct Started;

    pub trait BeforeStart {}
    impl BeforeStart for Ready {}
    impl BeforeStart for Accepted {}
}

pub struct FstrmReader<R, S> {
    reader: R,
    state: PhantomData<S>,
    content_types: HashSet<String>,
}

impl<R, S> FstrmReader<R, S> {
    /// Create a new reader that accpets all content types.
    pub fn new(reader: R) -> FstrmReader<R, states::Ready> {
        FstrmReader {
            reader,
            state: PhantomData,
            content_types: HashSet::new(),
        }
    }

    /// Create a new reader that accepts only given set of content types.
    pub fn allow_content_types<T>(
        reader: R,
        allowed_content_types: T,
    ) -> FstrmReader<R, states::Ready>
    where
        T: IntoIterator<Item = String>,
    {
        FstrmReader {
            reader,
            state: PhantomData,
            content_types: HashSet::from_iter(allowed_content_types),
        }
    }

    pub fn into_inner(self) -> R {
        self.reader
    }
}

impl<R, S: states::BeforeStart> FstrmReader<R, S> {
    fn allowed_content_types(&self) -> HashSet<&str> {
        self.content_types.iter().map(|s| s.as_str()).collect()
    }
}

impl<R: Read, S: states::BeforeStart> FstrmReader<R, S> {
    /// Read the START frame.
    pub fn start(mut self) -> Result<FstrmReader<R, states::Started>> {
        let frame = self.read_control_frame_of(ControlType::Start)?;
        let allowed_types = self.allowed_content_types();

        let mut buf = &frame[..];
        let mut types: HashSet<&str> = HashSet::with_capacity(allowed_types.len());
        while !buf.is_empty() {
            let field_type = buf.read_u32::<BigEndian>()?;
            match field_type {
                CONTROL_FIELD_CONTENT_TYPE => {
                    let size = buf.read_u32::<BigEndian>()? as usize;
                    if size > buf.len() {
                        warn!("paring error: control field too long");
                        return Err(ErrorKind::UnexpectedEof.into());
                    }
                    let (typ, remaining) = buf.split_at(size);
                    types.insert(str::from_utf8(typ).map_err(|_| {
                        io::Error::new(ErrorKind::InvalidData, "content type with invalid utf-8")
                    })?);
                    buf = remaining;
                }
                _ => info!("unknown control field: {}", field_type),
            }
        }

        let content_types = if allowed_types.is_empty() {
            // Allow any types
            HashSet::from_iter(types.into_iter().map(|s| s.to_string()))
        } else {
            HashSet::from_iter(allowed_types.intersection(&types).map(|s| s.to_string()))
        };

        if content_types.is_empty() {
            Err(io::Error::new(
                ErrorKind::InvalidData,
                "content types mismatched",
            ))
        } else {
            Ok(FstrmReader {
                reader: self.reader,
                state: PhantomData,
                content_types,
            })
        }
    }
}

impl<R: Read + Write> FstrmReader<R, states::Ready> {
    /// Read the READY frame then reply with ACCEPT.
    pub fn accept<'a, T, I: 'a>(&mut self) -> Result<FstrmReader<R, states::Accepted>>
    where
        T: IntoIterator<Item = &'a I>,
        I: AsRef<str>,
    {
        let frame = self.read_control_frame_of(ControlType::Ready)?;
        let allowed_types = self.allowed_content_types();

        // TODO: parse content types

        // TODO: write ACCEPT frame
        unimplemented!()
    }
}

impl<R: Read + Write> FstrmReader<R, states::Accepted> {
    /// Write FINISH frame, return the inner reader.
    pub fn finish(&self) -> Result<R> {
        unimplemented!()
    }
}

#[derive(Debug, PartialEq)]
enum ControlType {
    Accept,
    Start,
    Stop,
    Ready,
    Finish,
    Unknown(u32),
}

enum FrameHeader {
    Data { size: usize },
    Control { size: usize, typ: ControlType },
}

impl From<u32> for ControlType {
    fn from(value: u32) -> Self {
        match value {
            CONTROL_TYPE_ACCEPT => ControlType::Accept,
            CONTROL_TYPE_START => ControlType::Start,
            CONTROL_TYPE_STOP => ControlType::Stop,
            CONTROL_TYPE_READY => ControlType::Ready,
            CONTROL_TYPE_FINISH => ControlType::Finish,
            _ => ControlType::Unknown(value),
        }
    }
}

impl<R: Read, S> FstrmReader<R, S> {
    fn next_length(&mut self) -> Result<usize> {
        Ok(self.reader.read_u32::<BigEndian>()?.try_into().unwrap())
    }

    fn read_frame_header(&mut self) -> Result<FrameHeader> {
        let size = self.next_length()?;
        if size > 0 {
            Ok(FrameHeader::Data { size })
        } else {
            let size = self.next_length()?;
            let typ = self.reader.read_u32::<BigEndian>()?.into();
            if size > MAX_CONTROL_FRAME_SIZE {
                Err(io::Error::new(ErrorKind::Other, "control frame too large"))
            } else {
                Ok(FrameHeader::Control { size, typ })
            }
        }
    }

    fn read_control_frame_of(&mut self, expect_type: ControlType) -> Result<Box<[u8]>> {
        let size = match self.read_frame_header()? {
            FrameHeader::Data { .. } => {
                return Err(io::Error::new(
                    ErrorKind::InvalidData,
                    "unexpected data frame",
                ))
            }
            FrameHeader::Control { typ, size } if typ == expect_type => size,
            _ => {
                return Err(io::Error::new(
                    ErrorKind::InvalidData,
                    "unexpected type of control frame",
                ))
            }
        };
        let mut frame = vec![0u8; size];
        self.reader.read_exact(&mut frame)?;
        Ok(frame.into_boxed_slice())
    }
}

impl<R> FstrmReader<R, states::Started> {
    /// Negotiated content types
    pub fn content_types(&self) -> &HashSet<String> {
        &self.content_types
    }
}

impl<R: Read> FstrmReader<R, states::Started> {
    /// Read the next data frame, return None if the other side
    /// stop sending with a control frame.
    pub fn read_frame(&mut self) -> Result<Option<Frame<R>>> {
        match self.read_frame_header()? {
            FrameHeader::Data { size } => Ok(Some(Frame::new(&mut self.reader, size))),
            FrameHeader::Control { size, typ } => {
                // TODO: handle control frame
                Ok(None)
            }
        }
    }
}

pub struct Frame<'a, R> {
    reader: &'a mut R,
    size: usize,
    pos: usize,
}

impl<'a, R> Frame<'a, R> {
    fn new(reader: &'a mut R, size: usize) -> Self {
        Self {
            reader,
            size: size.try_into().unwrap(),
            pos: 0,
        }
    }

    pub fn size(&self) -> usize {
        self.size
    }

    pub fn remaining(&self) -> usize {
        self.size - self.pos
    }
}

impl<'a, R: Read> Read for Frame<'a, R> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        let max_len = min(buf.len(), self.remaining());
        let n = self.reader.read(&mut buf[..max_len])?;
        self.pos += n;
        if n == 0 && self.remaining() != 0 {
            Err(ErrorKind::UnexpectedEof.into())
        } else {
            Ok(n)
        }
    }
}
