use super::prelude::*;

impl<'a, const N: usize> MockSerial<'a, N> {
    /// Create a new mock transport with the given script steps.
    /// If more steps than capacity, extras are dropped (bounded, no panic).
    pub fn new(steps: &[MockStep<'a>]) -> Self {
        let mut script: Deque<MockStep<'a>, N> = Deque::new();
        for &step in steps {
            // If capacity is exceeded, silently drop extras
            let _ = script.push_back(step);
        }
        MockSerial {
            script,
            writes: Vec::new(),
            directions: Vec::new(),
        }
    }
}

impl<'a, const N: usize> SerialTransport for MockSerial<'a, N> {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, IoError> {
        if let Some(step) = self.script.pop_front() {
            match step {
                MockStep::Reply(bytes) | MockStep::Garbage(bytes) => {
                    if bytes.len() > buf.len() {
                        // Buffer too small; copy what fits and return error
                        buf.copy_from_slice(&bytes[..buf.len()]);
                        Err(IoError::ShortTransfer)
                    } else {
                        // Copy all bytes
                        buf[..bytes.len()].copy_from_slice(bytes);
                        Ok(bytes.len())
                    }
                }
                MockStep::Timeout => Err(IoError::Timeout),
                MockStep::ShortTransfer => Err(IoError::ShortTransfer),
                MockStep::DeviceError => Err(IoError::Device),
            }
        } else {
            // Script exhausted
            Err(IoError::Timeout)
        }
    }

    async fn read_until(&mut self, _delimiter: u8, buf: &mut [u8]) -> Result<usize, IoError> {
        // Same script-driven logic as read (delimiter is implicit in the scripted bytes)
        if let Some(step) = self.script.pop_front() {
            match step {
                MockStep::Reply(bytes) | MockStep::Garbage(bytes) => {
                    if bytes.len() > buf.len() {
                        // Buffer would overflow before delimiter; copy what fits
                        buf.copy_from_slice(&bytes[..buf.len()]);
                        Err(IoError::ShortTransfer)
                    } else {
                        // Copy all bytes
                        buf[..bytes.len()].copy_from_slice(bytes);
                        Ok(bytes.len())
                    }
                }
                MockStep::Timeout => Err(IoError::Timeout),
                MockStep::ShortTransfer => Err(IoError::ShortTransfer),
                MockStep::DeviceError => Err(IoError::Device),
            }
        } else {
            // Script exhausted; silent bus
            Err(IoError::Timeout)
        }
    }

    async fn write_all(&mut self, bytes: &[u8]) -> Result<(), IoError> {
        // Append every byte to self.writes (bounded — if full, return error)
        for &byte in bytes {
            if self.writes.push(byte).is_err() {
                // writes Vec is full
                return Err(IoError::Device);
            }
        }
        Ok(())
    }

    async fn set_direction(&mut self, dir: Direction) -> Result<(), IoError> {
        // Push direction (bounded — ignore overflow silently, still Ok)
        let _ = self.directions.push(dir);
        Ok(())
    }
}
