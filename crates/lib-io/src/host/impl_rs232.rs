use super::prelude::*;

impl SerialTransport for Rs232 {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, IoError> {
        let dur = self.read_timeout;
        match timeout(dur, async {
            self.stream.read(buf).await.map_err(|_| IoError::Device)
        })
        .await
        {
            Ok(inner) => inner,
            Err(_elapsed) => Err(IoError::Timeout),
        }
    }

    async fn read_until(&mut self, delimiter: u8, buf: &mut [u8]) -> Result<usize, IoError> {
        let dur = self.read_timeout;
        match timeout(dur, async {
            let mut pos = 0;
            while pos < buf.len() {
                let mut byte = [0u8; 1];
                match self.stream.read_exact(&mut byte).await {
                    Ok(_) => {
                        buf[pos] = byte[0];
                        pos += 1;
                        if byte[0] == delimiter {
                            return Ok(pos);
                        }
                    }
                    Err(_) => return Err(IoError::Device),
                }
            }
            Err(IoError::ShortTransfer)
        })
        .await
        {
            Ok(inner) => inner,
            Err(_elapsed) => Err(IoError::Timeout),
        }
    }

    async fn write_all(&mut self, bytes: &[u8]) -> Result<(), IoError> {
        let dur = self.read_timeout;
        match timeout(dur, async {
            self.stream
                .write_all(bytes)
                .await
                .map_err(|_| IoError::Device)
        })
        .await
        {
            Ok(inner) => inner,
            Err(_elapsed) => Err(IoError::Timeout),
        }
    }

    async fn set_direction(&mut self, _dir: Direction) -> Result<(), IoError> {
        // RS-232 is full-duplex; direction control is a no-op
        Ok(())
    }

    /// Clears the OS-level input buffer via `tokio-serial`'s `SerialPort::clear`, so a stale
    /// partial frame left over from a failed/timed-out exchange can't bleed into the next
    /// `read_until`.
    async fn discard_input(&mut self) -> Result<(), IoError> {
        self.stream
            .clear(ClearBuffer::Input)
            .map_err(|_| IoError::Device)
    }
}
