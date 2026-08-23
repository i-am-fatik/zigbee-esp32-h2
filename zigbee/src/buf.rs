/// Fills a caller-supplied buffer, and remembers if the caller supplied one
/// too small rather than writing past the end of it.
pub struct Writer<'a> {
    buf: &'a mut [u8],
    pos: usize,
    ran_out: bool,
}

impl<'a> Writer<'a> {
    pub fn new(buf: &'a mut [u8]) -> Self {
        Self {
            buf,
            pos: 0,
            ran_out: false,
        }
    }

    pub fn u8(&mut self, v: u8) -> &mut Self {
        self.bytes(&[v])
    }

    pub fn u16(&mut self, v: u16) -> &mut Self {
        self.bytes(&v.to_le_bytes())
    }

    pub fn u32(&mut self, v: u32) -> &mut Self {
        self.bytes(&v.to_le_bytes())
    }

    pub fn u64(&mut self, v: u64) -> &mut Self {
        self.bytes(&v.to_le_bytes())
    }

    pub fn bytes(&mut self, v: &[u8]) -> &mut Self {
        match self.buf.get_mut(self.pos..self.pos + v.len()) {
            Some(room) => {
                room.copy_from_slice(v);
                self.pos += v.len();
            }
            None => self.ran_out = true,
        }
        self
    }

    pub fn len(&self) -> usize {
        self.pos
    }

    /// What was written, or nothing at all when it did not fit. A partial
    /// frame is never worth sending, so the caller cannot reach one.
    pub fn written(&self) -> Option<&[u8]> {
        if self.ran_out {
            return None;
        }
        Some(&self.buf[..self.pos])
    }
}

pub struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    pub fn remaining(&self) -> usize {
        self.buf.len() - self.pos
    }

    pub fn u8(&mut self) -> Option<u8> {
        let v = *self.buf.get(self.pos)?;
        self.pos += 1;
        Some(v)
    }

    pub fn u16(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes(self.array::<2>()?))
    }

    pub fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.array::<4>()?))
    }

    pub fn u64(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(self.array::<8>()?))
    }

    pub fn array<const N: usize>(&mut self) -> Option<[u8; N]> {
        let slice = self.buf.get(self.pos..self.pos + N)?;
        let mut out = [0u8; N];
        out.copy_from_slice(slice);
        self.pos += N;
        Some(out)
    }

    pub fn skip(&mut self, n: usize) -> Option<()> {
        if self.pos + n > self.buf.len() {
            return None;
        }
        self.pos += n;
        Some(())
    }

    pub fn rest(&self) -> &'a [u8] {
        &self.buf[self.pos..]
    }
}

impl<'a> Writer<'a> {
    pub fn set(&mut self, pos: usize, value: u8) {
        match self.buf.get_mut(pos) {
            Some(slot) => *slot = value,
            None => self.ran_out = true,
        }
    }

    pub fn truncate(&mut self, pos: usize) {
        self.pos = pos;
    }
}
