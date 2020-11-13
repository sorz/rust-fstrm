use byteorder::{BigEndian, ReadBytesExt};
use std::{
    cmp::min,
    collections::HashSet,
    convert::TryInto,
    io::{ErrorKind, Read, Result, Write},
    iter::FromIterator,
    marker::PhantomData,
};

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
    pub fn start(&mut self) -> Result<FstrmReader<R, states::Started>> {
        unimplemented!()
    }
}

impl<R: Read + Write> FstrmReader<R, states::Ready> {
    /// Read the READY frame then reply with ACCEPT, if content types are matched.
    /// Set allowed_content_types as empty to allow any content type.
    pub fn accept<'a, T, S: 'a>(
        &mut self,
        allowed_content_types: T,
    ) -> Result<FstrmReader<R, states::Accepted>>
    where
        T: IntoIterator<Item = &'a S>,
        S: AsRef<str>,
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

impl<R: Read> FstrmReader<R, states::Started> {
    // Read the next data frame, return None if the other side
    // stop sending with a control frame.
    pub fn read_frame(&mut self) -> Result<Option<Frame<R>>> {
        let len = self.reader.read_u32::<BigEndian>()?;
        if len == 0 {
            // TODO: handle control frame
            Ok(None)
        } else {
            Ok(Some(Frame::new(&mut self.reader, len)))
        }
    }
}

pub struct Frame<'a, R> {
    reader: &'a mut R,
    size: usize,
    pos: usize,
}

impl<'a, R> Frame<'a, R> {
    fn new(reader: &'a mut R, size: u32) -> Self {
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
