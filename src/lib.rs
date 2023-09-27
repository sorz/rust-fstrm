pub mod control;
pub use control::Control;
pub use control::Data as ControlData;
pub use control::Field as ControlField;

pub mod reader;
pub use reader::Read;
pub mod writer;
pub use writer::Write;

use anyhow;
use thiserror;

pub type Payload = Vec<u8>;
pub type Result<T> = std::result::Result<T, Error>;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("frame format error")]
    FrameFormatError,

    #[error("bad control frame received")]
    ControlFrameError,

    #[error("content_type error")]
    ContentTypeError,

    #[error("todo error")]
    TodoError,

    #[error(transparent)]
    OtherError(#[from] anyhow::Error),
}

macro_rules! impl_from_error {
    ($Large: ty) => {
        impl From<$Large> for Error {
            #[inline]
            fn from(err: $Large) -> Self {
                Self::OtherError(err.into())
            }
        }
    };
}

impl_from_error! {std::io::Error}
impl_from_error! {std::num::TryFromIntError}

pub type FstrmLength = u32;
const FSTRM_LENGTH_SIZE: FstrmLength = 4;

#[derive(Copy, Clone)]
pub enum FrameHeader {
    ControlHeader,
    Length(FstrmLength),
}

impl FrameHeader {
    pub const fn is_length(&self) -> bool {
        matches!(*self, Self::Length(_))
    }

    pub const fn is_control_header(&self) -> bool {
        !self.is_length()
    }
}

pub enum Frame {
    Control(Control),
    Data(Payload),
}

impl Frame {
    pub const fn is_control(&self) -> bool {
        matches!(*self, Self::Control(_))
    }
    pub const fn is_data(&self) -> bool {
        !self.is_control()
    }

    pub fn encode<W: Write + ?Sized>(self, writer: &mut W) -> Result<()> {
        match self {
            Self::Data(payload) => {
                writer
                    .write_frame_header(FrameHeader::Length(payload.len().try_into().unwrap()))?;
                writer.write_frame_data(&payload)?;
            }
            Self::Control(control) => {
                writer.write_frame_header(FrameHeader::ControlHeader)?;

                control.encode(writer)?;
            }
        }

        Ok(())
    }
}
