//! Shared native calling-convention data. These tags do not own runtime objects.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbiType {
    I8,
    I32,
    I64,
    F32,
    F64,
    Ptr,
    Void,
}

pub const TAG_PTR: u64 = 0;
pub const TAG_INT: u64 = 1;
pub const TAG_BOOL: u64 = 2;
pub const TAG_NONE: u64 = 3;
pub const TAG_FUNC: u64 = 4;
pub const TAG_NOTIMPLEMENTED: u64 = 5;
pub const TAG_STOP_ITER: u64 = 6;
pub const TAG_ELLIPSIS: u64 = 7;
pub const NAN_PREFIX: u64 = 0xFFF8_0000_0000_0000;
pub const TAG_MASK: u64 = 0x0007_0000_0000_0000;
pub const TAG_SHIFT: u32 = 48;
pub const PAYLOAD_MASK: u64 = 0x0000_FFFF_FFFF_FFFF;
