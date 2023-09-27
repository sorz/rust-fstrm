use crate::reader::Read;
use crate::writer::Write;
use crate::{Error, FstrmLength, Result, FSTRM_LENGTH_SIZE};
use std::collections::VecDeque;

const FSTRM_CONTROL_ACCEPT: u32 = 0x01;
const FSTRM_CONTROL_START: u32 = 0x02;
const FSTRM_CONTROL_STOP: u32 = 0x03;
const FSTRM_CONTROL_READY: u32 = 0x04;
const FSTRM_CONTROL_FINISH: u32 = 0x05;

const FSTRM_CONTROL_FIELD_CONTENT_TYPE: u32 = 0x01;

const FRAME_LENGTH_MAX: u32 = 512;
const FIELD_CONTENT_TYPE_LENGTH_MAX: u32 = 256;

trait Discriminant<T: Copy> {
    #[inline]
    fn discriminant(&self) -> T {
        unsafe { *<*const _>::from(self).cast::<T>() }
    }

    #[inline]
    fn discriminant_size() -> FstrmLength {
        std::mem::size_of::<T>().try_into().unwrap()
    }
}

#[repr(u32)]
pub enum Control {
    Accept(Data) = FSTRM_CONTROL_ACCEPT,
    Start(Data) = FSTRM_CONTROL_START,
    Stop = FSTRM_CONTROL_STOP,
    Ready(Data) = FSTRM_CONTROL_READY,
    Finish = FSTRM_CONTROL_FINISH,
}

impl Discriminant<u32> for Control {}

#[derive(Debug, Clone)]
pub struct ContentType {
    val: Vec<u8>,
}

impl ContentType {
    #[inline]
    pub fn ref_body(&self) -> &[u8] {
        &self.val
    }

    #[inline]
    pub fn into_inner(self) -> Vec<u8> {
        self.val
    }

    #[inline]
    pub fn len(&self) -> FstrmLength {
        <usize as TryInto<FstrmLength>>::try_into(self.val.len()).unwrap()
    }

    #[inline]
    pub fn new<T: Into<Vec<u8>>>(val: T) -> Self {
        Self { val: val.into() }
    }
}

macro_rules! impl_into_content_type {
    ($x:ty) => {
        impl From<$x> for ContentType {
            #[inline]
            fn from(val: $x) -> Self {
                Self::new(val)
            }
        }
    };
    ($x:ty, $($other:ty),+) => {
        impl_into_content_type! { $x }
        impl_into_content_type! { $($other),+ }
    }
}

impl_into_content_type! { Vec<u8> , &str, String }

impl<T: PartialEq<Vec<u8>>> PartialEq<T> for ContentType {
    #[inline]
    fn eq(&self, other: &T) -> bool {
        other.eq(&self.val)
    }
}

#[repr(u32)]
#[derive(Clone)]
pub enum Field {
    ContentType(ContentType) = FSTRM_CONTROL_FIELD_CONTENT_TYPE,
}

impl Discriminant<u32> for Field {}

impl Field {
    pub fn size(&self) -> FstrmLength {
        Self::discriminant_size() + FSTRM_LENGTH_SIZE + self.body_size()
    }

    pub fn body_size(&self) -> FstrmLength {
        match self {
            Self::ContentType(content_type) => content_type.len(),
        }
    }

    pub fn ref_body(&self) -> &[u8] {
        match self {
            Self::ContentType(content_type) => content_type.ref_body(),
        }
    }

    pub fn decode<R: Read + ?Sized>(reader: &mut R) -> Result<Self> {
        match reader.read_frame_value()? {
            FSTRM_CONTROL_FIELD_CONTENT_TYPE => {
                let len = reader.read_frame_value()?;

                if len > FIELD_CONTENT_TYPE_LENGTH_MAX {
                    return Err(Error::FrameFormatError);
                }

                Ok(Field::ContentType(
                    reader.read_frame_data_for_len(len)?.into(),
                ))
            }
            _ => return Err(Error::FrameFormatError),
        }
    }

    pub fn encode<W: Write + ?Sized>(self, writer: &mut W) -> Result<()> {
        writer.write_frame_value(self.discriminant())?;
        writer.write_frame_len(self.body_size())?;
        writer.write_frame_data(self.ref_body())?;

        Ok(())
    }
}

pub enum Data {
    Empty,
    Field(Field),
    Fields(Vec<Field>),
}

impl Data {
    pub fn size(&self) -> FstrmLength {
        use Data::*;
        match self {
            Empty => 0,
            Field(field) => field.size(),
            Fields(fields) => fields.iter().map(|field| field.size()).sum(),
        }
    }

    pub fn encode<W: Write + ?Sized>(self, writer: &mut W) -> Result<()> {
        match self {
            Self::Empty => Ok(()),
            Self::Field(field) => field.encode(writer),
            Self::Fields(fields) => {
                let mut fields: VecDeque<Field> = fields.into();
                while let Some(field) = fields.pop_front() {
                    field.encode(writer)?;
                }
                Ok(())
            }
        }
    }

    pub fn filter<T: Fn(&Field) -> bool>(&self, f: T) -> Data {
        match self {
            Data::Field(field) if f(field) => Data::Field(field.clone()),
            Data::Fields(fields) => fields
                .as_slice()
                .iter()
                .filter(|x| f(x))
                .map(|x| x.clone())
                .collect::<Vec<Field>>()
                .into(),
            _ => Data::Empty,
        }
    }
}

impl Control {
    pub fn decode<R: Read + ?Sized>(reader: &mut R) -> Result<Self> {
        let mut len = reader.read_frame_value()?;

        if len > FRAME_LENGTH_MAX || len < Self::discriminant_size() {
            reader.consume(len);
            return Err(Error::FrameFormatError);
        }

        let discriminant = reader.read_frame_value()?;
        len -= Self::discriminant_size();
        match discriminant {
            FSTRM_CONTROL_FINISH if len == 0 => Ok(Self::Finish),
            FSTRM_CONTROL_STOP if len == 0 => Ok(Self::Stop),
            FSTRM_CONTROL_START => {
                if len == 0 {
                    Ok(Self::Start(Data::Empty))
                } else if len < Field::discriminant_size() {
                    reader.consume(len);

                    Err(Error::FrameFormatError)
                } else {
                    let field = Field::decode(reader)?;

                    let l = field.size();

                    if l < len {
                        reader.consume(len - l);
                    }

                    if l <= len {
                        Ok(Self::Start(Data::Field(field)))
                    } else {
                        Err(Error::TodoError)
                    }
                }
            }
            FSTRM_CONTROL_ACCEPT | FSTRM_CONTROL_READY => {
                let mut fields = Vec::<Field>::new();

                while len > 0 {
                    if len < Field::discriminant_size() {
                        reader.consume(len);

                        return Err(Error::FrameFormatError);
                    }

                    let field = Field::decode(reader)?;
                    let l = field.size();
                    if l > len {
                        return Err(Error::TodoError);
                    }

                    fields.push(field);
                    len -= l;
                }

                Ok(if discriminant == FSTRM_CONTROL_ACCEPT {
                    Self::Accept(fields.into())
                } else {
                    Self::Ready(fields.into())
                })
            }
            _ => {
                if len > 0 {
                    reader.consume(len);
                }
                Err(Error::FrameFormatError)
            }
        }
    }

    pub fn body_size(&self) -> FstrmLength {
        Self::discriminant_size()
            + match self {
                Self::Accept(fields) | Self::Start(fields) | Self::Ready(fields) => fields.size(),
                _ => 0,
            }
    }

    pub fn size(&self) -> FstrmLength {
        FSTRM_LENGTH_SIZE + self.body_size()
    }

    pub fn encode<W: Write + ?Sized>(self, writer: &mut W) -> Result<()> {
        writer.write_frame_len(self.body_size())?;
        writer.write_frame_value(self.discriminant())?;
        match self {
            Self::Accept(fields) | Self::Start(fields) | Self::Ready(fields) => {
                fields.encode(writer)
            }
            _ => Ok(()),
        }
    }
}

impl From<Data> for Vec<Field> {
    #[inline]
    fn from(data: Data) -> Vec<Field> {
        match data {
            Data::Empty => vec![],
            Data::Field(field) => vec![field],
            Data::Fields(fields) => fields,
        }
    }
}

impl From<Vec<Field>> for Data {
    #[inline]
    fn from(fields: Vec<Field>) -> Data {
        match fields.len() {
            0 => Data::Empty,
            1 => Data::Field(fields.last().unwrap().clone()),
            _ => Data::Fields(fields.into()),
        }
    }
}

impl From<Vec<ContentType>> for Data {
    #[inline]
    fn from(data: Vec<ContentType>) -> Data {
        data.iter()
            .map(|x| Field::ContentType(x.clone()))
            .collect::<Vec<Field>>()
            .into()
    }
}
