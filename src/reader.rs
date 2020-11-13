use byteorder::{BigEndian, ReadBytesExt};
use log::{info, warn};
use std::{
    cmp::min,
    collections::HashSet,
    convert::TryInto,
    io::{self, ErrorKind, Read, Result, Write},
    iter::FromIterator,
    marker::PhantomData,
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
}

impl<R, S> FstrmReader<R, S> {
    pub fn new(reader: R) -> FstrmReader<R, states::Ready> {
        FstrmReader {
            reader,
            state: PhantomData,
        }
    }

    pub fn into_inner(self) -> R {
        self.reader
    }
}

impl<R: Read, S: states::BeforeStart> FstrmReader<R, S> {
    /// Read the START frame.
    pub fn start<'a, T, I: 'a>(
        mut self,
        content_types: T,
    ) -> Result<FstrmReader<R, states::Started>>
    where
        T: IntoIterator<Item = &'a I>,
        I: AsRef<str>,
    {
        let content_types: HashSet<&[u8]> =
            HashSet::from_iter(content_types.into_iter().map(|s| s.as_ref().as_bytes()));

        let size = match self.read_frame_header()? {
            FrameHeader::Control {
                typ: ControlType::Start,
                size,
            } => size,
            _ => return Err(io::Error::new(ErrorKind::InvalidData, "not a START frame")),
        };
        let mut frame = vec![0u8; size];
        self.reader.read_exact(&mut frame)?;

        let mut buf = &frame[..];
        let mut types: HashSet<&[u8]> = HashSet::with_capacity(content_types.len());
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
                    types.insert(typ);
                    buf = remaining;
                }
                _ => info!("unknown control field: {}", field_type),
            }
        }

        if !content_types.is_empty() && content_types != types {
            Err(io::Error::new(
                ErrorKind::InvalidData,
                "content types mismatched",
            ))
        } else {
            Ok(FstrmReader {
                reader: self.reader,
                state: PhantomData,
            })
        }
    }
}

impl<R: Read + Write> FstrmReader<R, states::Ready> {
    /// Read the READY frame then reply with ACCEPT, if content types are matched.
    /// Set allowed_content_types as empty to allow any content type.
    pub fn accept<'a, T, I: 'a>(
        &mut self,
        allowed_content_types: T,
    ) -> Result<FstrmReader<R, states::Accepted>>
    where
        T: IntoIterator<Item = &'a I>,
        I: AsRef<str>,
    {
        let allowed_types: HashSet<&str> =
            HashSet::from_iter(allowed_content_types.into_iter().map(|s| s.as_ref()));

        unimplemented!()
    }
}

impl<R: Read + Write> FstrmReader<R, states::Accepted> {
    /// Write FINISH frame, return the inner reader.
    pub fn finish(&self) -> Result<R> {
        unimplemented!()
    }
}

#[repr(u32)]
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
}

impl<R: Read> FstrmReader<R, states::Started> {
    // Read the next data frame, return None if the other side
    // stop sending with a control frame.
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
