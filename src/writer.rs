use crate::*;
use byteorder::{BigEndian, WriteBytesExt};
use std::io;
pub trait Write: io::Write {
    #[inline]
    fn write_frame_value(&mut self, value: u32) -> std::io::Result<()> {
        self.write_u32::<BigEndian>(value)
    }

    #[inline]
    fn write_frame_len(&mut self, len: FstrmLength) -> std::io::Result<()> {
        self.write_frame_value(len)
    }

    #[inline]
    fn write_frame_header(&mut self, header: FrameHeader) -> std::io::Result<()> {
        match header {
            FrameHeader::ControlHeader => self.write_frame_value(0),
            FrameHeader::Length(length) => self.write_frame_len(length),
        }
    }

    #[inline]
    fn write_frame_data(&mut self, data: &[u8]) -> std::io::Result<()> {
        self.write_all(data)
    }
}
