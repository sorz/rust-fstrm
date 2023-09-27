use byteorder::{BigEndian, ReadBytesExt};
use std::io;
use std::io::BufRead;
use std::io::BufReader;

use crate::*;
use control::ContentType;

pub trait Inner<R: io::Read> {
    fn inner_ref(&self) -> &BufReader<R>;
    fn inner_mut(&mut self) -> &mut BufReader<R>;
}

pub trait RefContentType {
    fn content_type_ref(&self) -> Option<&ContentType>;
}

pub trait RefContentTypes {
    fn content_types_ref(&self) -> &Vec<ContentType>;
}

pub trait Read: io::Read {
    #[inline]
    fn read_frame_value(&mut self) -> io::Result<u32> {
        self.read_u32::<BigEndian>()
    }

    #[inline]
    fn read_frame_header(&mut self) -> io::Result<FrameHeader> {
        let value: FstrmLength = self.read_frame_value()?;

        log::trace!("read frame header {:#010x}", value);

        Ok(if value == 0 {
            FrameHeader::ControlHeader
        } else {
            FrameHeader::Length(value)
        })
    }

    #[inline]
    fn read_frame_len(&mut self) -> Result<FstrmLength> {
        if let FrameHeader::Length(l) = self.read_frame_header()? {
            log::trace!("read frame len {}", l);

            Ok(l)
        } else {
            Err(Error::FrameFormatError)
        }
    }

    #[inline]
    fn read_frame_data_for_len(&mut self, len: FstrmLength) -> io::Result<Vec<u8>> {
        let len = len.try_into().unwrap();
        let mut data = Vec::<u8>::with_capacity(len);

        log::trace!("read {} bytes of frame data", len);
        unsafe {
            data.set_len(len);
            self.read_exact(&mut data)?;
        }

        Ok(data)
    }

    #[inline]
    fn read_frame_data(&mut self) -> Result<Vec<u8>> {
        let len = self.read_frame_len()?;

        Ok(self.read_frame_data_for_len(len)?)
    }

    fn consume(&mut self, len: FstrmLength);

    #[inline]
    fn read_frame_control(&mut self) -> Result<Control> {
        Control::decode(self)
    }

    #[inline]
    fn read_frame(&mut self) -> Result<Frame> {
        Ok(if let FrameHeader::Length(l) = self.read_frame_header()? {
            Frame::Data(self.read_frame_data_for_len(l)?)
        } else {
            Frame::Control(self.read_frame_control()?)
        })
    }
}

pub struct Builder<'a, R: io::Read> {
    buf_reader: BufReader<R>,
    content_types: &'a Vec<ContentType>,
}

impl<R: io::Read> Builder<'_, R> {
    #[inline]
    pub fn into_inner(self) -> BufReader<R> {
        self.buf_reader
    }
}

impl<'a, R: io::Read> RefContentTypes for Builder<'a, R> {
    #[inline]
    fn content_types_ref(&self) -> &'a Vec<ContentType> {
        &self.content_types
    }
}

impl<'a, R: io::Read> Builder<'a, R> {
    pub fn new(inner: R, content_types: &'a Vec<ContentType>) -> Self {
        Self {
            buf_reader: BufReader::new(inner),
            content_types,
        }
    }
}

impl<R: io::Read> Inner<R> for Builder<'_, R> {
    #[inline]
    fn inner_ref(&self) -> &BufReader<R> {
        &self.buf_reader
    }

    #[inline]
    fn inner_mut(&mut self) -> &mut BufReader<R> {
        &mut self.buf_reader
    }
}

impl<R: io::Read> io::Read for Builder<'_, R> {
    #[inline]
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner_mut().read(buf)
    }
}

impl<R: io::Read> Read for Builder<'_, R> {
    #[inline]
    fn consume(&mut self, len: FstrmLength) {
        self.inner_mut().consume(len.try_into().unwrap());
    }
}

// buf_reader: BufReader<R>,

pub struct BaseReader<'a, R: io::Read> {
    buf_reader: &'a mut BufReader<R>,
    content_type: Option<reader::ContentType>,
}

impl<'a, R: io::Read> BaseReader<'a, R> {
    pub fn new<B: RefContentTypes + Inner<R> + ?Sized>(
        start: ControlData,
        builder: &'a mut B,
    ) -> Option<Self> {
        match start {
            ControlData::Empty => Some(Self {
                buf_reader: builder.inner_mut(),
                content_type: None,
            }),
            ControlData::Field(ControlField::ContentType(content_type))
                if builder.content_types_ref().len() == 0
                    || builder.content_types_ref().contains(&content_type) =>
            {
                Some(Self {
                    buf_reader: builder.inner_mut(),
                    content_type: Some(content_type),
                })
            }
            _ => None,
        }
    }
}

impl<R: io::Read> Inner<R> for BaseReader<'_, R> {
    #[inline]
    fn inner_ref(&self) -> &BufReader<R> {
        self.buf_reader
    }

    #[inline]
    fn inner_mut(&mut self) -> &mut BufReader<R> {
        self.buf_reader
    }
}

impl<R: io::Read> io::Read for BaseReader<'_, R> {
    #[inline]
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner_mut().read(buf)
    }
}

impl<R: io::Read> Read for BaseReader<'_, R> {
    #[inline]
    fn consume(&mut self, len: FstrmLength) {
        self.inner_mut().consume(len.try_into().unwrap());
    }
}

impl<R: io::Read> RefContentType for BaseReader<'_, R> {
    fn content_type_ref(&self) -> Option<&ContentType> {
        let Some(c) = &self.content_type else {
            return None;
        };
        Some(&c)
    }
}

impl<R: io::Read> std::iter::Iterator for BaseReader<'_, R> {
    type Item = Result<reader::Payload>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.read_frame() {
            Ok(Frame::Data(payload)) => Some(Ok(payload)),
            Ok(Frame::Control(Control::Stop)) => None,
            Ok(_) => Some(Err(Error::TodoError)),
            Err(err) => Some(Err(err)),
        }
    }
}

pub mod uni_directional {
    use crate::reader::*;
    use std::io;

    pub type Reader<'a, R> = BaseReader<'a, R>;

    pub trait Build<R: io::Read>: Read + RefContentTypes + Inner<R> {
        fn build<'a>(&'a mut self) -> Result<Reader<'a, R>> {
            if let Frame::Control(Control::Start(start)) = self.read_frame()? {
                Reader::new(start, self).ok_or(Error::TodoError)
            } else {
                Err(Error::TodoError)
            }
        }
    }

    impl<'a, R: io::Read> Build<R> for Builder<'a, R> {}
}

pub mod bi_directional {
    use crate::reader::*;
    use crate::writer::*;

    use std::io;
    use std::iter::Iterator;

    impl<R: io::Read + io::Write> io::Write for Builder<'_, R> {
        #[inline]
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.inner_mut().get_mut().write(buf)
        }

        #[inline]
        fn flush(&mut self) -> std::io::Result<()> {
            self.inner_mut().get_mut().flush()
        }
    }

    impl<R: io::Read + io::Write> Write for Builder<'_, R> {}

    impl<R: io::Read + io::Write> io::Write for BaseReader<'_, R> {
        #[inline]
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.inner_mut().get_mut().write(buf)
        }

        #[inline]
        fn flush(&mut self) -> std::io::Result<()> {
            self.inner_mut().get_mut().flush()
        }
    }

    impl<R: io::Read + io::Write> Write for BaseReader<'_, R> {}

    pub enum Reader<'a, R: io::Read + io::Write> {
        UniDirectional(BaseReader<'a, R>),
        BiDirectional(BaseReader<'a, R>),
    }

    impl<'a, R: io::Read + io::Write> Reader<'a, R> {
        #[inline]
        fn base_ref(&self) -> &BaseReader<'a, R> {
            match self {
                Self::UniDirectional(base) | Self::BiDirectional(base) => base,
            }
        }
        #[inline]
        fn read_mut(&mut self) -> &mut BaseReader<'a, R> {
            match self {
                Self::UniDirectional(base) | Self::BiDirectional(base) => base,
            }
        }

        #[inline]
        fn write_mut(&mut self) -> &mut BaseReader<'a, R> {
            match self {
                Self::UniDirectional(_) => panic!("UniDirectional readers do not allow writing"),
                Self::BiDirectional(base) => base,
            }
        }
    }

    impl<R: io::Read + io::Write> RefContentType for Reader<'_, R> {
        fn content_type_ref(&self) -> Option<&ContentType> {
            self.base_ref().content_type_ref()
        }
    }

    impl<R: io::Read + io::Write> io::Read for Reader<'_, R> {
        #[inline]
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            self.read_mut().read(buf)
        }
    }

    impl<R: io::Read + io::Write> Read for Reader<'_, R> {
        #[inline]
        fn consume(&mut self, len: FstrmLength) {
            self.read_mut().consume(len)
        }
    }

    impl<R: io::Read + io::Write> io::Write for Reader<'_, R> {
        #[inline]
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.write_mut().write(buf)
        }

        #[inline]
        fn flush(&mut self) -> std::io::Result<()> {
            self.write_mut().flush()
        }
    }

    pub trait Build<R: io::Read + io::Write>: Read + Write + RefContentTypes + Inner<R> {
        fn build(&mut self) -> Result<Reader<'_, R>> {
            match self.read_frame()? {
                Frame::Control(Control::Start(start)) => {
                    if let Some(base) = BaseReader::new(start, self) {
                        Ok(Reader::UniDirectional(base))
                    } else {
                        Err(Error::TodoError)
                    }
                }
                Frame::Control(Control::Ready(data)) => {
                    Frame::Control(Control::Accept({
                        let content_types = self.content_types_ref();

                        data.filter(|ControlField::ContentType(x)| content_types.contains(x))
                    }))
                    .encode(self)?;

                    if let Frame::Control(Control::Start(start)) = self.read_frame()? {
                        if let Some(base) = BaseReader::new(start, self) {
                            Ok(Reader::BiDirectional(base))
                        } else {
                            Err(Error::TodoError)
                        }
                    } else {
                        Err(Error::TodoError)
                    }
                }
                _ => Err(Error::TodoError),
            }
        }
    }

    impl<'a, R: io::Read + io::Write> Build<R> for Builder<'a, R> {}

    impl<'a, R: io::Read + io::Write> Iterator for Reader<'a, R> {
        type Item = <reader::BaseReader<'a, R> as Iterator>::Item;

        fn next(&mut self) -> Option<Self::Item> {
            self.read_mut().next()
        }
    }

    impl<R: io::Read + io::Write> std::ops::Drop for Reader<'_, R> {
        fn drop(&mut self) {
            if let Self::BiDirectional(base) = self {
                let _ = Frame::Control(Control::Finish).encode(base);
            }
        }
    }
}
