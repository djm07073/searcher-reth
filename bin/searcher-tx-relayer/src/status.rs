#[repr(u8)]
#[derive(Copy, Clone, PartialEq)]
pub(crate) enum Status {
    Running = 0,
    Paused = 1,
    Stopped = 2,
}

impl From<u8> for Status {
    fn from(value: u8) -> Self {
        match value {
            0 => Status::Running,
            1 => Status::Paused,
            2 => Status::Stopped,
            _ => unreachable!(),
        }
    }
}
