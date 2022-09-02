use dallo::SCRATCH_BUF_BYTES;
use rkyv::ser::serializers::{
    BufferScratch, BufferSerializer, CompositeSerializer,
};

pub type StandardBufSerializer<'a> = CompositeSerializer<
    BufferSerializer<&'a mut [u8]>,
    BufferScratch<&'a mut [u8; SCRATCH_BUF_BYTES]>,
>;

pub type Error = Box<dyn std::error::Error>;
